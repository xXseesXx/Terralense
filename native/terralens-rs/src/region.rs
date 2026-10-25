//! TerraFirmaGreg Core Modern 0.9.21 overworld-v1 region generation, ported from its pinned
//! TFC 1.20 task pipeline. Region points use grid coordinates (128 blocks per grid unit).
use crate::{
    biome::{self, area::Area},
    model::{Climate, Settings},
    noise::{Noise2D, OpenSimplex2D, TfgCellular2D, triangle},
    random::Xoroshiro,
};
use lru::LruCache;
use std::{
    cell::RefCell,
    collections::HashSet,
    num::NonZeroUsize,
    rc::Rc,
    sync::{Arc, Condvar, Mutex, OnceLock},
};

pub const GRID_BITS: i32 = 7;
pub const GRID_WIDTH: i32 = 1 << GRID_BITS;
pub const CELL_WIDTH: i32 = 96;
pub const REGION_RADIUS: i32 = 100;
pub const REGION_WIDTH: usize = (2 * REGION_RADIUS + 1) as usize;
pub const PARTITION_BITS: i32 = 5;

// A region stores thousands of generated points, while each river partition can
// retain cloned river polylines. Keep enough nearby work for normal panning and
// recursive neighbor lookups without allowing long zoomed-out sweeps to retain
// the entire explored world in RAM.
const REGION_CACHE_CAPACITY: usize = 32;
const RIVER_PARTITION_CACHE_CAPACITY: usize = 512;
const SHARED_REGION_CACHE_CAPACITY: usize = 128;

fn region_cache() -> LruCache<(i32, i32), Arc<Region>> {
    LruCache::new(NonZeroUsize::new(REGION_CACHE_CAPACITY).unwrap())
}

fn river_partition_cache() -> LruCache<(i32, i32), Rc<Vec<RiverEdge>>> {
    LruCache::new(NonZeroUsize::new(RIVER_PARTITION_CACHE_CAPACITY).unwrap())
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct SharedRegionKey {
    seed: i64,
    settings: [u64; 6],
    cell_x: i32,
    cell_z: i32,
}

impl SharedRegionKey {
    fn new(seed: i64, settings: &Settings, cell_x: i32, cell_z: i32) -> Self {
        Self {
            seed,
            settings: [
                settings.temperature_scale.to_bits(),
                settings.temperature_constant.to_bits(),
                settings.rainfall_scale.to_bits(),
                settings.rainfall_constant.to_bits(),
                settings.continentalness.to_bits(),
                settings.grass_density.to_bits(),
            ],
            cell_x,
            cell_z,
        }
    }
}

type SharedRegionLru = Mutex<LruCache<SharedRegionKey, Arc<Region>>>;
type InFlightRegions = (Mutex<HashSet<SharedRegionKey>>, Condvar);

fn shared_regions() -> &'static SharedRegionLru {
    static CACHE: OnceLock<SharedRegionLru> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(SHARED_REGION_CACHE_CAPACITY).unwrap(),
        ))
    })
}

fn in_flight_regions() -> &'static InFlightRegions {
    static IN_FLIGHT: OnceLock<InFlightRegions> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| (Mutex::new(HashSet::new()), Condvar::new()))
}

struct InFlightRegionGuard(SharedRegionKey);

impl Drop for InFlightRegionGuard {
    fn drop(&mut self) {
        let (keys, ready) = in_flight_regions();
        keys.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.0);
        ready.notify_all();
    }
}

/// Share only completed, immutable region data between worker-local generators.
/// Generation happens outside the global cache lock, and a per-key in-flight set
/// makes concurrent requests for the same region wait for one owner.
fn shared_region(key: SharedRegionKey, generate: impl FnOnce() -> Region) -> Arc<Region> {
    let mut generate = Some(generate);
    loop {
        if let Some(region) = shared_regions()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .cloned()
        {
            return region;
        }

        let (keys, ready) = in_flight_regions();
        let mut active = keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A completed owner may have populated the LRU between our first lookup
        // and taking this lock. Check again before claiming missing-key ownership.
        if let Some(region) = shared_regions()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .cloned()
        {
            return region;
        }
        if active.insert(key) {
            drop(active);
            let guard = InFlightRegionGuard(key);
            let region = Arc::new(generate.take().expect("region generator used once")());
            shared_regions()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .put(key, region.clone());
            drop(guard);
            return region;
        }

        active = ready
            .wait(active)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(active);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RegionPoint {
    pub land: bool,
    pub island: bool,
    pub distance_to_ocean: i8,
    pub distance_to_edge: i8,
    pub temperature: f32,
    pub rainfall: f32,
    pub base_ocean_depth: i8,
    pub base_land_height: i8,
    pub biome_altitude: i8,
    pub mountain: bool,
    pub coastal_mountain: bool,
    pub river: bool,
    pub lake: bool,
    pub biome: u16,
    pub rock: i32,
    pub hotspot_age: i8,
    pub karst: bool,
    pub distance_to_west_coast: i8,
    pub x: i32,
    pub z: i32,
    pub index: usize,
}
impl RegionPoint {
    pub fn shore(&self) -> bool {
        self.distance_to_ocean == -2
    }
    pub fn discrete_biome_altitude(&self) -> i32 {
        (self.biome_altitude as i32).div_euclid(4)
    }
}

#[derive(Clone, Debug)]
pub struct Region {
    pub min_x: i32,
    pub min_z: i32,
    pub max_x: i32,
    pub max_z: i32,
    pub size_x: usize,
    pub size_z: usize,
    pub data: Vec<Option<RegionPoint>>,
    pub rivers: Vec<RiverEdge>,
}
impl Region {
    fn with_radius(cx: i32, cz: i32, radius: i32) -> Self {
        let min_x = cx - radius;
        let min_z = cz - radius;
        let size = (2 * radius + 1) as usize;
        Self {
            min_x,
            min_z,
            max_x: cx + radius,
            max_z: cz + radius,
            size_x: size,
            size_z: size,
            data: vec![None; size * size],
            rivers: vec![],
        }
    }
    #[inline]
    pub fn idx(&self, x: i32, z: i32) -> usize {
        (x - self.min_x) as usize + self.size_x * (z - self.min_z) as usize
    }
    fn in_bounds(&self, x: i32, z: i32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }
    pub fn get(&self, x: i32, z: i32) -> Option<&RegionPoint> {
        if self.in_bounds(x, z) {
            self.data[self.idx(x, z)].as_ref()
        } else {
            None
        }
    }
    pub fn get_mut(&mut self, x: i32, z: i32) -> Option<&mut RegionPoint> {
        if self.in_bounds(x, z) {
            let i = self.idx(x, z);
            self.data[i].as_mut()
        } else {
            None
        }
    }
    pub fn point_at(&self, x: i32, z: i32) -> Option<&RegionPoint> {
        self.get(x, z)
    }
    pub fn point_at_mut(&mut self, x: i32, z: i32) -> Option<&mut RegionPoint> {
        self.get_mut(x, z)
    }
    pub fn offset(&self, i: usize, dx: i32, dz: i32) -> Option<usize> {
        let x = (i % self.size_x) as i32 + dx;
        let z = (i / self.size_x) as i32 + dz;
        if x >= 0 && z >= 0 && x < self.size_x as i32 && z < self.size_z as i32 {
            Some(x as usize + self.size_x * z as usize)
        } else {
            None
        }
    }
    fn shrink(&mut self, min_x: i32, min_z: i32, max_x: i32, max_z: i32) {
        let sx = (max_x - min_x + 1) as usize;
        let sz = (max_z - min_z + 1) as usize;
        let mut data = vec![None; sx * sz];
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                let old = self.idx(x, z);
                let i = (x - min_x) as usize + sx * (z - min_z) as usize;
                data[i] = self.data[old];
            }
        }
        self.min_x = min_x;
        self.min_z = min_z;
        self.max_x = max_x;
        self.max_z = max_z;
        self.size_x = sx;
        self.size_z = sz;
        self.data = data;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RiverEdge {
    pub source: (f64, f64),
    pub drain: (f64, f64),
    pub segments: Vec<(f64, f64)>,
    pub width: u8,
    /// Width of the connected downstream edge after river-width propagation.
    /// Kept on the edge because `rivers_at` merges candidates from partitions,
    /// so `drain_edge` is only an index into its originating region's list.
    pub drain_width: u8,
    pub source_edge: bool,
    pub drain_edge: Option<usize>,
    pub min_part_x: i32,
    pub min_part_z: i32,
    pub max_part_x: i32,
    pub max_part_z: i32,
}

pub use crate::noise::Cell;

fn flood_small_oceans(r: &mut Region) {
    let mut seen = vec![false; r.data.len()];
    let threshold = 180usize;
    for start in 0..r.data.len() {
        if seen[start] || r.data[start].as_ref().is_none_or(|p| p.land) {
            continue;
        }
        let mut q = vec![start];
        let mut head = 0;
        let mut unbounded = false;
        seen[start] = true;
        while head < q.len() {
            let i = q[head];
            head += 1;
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let Some(j) = r.offset(i, dx, dz) else {
                        unbounded = true;
                        continue;
                    };
                    let Some(p) = r.data[j].as_ref() else {
                        unbounded = true;
                        continue;
                    };
                    if p.land || seen[j] {
                        continue;
                    }
                    seen[j] = true;
                    q.push(j);
                }
            }
        }
        if q.len() < threshold && !unbounded {
            for i in q {
                if let Some(p) = r.data[i].as_mut() {
                    p.land = true;
                }
            }
        }
    }
}

fn annotate_distance_edge(r: &mut Region) {
    let mut queue = std::collections::VecDeque::new();
    let mut explored = vec![false; r.data.len()];
    for x in 0..r.size_x {
        for z in 0..r.size_z {
            let i = x + r.size_x * z;
            let boundary = x == 0 || z == 0 || x + 1 == r.size_x || z + 1 == r.size_z;
            if r.data[i].is_none() || boundary {
                explored[i] = true;
                queue.push_back(i);
                if let Some(p) = r.data[i].as_mut() {
                    p.distance_to_edge = -1;
                }
            }
        }
    }
    while let Some(last) = queue.pop_front() {
        let dist = r.data[last]
            .as_ref()
            .map_or(0, |p| p.distance_to_edge as i32 + 1) as i8;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(next) = r.offset(last, dx, dz) else {
                    continue;
                };
                if let Some(p) = r.data[next].as_mut()
                    && p.distance_to_edge == 0
                    && !explored[next]
                {
                    p.distance_to_edge = dist;
                    queue.push_back(next);
                }
                explored[next] = true;
            }
        }
    }
}

fn annotate_distance_ocean(r: &mut Region) {
    let mut queue = std::collections::VecDeque::new();
    let mut explored = vec![false; r.data.len()];
    for x in 0..r.size_x {
        for z in 0..r.size_z {
            let i = x + r.size_x * z;
            if r.data[i].as_ref().is_some_and(|p| !p.land) {
                if let Some(p) = r.data[i].as_mut() {
                    p.distance_to_ocean = -1;
                }
                explored[i] = true;
                queue.push_back(i);
            }
        }
    }
    while let Some(last) = queue.pop_front() {
        let last_land = r.data[last].as_ref().is_some_and(|p| p.land);
        let dist = r.data[last]
            .as_ref()
            .map_or(0, |p| p.distance_to_ocean as i32 + 1) as i8;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(next) = r.offset(last, dx, dz) else {
                    continue;
                };
                let qualifies = r.data[next]
                    .as_ref()
                    .is_some_and(|p| p.land && p.distance_to_ocean == 0);
                if qualifies {
                    let island = r.data[next].as_ref().is_some_and(|p| p.island);
                    if !last_land
                        && !island
                        && let Some(lp) = r.data[last].as_mut()
                    {
                        lp.distance_to_ocean = -2;
                    }
                    if !explored[next] {
                        if let Some(p) = r.data[next].as_mut() {
                            p.distance_to_ocean = dist;
                        }
                        queue.push_back(next);
                    }
                }
                explored[next] = true;
            }
        }
    }
}

#[inline]
fn lerp(d: f32, a: f32, b: f32) -> f32 {
    a + d * (b - a)
}
#[inline]
fn clamped_map(v: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    lerp(((v - a) / (b - a)).clamp(0.0, 1.0), c, d)
}

struct Deque {
    a: Vec<usize>,
    head: usize,
    tail: usize,
}
impl Deque {
    fn new(n: usize) -> Self {
        Self {
            a: vec![0; 2 * n + 1],
            head: n,
            tail: n,
        }
    }
    fn push(&mut self, x: usize) {
        self.a[self.tail] = x;
        self.tail += 1;
    }
    fn first(&mut self, x: usize) {
        self.head -= 1;
        self.a[self.head] = x;
    }
    fn pop(&mut self) -> Option<usize> {
        if self.head >= self.tail {
            None
        } else {
            let x = self.a[self.head];
            self.head += 1;
            Some(x)
        }
    }
}
#[inline]
fn byte(x: i32) -> i8 {
    x as i8
}

fn annotate_base_land_height(r: &mut Region, rng: &mut Xoroshiro) {
    let mut explored = vec![false; r.data.len()];
    let mut q = Deque::new(r.data.len());
    let mut islands = Vec::new();
    for x in 0..r.size_x {
        for z in 0..r.size_z {
            let i = x + r.size_x * z;
            if let Some(p) = r.data[i].as_mut()
                && p.land
            {
                let mut h = p.distance_to_ocean as i32;
                if h > p.distance_to_edge as i32 {
                    h = byte((0.3f32 * h as f32 + 0.7f32 * p.distance_to_edge as f32) as i32)
                        as i32;
                }
                p.base_land_height = h as i8;
                p.base_ocean_depth = if p.island { 3 } else { 0 };
                explored[i] = true;
                if p.island { islands.push(i) } else { q.push(i) }
            }
        }
    }
    while let Some(last) = q.pop() {
        let depth = r.data[last].unwrap().base_ocean_depth as i32 + 1;
        if depth == 3 && !islands.is_empty() {
            for i in islands.drain(..) {
                q.push(i)
            }
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(j) = r.offset(last, dx, dz) else {
                    continue;
                };
                let source = r.data[last].unwrap().base_ocean_depth as i32;
                let mut push_first = false;
                let mut new_depth = byte(depth);
                if let Some(p) = r.data[j].as_mut()
                    && !p.land
                    && p.base_ocean_depth == 0
                    && !explored[j]
                {
                    if rng.next_int_bound(15) == 0 {
                        new_depth = byte(source);
                        push_first = true;
                    }
                    p.base_ocean_depth = new_depth;
                }
                if r.data[j].is_some()
                    && !explored[j]
                    && !r.data[j].unwrap().land
                    && r.data[j].unwrap().base_ocean_depth == new_depth
                {
                    if push_first { q.first(j) } else { q.push(j) }
                }
                explored[j] = true;
            }
        }
    }
}

fn place_mountain_range(r: &Region, rng: &mut Xoroshiro, origin: usize) -> Vec<usize> {
    let mut explored = vec![false; r.data.len()];
    let mut q = Deque::new(r.data.len());
    let mut range = vec![origin];
    q.push(origin);
    explored[origin] = true;
    let height = (r.data[origin].unwrap().base_land_height as i32).max(1);
    let max_size = 70 + rng.next_int_bound(40);
    while let Some(last) = q.pop() {
        let p = r.data[last].unwrap();
        if range.len() > max_size as usize {
            break;
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(j) = r.offset(last, dx, dz) else {
                    continue;
                };
                if let Some(n) = r.data[j]
                    && n.land
                    && n.base_land_height as i32 >= height - 1
                    && n.base_land_height as i32 <= height + 1
                    && (n.base_land_height > 2 || n.distance_to_ocean < 3)
                    && !explored[j]
                {
                    if p.base_land_height != n.base_land_height {
                        q.push(j)
                    } else {
                        q.first(j)
                    }
                    range.push(j);
                }
                explored[j] = true;
            }
        }
    }
    range
}
fn add_mountains(r: &mut Region, rng: &mut Xoroshiro) {
    let mut placed = 0;
    for _ in 0..40 {
        if placed >= 3 {
            break;
        }
        let x = r.min_x + rng.next_int_bound(r.size_x as i32);
        let z = r.min_z + rng.next_int_bound(r.size_z as i32);
        let Some(origin) = r.get(x, z).copied() else {
            continue;
        };
        if origin.land
            && (origin.base_land_height <= 1 || (4..=11).contains(&origin.base_land_height))
        {
            let ids = place_mountain_range(r, rng, r.idx(x, z));
            if ids.len() > 45 {
                for i in ids {
                    let p = r.data[i].as_mut().unwrap();
                    p.mountain = true;
                    if origin.base_land_height <= 2 {
                        p.coastal_mountain = true;
                    }
                }
                placed += 1;
            }
        }
    }
}
fn annotate_biome_altitude(r: &mut Region, rng: &mut Xoroshiro) {
    let mut explored = vec![false; r.data.len()];
    let mut q = Deque::new(r.data.len());
    for x in 0..r.size_x {
        for z in 0..r.size_z {
            let i = x + r.size_x * z;
            if let Some(p) = r.data[i].as_mut()
                && p.land
                && p.mountain
            {
                p.biome_altitude = 12;
                q.push(i);
                explored[i] = true;
            }
        }
    }
    while let Some(last) = q.pop() {
        let p = r.data[last].unwrap();
        let alt = p.biome_altitude as i32 - 1;
        if alt < 0 {
            continue;
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(j) = r.offset(last, dx, dz) else {
                    continue;
                };
                if let Some(n) = r.data[j].as_mut()
                    && n.land
                    && n.biome_altitude == 0
                    && !explored[j]
                {
                    if rng.next_int_bound(13) == 0 && p.biome_altitude != 12 {
                        n.biome_altitude = p.biome_altitude;
                        q.first(j)
                    } else {
                        n.biome_altitude = byte(alt);
                        q.push(j)
                    }
                }
                explored[j] = true;
            }
        }
    }
    for p in r.data.iter_mut().flatten() {
        if p.land && p.discrete_biome_altitude() == 0 && p.base_land_height >= 4 {
            p.biome_altitude = 4;
            if p.base_land_height >= 11 {
                p.biome_altitude = 8;
            }
        }
    }
}

// TFG's hotspot fields (`TFGBiomeNoise.hotSpotIntensity`) sampled by regional tasks.
struct Hotspots {
    plates: TfgCellular2D,
    plates_grid: TfgCellular2D,
    simplex: OpenSimplex2D,
}
impl Hotspots {
    fn new(seed: i64) -> Self {
        let mut plates = TfgCellular2D::new(seed);
        plates.spread((0.00590625f32 / 96.0f32) as f64);
        let mut plates_grid = plates.clone();
        plates_grid.spread(128.0);
        Self {
            plates,
            plates_grid,
            simplex: OpenSimplex2D::new(seed),
        }
    }
    fn active(&self, x: f64, z: f64) -> f64 {
        let mut v = 0.0;
        for i in 0..3 {
            let scale = (1 << i) as f64;
            let n = self.simplex.sample(x * 0.003 / scale, z * 0.003 / scale);
            if n > 0.75 {
                v += (n - 0.75) * 7.2 * 0.5f64.powi(3 - i);
            }
        }
        v
    }
    fn warp(&self, x: f64, z: f64, ux: f64, velocity: f64, accel: f64) -> f64 {
        let uz = (((ux * 16.0).abs() % 1.0 > 0.5) as i32 as f64 * 2.0 - 1.0) * ux * 256.0 % 1.0;
        let vx = (ux + if ux > 0.0 { 1.0 } else { -1.0 }) * velocity;
        let vz = (uz + if uz > 0.0 { 1.0 } else { -1.0 }) * velocity;
        self.active(x + vx - vz * accel, z + vz + vx * accel)
    }
    fn values_block(&self, x: f64, z: f64) -> [f64; 4] {
        let ux = self.plates.cell(x, z).noise;
        [
            self.active(x, z),
            (self.warp(x, z, ux, 1024.0, 0.0) - 0.1).max(0.0) * 1.111,
            (self.warp(x, z, ux, 2048.0, 0.25) - 0.2).max(0.0) * 1.25,
            (self.warp(x, z, ux, 3072.0, 0.5) - 0.3).max(0.0) * 1.4286,
        ]
    }
    fn age(v: [f64; 4]) -> i8 {
        let m = v.into_iter().fold(f64::NEG_INFINITY, f64::max);
        if m <= -0.8 || v.iter().filter(|&&x| x == m).count() != 1 {
            0
        } else {
            (v.iter().position(|&x| x == m).unwrap() + 1) as i8
        }
    }
}

fn annotate_west_coast(r: &mut Region, scale: f64) {
    for x in 0..r.size_x {
        for z in 0..r.size_z {
            let i = x + r.size_x * z;
            let Some(p) = r.data[i].as_ref() else {
                continue;
            };
            if x == 0 {
                continue;
            }
            let point = *p;
            let last = r.data[i - 1].map(|p| p.distance_to_west_coast);
            let value = if last.is_none() {
                if point.land {
                    byte(25 + point.distance_to_ocean as i32)
                } else {
                    0
                }
            } else if !point.land {
                byte((last.unwrap() as i32 - 2).max(0))
            } else {
                let frequency = 128.0f32 / (2.0f32 * scale as f32);
                let fz = point.z as f32;
                let wave = (((4.0f32 * frequency) * fz) - 4.0 * ((frequency * fz + 0.75).floor()))
                    .abs()
                    - 1.0;
                let start = -2 + if wave > 0.1 { 1 } else { 0 };
                let end = 2 - if wave < -0.1 { 1 } else { 0 };
                let mut sum = 0i32;
                let mut count = 0;
                for dz in start..=end {
                    sum += r
                        .get(point.x - 1, point.z + dz)
                        .map_or(last.unwrap() as i32, |a| a.distance_to_west_coast as i32);
                    count += 1;
                }
                byte(((sum as f32 / count as f32).ceil() as i32) + 1)
            };
            if let Some(p) = r.data[i].as_mut() {
                p.distance_to_west_coast = value;
            }
        }
    }
    let mut seen = vec![false; r.data.len()];
    let mut q = std::collections::VecDeque::new();
    for x in 0..r.size_x {
        for z in 0..r.size_z {
            let i = x + r.size_x * z;
            if r.data[i].is_some_and(|p| p.land) {
                seen[i] = true;
                q.push_back(i);
            }
        }
    }
    while let Some(i) = q.pop_front() {
        let last = r.data[i].unwrap().distance_to_west_coast as i32;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(j) = r.offset(i, dx, dz) else {
                    continue;
                };
                if r.data[j].is_some_and(|p| p.distance_to_west_coast == 0) {
                    if !seen[j] {
                        let v = byte(last + if last > 40 { -1 } else { 1 });
                        r.data[j].as_mut().unwrap().distance_to_west_coast = v;
                        q.push_back(j);
                    }
                    seen[j] = true;
                }
            }
        }
    }
}

fn tfg_islands(r: &mut Region, rng: &mut Xoroshiro) {
    let mut placed = 0;
    for _ in 0..130 {
        if placed >= 15 {
            break;
        }
        let index = rng.next_int_bound(r.data.len() as i32) as usize;
        let Some(origin) = r.data[index] else {
            continue;
        };
        if origin.land || origin.shore() || origin.distance_to_edge <= 2 {
            continue;
        }
        let mut point = Some(origin);
        for _ in 0..12 {
            let Some(p) = point else { break };
            if let Some(m) = r.data.get_mut(p.index).and_then(Option::as_mut) {
                m.land = true;
                m.island = true;
            }
            let x = origin.x + rng.next_int_bound(4) - rng.next_int_bound(4);
            let z = origin.z + rng.next_int_bound(4) - rng.next_int_bound(4);
            point = r.get(x, z).copied();
            if point.is_none_or(|p| (p.land && !p.island) || p.distance_to_edge <= 2) {
                break;
            }
        }
        // Upstream counts an accepted origin even if its twelve-step chain stops early.
        placed += 1;
    }
}

fn add_hotspots(r: &mut Region, hotspots: &Hotspots) {
    let mut queue = Vec::new();
    let mut intensity = vec![0.0; r.data.len()];
    for (index, p) in r.data.iter_mut().enumerate() {
        let Some(p) = p else { continue };
        let cell = hotspots.plates_grid.cell(p.x as f64, p.z as f64);
        let values = hotspots.values_block((p.x as f64 + 0.5) * 128.0, (p.z as f64 + 0.5) * 128.0);
        let v = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        intensity[index] = v;
        if v > 0.65 && (cell.f1 - cell.f2).abs() > 0.05 {
            p.hotspot_age = Hotspots::age(values);
            if p.hotspot_age != 4 {
                p.land = true;
            }
            queue.push(index);
        }
    }
    let mut head = 0;
    while head < queue.len() {
        let i = queue[head];
        head += 1;
        let age = r.data[i].unwrap().hotspot_age;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(j) = r.offset(i, dx, dz) else {
                    continue;
                };
                let Some(n) = r.data[j] else { continue };
                if n.hotspot_age != 0 {
                    continue;
                }
                if intensity[j] > 0.15 {
                    queue.push(j);
                    let p = r.data[j].as_mut().unwrap();
                    p.hotspot_age = age;
                    if age != 4 {
                        p.land = true;
                    }
                } else if !n.land
                    && hotspots
                        .values_block(
                            (n.x as f64 + 0.5 - dx as f64) * 128.0,
                            (n.z as f64 + 0.5 - dz as f64) * 128.0,
                        )
                        .iter()
                        .copied()
                        .fold(f64::NEG_INFINITY, f64::max)
                        > 0.15
                {
                    let p = r.data[j].as_mut().unwrap();
                    p.hotspot_age = age;
                    if age != 4 {
                        p.land = true;
                    }
                }
            }
        }
    }
}

/// Native implementation of Core Modern 0.9.21 overworld v1's region generator. The cache is
/// deterministic and bounded; it contains only completed regions. `Settings` are read exactly as
/// the TFC RegionGenerator constructor does.
// A partition is 3 grid units and a cell is 96, hence 32x32 partition bins.
// Index once per nearby cell instead of scanning nine regions for every map pixel.
struct RiverNeighborhood {
    regions: Vec<Arc<Region>>,
    bins: Vec<Vec<(usize, usize)>>,
    part_x: i32,
    part_z: i32,
}
impl RiverNeighborhood {
    fn new(cx: i32, cz: i32, regions: Vec<Arc<Region>>) -> Self {
        const SIDE: usize = (CELL_WIDTH / 3) as usize;
        let part_x = cx * SIDE as i32;
        let part_z = cz * SIDE as i32;
        let mut bins = vec![Vec::new(); SIDE * SIDE];
        for (region_index, region) in regions.iter().enumerate() {
            for (edge_index, edge) in region.rivers.iter().enumerate() {
                let x0 = edge.min_part_x.max(part_x);
                let x1 = edge.max_part_x.min(part_x + SIDE as i32 - 1);
                let z0 = edge.min_part_z.max(part_z);
                let z1 = edge.max_part_z.min(part_z + SIDE as i32 - 1);
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        bins[(z - part_z) as usize * SIDE + (x - part_x) as usize]
                            .push((region_index, edge_index));
                    }
                }
            }
        }
        Self {
            regions,
            bins,
            part_x,
            part_z,
        }
    }
    fn edges(&self, px: i32, pz: i32) -> Vec<RiverEdge> {
        const SIDE: usize = (CELL_WIDTH / 3) as usize;
        let x = px - self.part_x;
        let z = pz - self.part_z;
        debug_assert!((0..SIDE as i32).contains(&x) && (0..SIDE as i32).contains(&z));
        self.bins[z as usize * SIDE + x as usize]
            .iter()
            .map(|&(region, edge)| self.regions[region].rivers[edge].clone())
            .collect()
    }
}

pub struct RegionCore {
    pub seed: i64,
    pub settings: Settings,
    region_seed: i64,
    cell_noise: TfgCellular2D,
    continent_noise: Noise2D,
    temperature_noise: Noise2D,
    rainfall_noise: Noise2D,
    ocean_noise: OpenSimplex2D,
    biome_area: Area,
    rock_area: Area,
    rock_layer: Area,
    forest_layer: Area,
    hotspots: Hotspots,
    layer_height_noise: OpenSimplex2D,
    skew_x: OpenSimplex2D,
    skew_z: OpenSimplex2D,
    regions: RefCell<LruCache<(i32, i32), Arc<Region>>>,
    river_partitions: RefCell<LruCache<(i32, i32), Rc<Vec<RiverEdge>>>>,
    river_neighborhoods: RefCell<LruCache<(i32, i32), Rc<RiverNeighborhood>>>,
}
pub struct Generator {
    core: Rc<RegionCore>,
    biome_layer: Area,
}
impl std::ops::Deref for Generator {
    type Target = RegionCore;
    fn deref(&self) -> &RegionCore {
        &self.core
    }
}
impl Generator {
    pub fn new(world_seed: i64, settings: Settings) -> Self {
        let mut random = Xoroshiro::new(world_seed);
        let region_gen_seed = random.next_long();
        let mut cell_noise = TfgCellular2D::new(random.next_long());
        cell_noise.spread((1.0f32 / 96.0f32) as f64);
        let continent_min = settings.continentalness as f32 * 10.0f32 - 2.5f32;
        let cbase_cell = cell_noise.clone();
        let continent_base: Noise2D = Rc::new(move |x, z| {
            let c = cbase_cell.cell(x, z);
            1.0 - c.f1 / (0.37f32 as f64 + c.f2)
        });
        let mut continent_simplex = OpenSimplex2D::new(random.next_long());
        continent_simplex
            .spread(0.24f32 as f64)
            .scaled(continent_min as f64, 8.7f32 as f64)
            .octaves(4);
        let continent_noise = Rc::new(move |x, z| {
            let a = continent_base(x, z);
            if a == 0.0 {
                0.0
            } else {
                a * continent_simplex.sample(x, z)
            }
        });
        let temp_scale = (2.0 * settings.temperature_scale) as f32;
        let temp_frequency = 128.0f32 / temp_scale;
        let temp_constant = settings.temperature_constant;
        let temp_enabled = settings.temperature_scale != 0.0;
        let temp_simplex_seed = random.next_int();
        let mut temp_simplex = OpenSimplex2D::from_int(temp_simplex_seed);
        temp_simplex
            .octaves(2)
            .spread(0.15f32 as f64)
            .scaled(-3.0, 3.0);
        let temperature_noise: Noise2D = Rc::new(move |x, z| {
            let base = if !temp_enabled {
                temp_constant
            } else {
                triangle(1.0, 0.0, temp_frequency as f64, z)
            };
            // Noise2D::scaled(-20, 30) maps [-1, 1] to [-20, 30].
            base * 25.0 + 5.0 + temp_simplex.sample(x, z)
        });
        let rain_scale = (2.0 * settings.rainfall_scale) as f32;
        let rain_frequency = 128.0f32 / rain_scale;
        let rain_constant = settings.rainfall_constant;
        let rain_enabled = settings.rainfall_scale != 0.0;
        let rain_simplex_seed = random.next_int();
        let mut rain_simplex = OpenSimplex2D::from_int(rain_simplex_seed);
        rain_simplex
            .octaves(2)
            .spread(0.15f32 as f64)
            .scaled(-80.0, 40.0);
        let rainfall_noise: Noise2D = Rc::new(move |x, z| {
            let base = if !rain_enabled {
                rain_constant
            } else {
                triangle(1.0, 0.0, rain_frequency as f64, x)
            };
            (base * 250.0 + 250.0) + rain_simplex.sample(x, z)
        });
        let biome_area = biome::area::uniform_area(&mut random, 2);
        let rock_area = biome::area::uniform_area(&mut random, 3);
        let chunk_data_seed = random.next_long();
        let biome_layer_seed = random.next_long();
        let mut chunk_random = Xoroshiro::new(chunk_data_seed);
        let reseed = chunk_data_seed ^ chunk_random.next_long();
        chunk_random.set_seed(reseed);
        let rock_seed = chunk_random.next_long();
        let mut layer_height_noise = OpenSimplex2D::from_int(chunk_random.next_int());
        layer_height_noise
            .octaves(3)
            .scaled(43.0, 63.0)
            .spread(0.014f32 as f64);
        let mut skew_x = OpenSimplex2D::from_int(chunk_random.next_int());
        skew_x
            .octaves(2)
            .scaled(-1.8f32 as f64, 1.8f32 as f64)
            .spread(0.01f32 as f64);
        let mut skew_z = OpenSimplex2D::from_int(chunk_random.next_int());
        skew_z
            .octaves(2)
            .scaled(-1.8f32 as f64, 1.8f32 as f64)
            .spread(0.01f32 as f64);
        let forest_seed = chunk_random.next_long();
        let forest_layer = biome::forest::overworld_forest_layer(forest_seed);
        let mut ocean_noise = OpenSimplex2D::new(world_seed);
        ocean_noise.spread(0.02f32 as f64);
        let core = Rc::new_cyclic(|weak: &std::rc::Weak<RegionCore>| {
            let rock_core = weak.clone();
            let rock_source: Area = Rc::new(move |x, z| {
                rock_core
                    .upgrade()
                    .map_or(0, |core| core.region_point(x, z).rock)
            });
            let rock_layer = biome::layers::overworld_rock_layer(rock_seed, rock_source);
            RegionCore {
                seed: world_seed,
                settings,
                region_seed: region_gen_seed,
                cell_noise,
                continent_noise,
                temperature_noise,
                rainfall_noise,
                ocean_noise,
                biome_area,
                rock_area,
                rock_layer,
                forest_layer,
                hotspots: Hotspots::new(world_seed),
                layer_height_noise,
                skew_x,
                skew_z,
                regions: RefCell::new(region_cache()),
                river_partitions: RefCell::new(river_partition_cache()),
                river_neighborhoods: RefCell::new(LruCache::new(NonZeroUsize::new(4).unwrap())),
            }
        });
        let biome_core = Rc::downgrade(&core);
        let biome_source: Area = Rc::new(move |x, z| {
            biome_core
                .upgrade()
                .map_or(0, |core| core.region_point(x, z).biome as i32)
        });
        let biome_layer = biome::layers::region_biome_layer(biome_layer_seed, biome_source);
        Self { core, biome_layer }
    }
}
impl RegionCore {
    fn cell(&self, x: i32, z: i32) -> Cell {
        self.cell_noise.cell(x as f64, z as f64)
    }
    fn get_region(&self, gx: i32, gz: i32) -> Arc<Region> {
        let c = self.cell(gx, gz);
        let key = (c.cx, c.cy);
        let cached = { self.regions.borrow_mut().get(&key).cloned() };
        if let Some(r) = cached {
            return r;
        }
        let shared_key = SharedRegionKey::new(self.seed, &self.settings, c.cx, c.cy);
        let r = shared_region(shared_key, || self.create_region(c));
        self.regions.borrow_mut().put(key, r.clone());
        r
    }
    fn create_region(&self, c: Cell) -> Region {
        let cx = fast_round(c.x);
        let cz = fast_round(c.y);
        // A nearest jittered node is at most (sqrt(2)/2 + 0.437016)*96
        // grid units away. 112 also covers rounding the center to an integer.
        // The old 100-radius box can omit real owning points in tilted cells.
        let mut radius = REGION_RADIUS + 12;
        let (mut full, min_x, min_z, max_x, max_z) = loop {
            let mut full = Region::with_radius(cx, cz, radius);
            let mut min_x = i32::MAX;
            let mut min_z = i32::MAX;
            let mut max_x = i32::MIN;
            let mut max_z = i32::MIN;
            let mut touches_boundary = false;
            for dx in 0..full.size_x as i32 {
                for dz in 0..full.size_z as i32 {
                    let x = full.min_x + dx;
                    let z = full.min_z + dz;
                    let other = self.cell(x, z);
                    if other.x == c.x && other.y == c.y {
                        let index = full.idx(x, z);
                        full.data[index] = Some(RegionPoint {
                            x,
                            z,
                            index,
                            ..RegionPoint::default()
                        });
                        min_x = min_x.min(x);
                        min_z = min_z.min(z);
                        max_x = max_x.max(x);
                        max_z = max_z.max(z);
                        touches_boundary |= dx == 0
                            || dz == 0
                            || dx == full.size_x as i32 - 1
                            || dz == full.size_z as i32 - 1;
                    }
                }
            }
            if !touches_boundary {
                break (full, min_x, min_z, max_x, max_z);
            }
            // Rare tilted Voronoi cells extend beyond the usual 100-grid storage
            // radius. Complete that cell before running tasks instead of truncating
            // it and later losing tile workers on a missing point.
            radius += REGION_RADIUS / 2;
        };
        full.shrink(min_x, min_z, max_x, max_z);
        for i in 0..full.data.len() {
            let x = full.min_x + (i % full.size_x) as i32;
            let z = full.min_z + (i / full.size_x) as i32;
            if let Some(p) = full.data[i].as_mut() {
                p.index = i;
                p.land = (self.continent_noise)(x as f64, z as f64) > 4.4;
            }
        }
        annotate_distance_edge(&mut full);
        flood_small_oceans(&mut full);
        let mut rng = Xoroshiro::new(region_seed(self.region_seed, c));
        tfg_islands(&mut full, &mut rng);
        add_hotspots(&mut full, &self.hotspots);
        annotate_distance_ocean(&mut full);
        annotate_base_land_height(&mut full, &mut rng);
        annotate_west_coast(&mut full, self.settings.temperature_scale);
        add_mountains(&mut full, &mut rng);
        annotate_biome_altitude(&mut full, &mut rng);
        let temp = self.temperature_noise.clone();
        let rain = self.rainfall_noise.clone();
        let ocean = self.ocean_noise.clone();
        for p in full.data.iter_mut().flatten() {
            let x = p.x as f64;
            let z = p.z as f64;
            p.temperature = (temp)(x, z) as f32;
            p.rainfall = (rain)(x, z) as f32;
            let bias = if p.land {
                clamped_map(p.distance_to_edge as f32, 2., 6., 0., 1.).min(clamped_map(
                    p.distance_to_ocean as f32,
                    2.,
                    6.,
                    0.,
                    1.,
                ))
            } else {
                0.
            };
            let target_temp = lerp(bias, 5., p.temperature);
            let target_rain = lerp(bias, (p.rainfall + 350.).min(500.), p.rainfall);
            let delta = clamped_map(ocean.sample(x, z) as f32, -0.8, 0.9, -0.07, 0.23);
            let old = p.temperature;
            p.temperature = lerp(delta, old, target_temp);
            let rain_delta = clamped_map(p.temperature - old, -2., 2., 0., 0.25);
            p.rainfall = lerp(rain_delta, p.rainfall, target_rain).clamp(0., 500.);
        }
        // TFGChooseRocks precedes TFGChooseBiomes in task order.
        for i in 0..full.data.len() {
            let Some(center) = full.data[i] else { continue };
            let mut kind = if center.land { 2 } else { 0 };
            let mut min_dist = i32::MAX;
            for dx in -2..=2 {
                for dz in -2..=2 {
                    let Some(j) = full.offset(i, dx, dz) else {
                        continue;
                    };
                    let Some(n) = full.data[j] else { continue };
                    let dist = dx.abs() + dz.abs();
                    if dist < min_dist {
                        if (n.island && dist < 4) || n.hotspot_age > 0 {
                            kind = 1;
                            min_dist = dist;
                        } else if (n.mountain || n.coastal_mountain) && dist < 3 {
                            kind = 3;
                            min_dist = dist;
                        }
                    }
                }
            }
            let area_seed = (self.rock_area)(center.x, center.z);
            full.data[i].as_mut().unwrap().rock = area_seed.wrapping_shl(2) | kind;
        }
        for p in full.data.iter_mut().flatten() {
            p.karst = matches!(
                crate::geology::sample_rock_layer(p.rock, 0).as_str(),
                "limestone" | "dolomite" | "chalk" | "marble"
            );
        }
        let seed = rng.next_long();
        let climate_seed = rng.next_long();
        for p in full.data.iter_mut().flatten() {
            let a = (self.biome_area)(p.x, p.z);
            biome::choose::choose_biome(p, seed, climate_seed, a);
        }
        crate::rivers::add_rivers_and_lakes(&mut full, &mut rng);
        full
    }
    fn region_point(&self, gx: i32, gz: i32) -> RegionPoint {
        let r = self.get_region(gx, gz);
        *r.get(gx,gz).unwrap_or_else(|| {
            let c=self.cell(gx,gz);
            panic!("region lookup missing grid=({gx},{gz}) seed={} cell=({},{}) center=({},{}) bounds=({},{})..({},{}) size={}x{}",self.seed,c.cx,c.cy,c.x,c.y,r.min_x,r.min_z,r.max_x,r.max_z,r.size_x,r.size_z)
        })
    }
}
impl Generator {
    pub fn region_point(&self, gx: i32, gz: i32) -> RegionPoint {
        self.core.region_point(gx, gz)
    }
    pub fn biome_at_quart(&self, qx: i32, qz: i32) -> u16 {
        let id = (self.biome_layer)(qx, qz) as u16;
        if !biome::ids::has_river(id) {
            return id;
        }
        let gx = qx.div_euclid(32);
        let gz = qz.div_euclid(32);
        if self.rivers_at(gx, gz).iter().any(|e| {
            crate::rivers::river_distance_sq(e, qx as f64 / 32.0, qz as f64 / 32.0)
                < (0.08f32 as f64).powi(2)
        }) {
            biome::ids::id::RIVER
        } else {
            id
        }
    }
    /// The underlying TFG quart biome before the separate river overlay.
    /// Surface height sampling uses this and applies river carving independently.
    pub fn biome_at_quart_no_river(&self, qx: i32, qz: i32) -> u16 {
        (self.biome_layer)(qx, qz) as u16
    }
    pub fn biome(&self, x: i32, z: i32) -> u16 {
        self.biome_at_quart(x >> 2, z >> 2)
    }
    pub fn climate(&self, x: i32, z: i32) -> Climate {
        let ox = (x >> 4) << 4;
        let oz = (z >> 4) << 4;
        let gx = ox >> 7;
        let gz = oz >> 7;
        let p00 = self.region_point(gx, gz);
        let p01 = self.region_point(gx, gz + 1);
        let p10 = self.region_point(gx + 1, gz);
        let p11 = self.region_point(gx + 1, gz + 1);
        let dx = ox as f64 / 128.0 - gx as f64;
        let dz = oz as f64 / 128.0 - gz as f64;
        let temp = climate_corners(
            p00.temperature,
            p01.temperature,
            p10.temperature,
            p11.temperature,
            dx,
            dz,
        );
        let mut rain = climate_corners(
            p00.rainfall,
            p01.rainfall,
            p10.rainfall,
            p11.rainfall,
            dx,
            dz,
        );
        for edge in self.rivers_at(gx, gz).iter() {
            if edge.width < 12
                || !crate::rivers::river_maybe_intersects(
                    edge,
                    ox as f64 / 128.0,
                    oz as f64 / 128.0,
                    0.3125,
                )
            {
                continue;
            }
            let width = (edge.width as f32 - 12.0f32) / 12.0f32;
            for (idx, (cx, cz)) in [(ox, oz), (ox, oz + 16), (ox + 16, oz), (ox + 16, oz + 16)]
                .into_iter()
                .enumerate()
            {
                let d = crate::rivers::river_distance_sq(edge, cx as f64 / 128.0, cz as f64 / 128.0)
                    as f32;
                let influence = clamped_map(d, 0.0, 0.3125f32 * 0.3125f32, 1.0, 0.0);
                rain[idx] = rain[idx].max(influence * width * 300.0f32);
            }
        }
        rain = rain.map(|v| v.clamp(0.0, 500.0));
        Climate {
            temperature: climate_value(temp, (x & 15) as f64 / 16.0, (z & 15) as f64 / 16.0),
            rainfall: climate_value(rain, (x & 15) as f64 / 16.0, (z & 15) as f64 / 16.0),
        }
    }
    pub fn forest_type(&self, cx: i32, cz: i32) -> u8 {
        (self.forest_layer)(cx, cz) as u8
    }
    pub fn rock_category(&self, x: i32, z: i32) -> i32 {
        (self.rock_layer)(x, z)
    }
    pub fn rock_point_at(&self, x: i32, z: i32) -> i32 {
        (self.rock_layer)(x, z)
    }
    pub fn surface_rock_category(&self, x: i32, z: i32) -> i32 {
        let offsets = rock_offsets();
        let ox = x + offsets[0];
        let oz = z + offsets[1];
        let sx = (self.skew_x.sample(ox as f64, oz as f64) as f32 * 12.) as i32;
        let sz = (self.skew_z.sample(ox as f64, oz as f64) as f32 * 12.) as i32;
        (self.rock_layer)(x + sx, z + sz)
    }
    pub fn surface_rock_at(&self, x: i32, z: i32) -> i32 {
        self.surface_rock_category(x, z)
    }
    pub fn rock_layer_at(&self, x: i32, y: i32, z: i32, surface_y: i32) -> (i32, usize) {
        let adjusted = if surface_y > 125 {
            125.0f32 + 0.3f32 * (surface_y - 125) as f32
        } else {
            surface_y as f32
        };
        let mut layer = 0usize;
        let mut dy = adjusted - y as f32;
        loop {
            let offsets = rock_offsets();
            let lx = x + offsets[(layer & 7) * 2];
            let lz = z + offsets[(layer & 7) * 2 + 1];
            let h = self.layer_height_noise.sample(lx as f64, lz as f64) as f32;
            if dy <= h {
                break;
            }
            dy -= h;
            layer += 1;
            if dy <= 0.0 {
                break;
            }
        }
        let o = rock_offsets();
        let lx = x + o[(layer & 7) * 2];
        let lz = z + o[(layer & 7) * 2 + 1];
        let sx = (self.skew_x.sample(lx as f64, lz as f64) as f32 * (dy + 12.)) as i32;
        let sz = (self.skew_z.sample(lx as f64, lz as f64) as f32 * (dy + 12.)) as i32;
        ((self.rock_layer)(x + sx, z + sz), layer)
    }
    pub fn hotspot_at(&self, x: i32, z: i32) -> (f64, i8) {
        let v = self.hotspots.values_block(x as f64, z as f64);
        (
            v.into_iter().fold(f64::NEG_INFINITY, f64::max),
            Hotspots::age(v),
        )
    }
    pub fn hotspot(&self, x: i32, z: i32) -> (f64, i8) {
        self.hotspot_at(x, z)
    }
    pub fn rivers_at(&self, gx: i32, gz: i32) -> Rc<Vec<RiverEdge>> {
        self.core.rivers_at(gx, gz)
    }
    pub fn river_edges_at(&self, x: i32, z: i32) -> Vec<RiverEdge> {
        self.core
            .rivers_at(x.div_euclid(128), z.div_euclid(128))
            .as_ref()
            .clone()
    }
}
impl RegionCore {
    fn rivers_at(&self, gx: i32, gz: i32) -> Rc<Vec<RiverEdge>> {
        let px = gx.div_euclid(3);
        let pz = gz.div_euclid(3);
        let key = (px, pz);
        if let Some(edges) = self.river_partitions.borrow_mut().get(&key).cloned() {
            return edges;
        }
        let cx = gx.div_euclid(CELL_WIDTH);
        let cz = gz.div_euclid(CELL_WIDTH);
        let cell = (cx, cz);
        let cached = { self.river_neighborhoods.borrow_mut().get(&cell).cloned() };
        let neighborhood = cached.unwrap_or_else(|| {
            let mut regions = Vec::with_capacity(9);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    regions.push(self.get_region((cx + dx) * CELL_WIDTH, (cz + dz) * CELL_WIDTH));
                }
            }
            let index = Rc::new(RiverNeighborhood::new(cx, cz, regions));
            self.river_neighborhoods
                .borrow_mut()
                .put(cell, index.clone());
            index
        });
        let edges = Rc::new(neighborhood.edges(px, pz));
        self.river_partitions.borrow_mut().put(key, edges.clone());
        edges
    }
}
fn region_seed(seed: i64, c: Cell) -> i64 {
    let bits = (c.noise as f32).to_bits() as i32 as i64;
    seed ^ bits.wrapping_mul(7_189_234_123)
}
fn climate_corners(v00: f32, v01: f32, v10: f32, v11: f32, dx: f64, dz: f64) -> [f32; 4] {
    let lerp = |d: f64, a: f64, b: f64| a + (b - a) * d;
    let lerp4 = |x: f64, z: f64| {
        let a = lerp(z, v00 as f64, v01 as f64);
        let b = lerp(z, v10 as f64, v11 as f64);
        lerp(x, a, b) as f32
    };
    [
        lerp4(dx, dz),
        lerp4(dx, dz + 0.125),
        lerp4(dx + 0.125, dz),
        lerp4(dx + 0.125, dz + 0.125),
    ]
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn empty_region() -> Region {
        Region {
            min_x: 0,
            min_z: 0,
            max_x: -1,
            max_z: -1,
            size_x: 0,
            size_z: 0,
            data: Vec::new(),
            rivers: Vec::new(),
        }
    }

    #[test]
    fn region_cache_is_bounded_lru_and_eviction_keeps_live_region_references() {
        let mut cache = region_cache();
        for i in 0..REGION_CACHE_CAPACITY {
            cache.put((i as i32, 0), Arc::new(empty_region()));
        }
        let retained = cache.get(&(0, 0)).unwrap().clone();
        cache.put((REGION_CACHE_CAPACITY as i32, 0), Arc::new(empty_region()));

        assert_eq!(cache.len(), REGION_CACHE_CAPACITY);
        assert!(
            cache.peek(&(0, 0)).is_some(),
            "recently used region stays cached"
        );
        assert!(
            cache.peek(&(1, 0)).is_none(),
            "least recently used region is evicted"
        );
        assert_eq!(
            retained.size_x, 0,
            "an active Arc remains valid after eviction"
        );
    }

    #[test]
    fn river_partition_cache_is_bounded_lru() {
        let mut cache = river_partition_cache();
        for i in 0..RIVER_PARTITION_CACHE_CAPACITY {
            cache.put((i as i32, 0), Rc::new(Vec::new()));
        }
        let retained = cache.get(&(0, 0)).unwrap().clone();
        cache.put(
            (RIVER_PARTITION_CACHE_CAPACITY as i32, 0),
            Rc::new(Vec::new()),
        );

        assert_eq!(cache.len(), RIVER_PARTITION_CACHE_CAPACITY);
        assert!(cache.peek(&(0, 0)).is_some());
        assert!(cache.peek(&(1, 0)).is_none());
        assert!(retained.is_empty());
    }

    #[test]
    fn generators_with_same_full_world_key_share_immutable_regions() {
        let seed = 0x51_7a_9b_3d_2c_1e_i64;
        let settings = Settings::default();
        let first = Generator::new(seed, settings.clone());
        let second = Generator::new(seed, settings.clone());
        let a = first.core.get_region(0, 0);
        let b = second.core.get_region(0, 0);
        assert!(Arc::ptr_eq(&a, &b));

        let mut changed = settings;
        changed.grass_density += 0.125;
        let other_world = Generator::new(seed, changed);
        let c = other_world.core.get_region(0, 0);
        assert!(
            !Arc::ptr_eq(&a, &c),
            "every Settings field participates in the key"
        );

        let other_seed = Generator::new(seed.wrapping_add(1), Settings::default());
        let d = other_seed.core.get_region(0, 0);
        assert!(!Arc::ptr_eq(&a, &d), "seed participates in the key");
    }

    #[test]
    fn rare_tilted_cell_is_complete_before_cached_region_tasks_run() {
        let generator = Generator::new(7787531943268589495, Settings::default());
        // This cell extends 101.45 grid units east of its jittered center.
        // Loading its center first used to cache a truncated region and panic later.
        let first = generator.core.get_region(-1785, -4783);
        assert!(first.max_x >= -1684);
        let point = generator.region_point(-1684, -4752);
        assert_eq!((point.x, point.z), (-1684, -4752));
        for z in -4756..=-4748 {
            for x in -1687..=-1681 {
                let point = generator.region_point(x, z);
                assert_eq!((point.x, point.z), (x, z));
            }
        }
        let second = generator.core.get_region(-1684, -4752);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn indexed_river_partitions_match_the_original_scan_at_signed_cell_boundaries() {
        let generator = Generator::new(7787531943268589495, Settings::default());
        let mut matched_edges = 0;
        for (cx, cz) in [(-1, -1), (0, 0), (1, -1)] {
            let mut regions = Vec::new();
            for dx in -1..=1 {
                for dz in -1..=1 {
                    regions.push(
                        generator
                            .core
                            .get_region((cx + dx) * CELL_WIDTH, (cz + dz) * CELL_WIDTH),
                    );
                }
            }
            let index = RiverNeighborhood::new(cx, cz, regions.clone());
            for z in 0..CELL_WIDTH / 3 {
                for x in 0..CELL_WIDTH / 3 {
                    let px = cx * (CELL_WIDTH / 3) + x;
                    let pz = cz * (CELL_WIDTH / 3) + z;
                    let expected: Vec<_> = regions
                        .iter()
                        .flat_map(|r| r.rivers.iter())
                        .filter(|e| {
                            e.min_part_x <= px
                                && e.max_part_x >= px
                                && e.min_part_z <= pz
                                && e.max_part_z >= pz
                        })
                        .cloned()
                        .collect();
                    matched_edges += expected.len();
                    assert_eq!(index.edges(px, pz), expected, "partition {px},{pz}");
                    assert_eq!(
                        generator.rivers_at(px * 3 + 1, pz * 3 + 1).as_ref(),
                        &expected
                    );
                }
            }
        }
        assert!(
            matched_edges > 0,
            "comparison must include populated river bins"
        );
        assert!(generator.core.river_neighborhoods.borrow().len() <= 4);
    }

    #[test]
    fn concurrent_same_key_region_requests_share_one_generation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;
        use std::time::Duration;

        let key = SharedRegionKey {
            seed: i64::MIN + 42,
            settings: [17, 23, 31, 47, 59, 71],
            cell_x: i32::MIN + 8,
            cell_z: i32::MAX - 8,
        };
        let generated = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let generated = generated.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    shared_region(key, || {
                        generated.fetch_add(1, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(10));
                        empty_region()
                    })
                })
            })
            .collect();
        let regions: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();

        assert_eq!(generated.load(Ordering::Relaxed), 1);
        assert!(regions.iter().all(|r| Arc::ptr_eq(r, &regions[0])));
    }
}
fn climate_value(v: [f32; 4], dx: f64, dz: f64) -> f64 {
    let lerp = |d: f64, a: f64, b: f64| a + (b - a) * d;
    let a = lerp(dz, v[0] as f64, v[1] as f64);
    let b = lerp(dz, v[2] as f64, v[3] as f64);
    lerp(dx, a, b) as f32 as f64
}
fn fast_round(value: f64) -> i32 {
    (value + 0.5).floor() as i32
}
fn rock_offsets() -> [i32; 16] {
    static OFF: std::sync::OnceLock<[i32; 16]> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| {
        let mut rng = Xoroshiro::new(1_923_874_192_341);
        let mut a = [0; 16];
        for x in &mut a {
            *x = rng.next_int_bound(100_000);
        }
        a
    })
}
