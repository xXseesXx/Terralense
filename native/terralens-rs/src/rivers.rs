//! TFC 1.20 region rivers and lakes (`AddRiversAndLakes`, `River`, `MidpointFractal`).
//!
//! River builders grow inland from shore drains. Their build order, float boundaries, rejection
//! rules and random draws match the upstream source because each affects subsequent region RNG.
use crate::biome::ids::{self, id};
use crate::random::Xoroshiro;
use crate::region::{Region, RiverEdge};
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

const MIN_WIDTH: u8 = 8;
const MAX_WIDTH: u8 = 24;
const RIVER_LENGTH: f32 = 2.7;
const RIVER_DEPTH: i32 = 17;
const BRANCH_JITTER: i32 = (RIVER_DEPTH as f32 * 0.3f32) as i32;
const RIVER_FEATHER: f64 = 0.8f32 as f64;
const MIN_BRANCH_ANGLE: f64 = 0.4f32 as f64;
const MIN_BRANCH_DISTANCE: i32 = 2;
const MIN_RIVER_EDGE_COUNT: usize = 6;
// 1 + ceil(1.5f32 * RIVER_LENGTH), evaluated for the upstream length 2.7.
// Float ceil is not const-stable on the supported Rust 1.88 compiler.
const MAX_AFFECTING_GRID_DISTANCE: i32 = 6;
const EDGE_BISECTIONS: usize = 4;
const JITTER_MAX: f64 = 0.2f32 as f64;
const JITTER_MIN: f64 = 0.05f32 as f64;

#[derive(Clone, Copy, Debug)]
struct Vertex {
    x: f64,
    z: f64,
    angle: f64,
    length: f64,
    distance: i32,
    id: u64,
}
#[derive(Clone, Copy, Debug)]
struct Edge {
    source: Vertex,
    drain: Vertex,
}

#[derive(Clone, Debug)]
pub struct RiverInfo {
    pub edge_index: usize,
    /// Squared distance to the center line, in block².
    pub dist_sq_blocks: f64,
    /// Squared interpolated river width, in TFC grid².
    pub width_sq: f64,
}

/// Grows river edges and applies river mouth/lake region mutations using the continuing region RNG.
pub fn add_rivers_and_lakes(region: &mut Region, random: &mut Xoroshiro) {
    let mut builders = Vec::new();
    let mut next_builder_id = 1u64;

    // Upstream iterates `region.data` in its flat row-major order (x changes fastest).
    // Draw ordering matters here because each mouth consumes the shared region RNG.
    for dz in 0..region.size_z {
        for dx in 0..region.size_x {
            let index = dx + region.size_x * dz;
            let Some(point) = region.data[index] else {
                continue;
            };
            if !point.shore() {
                continue;
            }
            let angle = find_best_starting_angle(region, random, index);
            let Some(angle) = angle else { continue };
            let mut river_rng = Xoroshiro::new(random.next_long());
            // `min + dx + .5f` is evaluated as float in Java.
            let x = (region.min_x + dx as i32) as f32 + 0.5f32;
            let z = (region.min_z + dz as i32) as f32 + 0.5f32;
            builders.push(RiverBuilder::new(
                next_builder_id,
                &mut river_rng,
                x as f64,
                z as f64,
                angle,
            ));
            next_builder_id += 1;
            if let Some(point) = region.data[index].as_mut() {
                point.river = true;
            }
        }
    }

    let legal = |prev: Vertex, vertex: Vertex, region: &Region| {
        let px = java_round(prev.x);
        let pz = java_round(prev.z);
        let vx = java_round(vertex.x);
        let vz = java_round(vertex.z);
        let (Some(p), Some(n)) = (region.point_at(px, pz), region.point_at(vx, vz)) else {
            return false;
        };
        n.land
            && n.distance_to_ocean >= p.distance_to_ocean
            && (n.distance_to_ocean as i32) >= 3.min(prev.distance / 2)
    };
    let edges = RiverContext { region }.build(&mut builders, legal);
    let mut result = Vec::with_capacity(edges.len());

    for edge in edges {
        let segments = midpoint_fractal(
            random,
            EDGE_BISECTIONS,
            edge.source.x,
            edge.source.z,
            edge.drain.x,
            edge.drain.z,
        );
        let center_x = java_round((0.5f32 * (edge.source.x as f32 + edge.drain.x as f32)) as f64);
        let center_z = java_round((0.5f32 * (edge.source.z as f32 + edge.drain.z as f32)) as f64);
        result.push(RiverEdge {
            source: (edge.source.x, edge.source.z),
            drain: (edge.drain.x, edge.drain.z),
            segments,
            width: MIN_WIDTH,
            drain_width: MIN_WIDTH,
            source_edge: false,
            drain_edge: None,
            min_part_x: grid_to_part(center_x - MAX_AFFECTING_GRID_DISTANCE),
            min_part_z: grid_to_part(center_z - MAX_AFFECTING_GRID_DISTANCE),
            max_part_x: grid_to_part(center_x + MAX_AFFECTING_GRID_DISTANCE),
            max_part_z: grid_to_part(center_z + MAX_AFFECTING_GRID_DISTANCE),
        });
    }

    annotate_rivers(region, random, &mut result);
    region.rivers = result;
}

fn find_best_starting_angle(region: &Region, random: &mut Xoroshiro, index: usize) -> Option<f64> {
    // Java Float.MIN_VALUE is the smallest positive subnormal, not negative infinity.
    let mut best_metric = f32::from_bits(1) as f64;
    let mut best_count = 0i32;
    let mut best_angle = None;
    for dir_x in -1i32..=1 {
        for dir_z in -1i32..=1 {
            if dir_x == 0 && dir_z == 0 {
                continue;
            }
            let Some(other) = region.offset(index, 4 * dir_x, 4 * dir_z) else {
                continue;
            };
            let Some(point) = region.data[other] else {
                continue;
            };
            if !point.land {
                continue;
            }
            let metric = point.distance_to_ocean as f64 - dir_x.abs() as f64 - dir_z.abs() as f64;
            // As in Java/TS, draw on every equal candidate, not just ties discovered once.
            if metric > best_metric
                || (metric == best_metric && random.next_int_bound(1 + best_count) == 0)
            {
                if metric > best_metric {
                    best_metric = metric;
                    best_count = 0;
                }
                best_count += 1;
                best_angle = Some((dir_z as f64).atan2(dir_x as f64) as f32 as f64);
            }
        }
    }
    best_angle.map(|angle| {
        let jitter = (random.next_float() * 0.2f32 - 0.1f32) as f64;
        (angle as f32 + jitter as f32) as f64
    })
}

fn annotate_rivers(region: &mut Region, random: &mut Xoroshiro, rivers: &mut [RiverEdge]) {
    let mut starts: HashMap<(u64, u64), usize> = HashMap::with_capacity(rivers.len());
    for (i, edge) in rivers.iter().enumerate() {
        starts.insert((edge.source.0.to_bits(), edge.source.1.to_bits()), i);
    }
    for i in 0..rivers.len() {
        let target = starts
            .get(&(rivers[i].drain.0.to_bits(), rivers[i].drain.1.to_bits()))
            .copied();
        rivers[i].drain_edge = target;
        if let Some(j) = target {
            rivers[j].source_edge = true;
        }
    }
    for i in 0..rivers.len() {
        if rivers[i].source_edge {
            continue;
        }
        let mut current = Some(i);
        let mut width = MIN_WIDTH;
        while let Some(j) = current {
            rivers[j].width = rivers[j].width.max(width);
            current = rivers[j].drain_edge;
            width = (width + 2).min(MAX_WIDTH);
        }
    }
    for i in 0..rivers.len() {
        rivers[i].drain_width = rivers[i]
            .drain_edge
            .map_or(rivers[i].width, |j| rivers[j].width);
    }
    for edge in rivers.iter() {
        if !edge.source_edge && random.next_int_bound(3) == 0 {
            place_lake_near(region, edge, 1, 1);
            place_lake_near(region, edge, -1, 1);
            place_lake_near(region, edge, 1, -1);
            place_lake_near(region, edge, -1, -1);
        }
    }
}

fn place_lake_near(region: &mut Region, edge: &RiverEdge, ox: i32, oz: i32) {
    // Java `(int)` truncates toward zero, including at negative grid coordinates.
    let x = (edge.source.0 + 0.3f32 as f64 * ox as f64) as i32;
    let z = (edge.source.1 + 0.3f32 as f64 * oz as f64) as i32;
    let Some(point) = region.point_at_mut(x, z) else {
        return;
    };
    if point.land
        && point.distance_to_ocean >= 2
        && point.distance_to_edge >= 2
        && ids::has_lake(point.biome)
    {
        point.biome = lake_for(point.biome);
        point.lake = true;
        point.rainfall += 0.09f32 * (500.0f32 - point.rainfall);
    }
}
fn lake_for(biome: u16) -> u16 {
    match biome {
        id::MOUNTAINS => id::MOUNTAIN_LAKE,
        id::VOLCANIC_MOUNTAINS => id::VOLCANIC_MOUNTAIN_LAKE,
        id::OLD_MOUNTAINS => id::OLD_MOUNTAIN_LAKE,
        id::OCEANIC_MOUNTAINS => id::OCEANIC_MOUNTAIN_LAKE,
        id::VOLCANIC_OCEANIC_MOUNTAINS => id::VOLCANIC_OCEANIC_MOUNTAIN_LAKE,
        id::PLATEAU => id::PLATEAU_LAKE,
        _ => id::LAKE,
    }
}

struct RiverBuilder {
    id: u64,
    next_vertex: u64,
    rng: Xoroshiro,
    root: Vertex,
    edges: Vec<Edge>,
    branch_queue: VecDeque<Edge>,
    branch: Vec<Edge>,
    feather_sq: f64,
}
impl RiverBuilder {
    fn new(id: u64, rng: &mut Xoroshiro, x: f64, z: f64, angle: f64) -> Self {
        Self {
            id,
            next_vertex: 1,
            rng: rng.clone(),
            root: Vertex {
                x,
                z,
                angle,
                length: RIVER_LENGTH as f64,
                distance: 0,
                id: id << 32,
            },
            edges: vec![],
            branch_queue: VecDeque::new(),
            branch: vec![],
            feather_sq: RIVER_FEATHER * RIVER_FEATHER,
        }
    }
    fn empty(id: u64) -> Self {
        Self {
            id,
            next_vertex: 1,
            rng: Xoroshiro::new(0),
            root: Vertex {
                x: 0.,
                z: 0.,
                angle: 0.,
                length: 0.,
                distance: 0,
                id: id << 32,
            },
            edges: vec![],
            branch_queue: VecDeque::new(),
            branch: vec![],
            feather_sq: RIVER_FEATHER * RIVER_FEATHER,
        }
    }
    fn compute_next(&mut self, prev: Vertex, length: f64, distance: i32) -> Vertex {
        let next_angle = if distance == 0 {
            prev.angle
        } else {
            prev.angle
                + (self.rng.next_double() * 0.5f32 as f64 + 0.2f32 as f64)
                    * if self.rng.next_boolean() { 1.0 } else { -1.0 }
        };
        let next_length = length * (self.rng.next_double() * 0.08f32 as f64 + 0.92f32 as f64);
        let dx = mth_cos(next_angle as f32) as f64 * next_length;
        let dz = mth_sin(next_angle as f32) as f64 * next_length;
        let id = (self.id << 32) | self.next_vertex;
        self.next_vertex += 1;
        Vertex {
            x: prev.x + dx,
            z: prev.z + dz,
            angle: next_angle,
            length: next_length,
            distance: distance + 1,
            id,
        }
    }
    fn prune_if_short(&mut self) {
        if self.edges.len() < MIN_RIVER_EDGE_COUNT {
            self.edges.clear();
            self.branch_queue.clear();
        }
    }
    fn build_initial(
        &mut self,
        ctx: &RiverContext<'_>,
        others: &[RiverBuilder],
        legal: &impl Fn(Vertex, Vertex, &Region) -> bool,
    ) -> bool {
        let mut prev = self.root;
        let length = RIVER_DEPTH + self.rng.next_int_bound(1 + BRANCH_JITTER);
        for _ in 0..length {
            let next = self.compute_next(prev, prev.length, prev.distance);
            let edge = Edge {
                source: next,
                drain: prev,
            };
            if ctx.intersect_any(edge, self, others, legal) {
                self.prune_if_short();
                return true;
            }
            self.edges.push(edge);
            prev = next;
            if prev.distance < RIVER_DEPTH && prev.distance >= MIN_BRANCH_DISTANCE {
                self.branch_queue.push_back(edge);
            }
        }
        false
    }
    fn build_branch(
        &mut self,
        ctx: &RiverContext<'_>,
        others: &[RiverBuilder],
        legal: &impl Fn(Vertex, Vertex, &Region) -> bool,
    ) -> bool {
        let Some(prev_edge) = self.branch_queue.pop_front() else {
            self.prune_if_short();
            return true;
        };
        let mut prev = prev_edge.drain;
        let mut prev_dist = prev.distance + self.rng.next_int_bound(3);
        let branch_next = self.compute_next(prev, prev.length, prev_dist);
        let delta = (prev_edge.source.angle - branch_next.angle).abs();
        if delta < MIN_BRANCH_ANGLE || 2.0 * std::f64::consts::PI - delta < MIN_BRANCH_ANGLE {
            return false;
        }
        self.branch.clear();
        let first = Edge {
            source: branch_next,
            drain: prev,
        };
        if ctx.intersect_any(first, self, others, legal) {
            return false;
        }
        self.branch.push(first);
        prev = branch_next;
        let mut i = 0;
        loop {
            // The Java for-loop evaluates this random bound every iteration, including the one that ends it.
            let limit = RIVER_DEPTH - prev.distance + self.rng.next_int_bound(1 + BRANCH_JITTER);
            if i >= limit {
                break;
            }
            let next = self.compute_next(prev, prev.length, prev_dist);
            let edge = Edge {
                source: next,
                drain: prev,
            };
            if ctx.intersect_any(edge, self, others, legal) {
                break;
            }
            self.branch.push(edge);
            prev = next;
            prev_dist = next.distance;
            if (MIN_BRANCH_DISTANCE..RIVER_DEPTH).contains(&prev_dist) {
                self.branch_queue.push_back(edge);
            }
            i += 1;
        }
        self.edges.extend_from_slice(&self.branch);
        false
    }
    fn intersects_any(&self, edge: Edge) -> bool {
        self.edges.iter().any(|e| {
            e.source.id != edge.drain.id
                && e.drain.id != edge.drain.id
                && (distance_edge_to_vertex(*e, edge.source) < self.feather_sq
                    || segments_intersect(e.source, e.drain, edge.source, edge.drain))
        })
    }
}

struct RiverContext<'a> {
    region: &'a Region,
}
impl RiverContext<'_> {
    fn intersect_any(
        &self,
        edge: Edge,
        current: &RiverBuilder,
        others: &[RiverBuilder],
        legal: &impl Fn(Vertex, Vertex, &Region) -> bool,
    ) -> bool {
        if !legal(edge.drain, edge.source, self.region) {
            return true;
        }
        current.intersects_any(edge) || others.iter().any(|b| b.intersects_any(edge))
    }
    fn build(
        &self,
        builders: &mut [RiverBuilder],
        legal: impl Fn(Vertex, Vertex, &Region) -> bool,
    ) -> Vec<Edge> {
        let mut heap = BuilderHeap::default();
        for i in 0..builders.len() {
            let mut current =
                std::mem::replace(&mut builders[i], RiverBuilder::empty(i as u64 + 1));
            let cut = current.build_initial(self, builders, &legal);
            builders[i] = current;
            if cut {
                heap.push(i, builders);
            }
        }
        loop {
            let Some(i) = heap.pop(builders) else { break };
            let mut current =
                std::mem::replace(&mut builders[i], RiverBuilder::empty(i as u64 + 1));
            let done = current.build_branch(self, builders, &legal);
            builders[i] = current;
            if !done {
                heap.push(i, builders);
            }
        }
        builders
            .iter()
            .flat_map(|b| b.edges.iter().copied())
            .collect()
    }
}

#[derive(Default)]
struct BuilderHeap {
    values: Vec<usize>,
}
impl BuilderHeap {
    fn score(i: usize, b: &[RiverBuilder]) -> i32 {
        -(b[i].edges.len() as i32) - 10 * b[i].branch_queue.len() as i32
    }
    fn push(&mut self, value: usize, b: &[RiverBuilder]) {
        let mut k = self.values.len();
        self.values.push(value);
        while k > 0 {
            let p = (k - 1) >> 1;
            let parent = self.values[p];
            if Self::score(value, b) >= Self::score(parent, b) {
                break;
            }
            self.values[k] = parent;
            k = p;
        }
        self.values[k] = value;
    }
    fn pop(&mut self, b: &[RiverBuilder]) -> Option<usize> {
        let result = *self.values.first()?;
        let last = self.values.pop()?;
        if self.values.is_empty() {
            return Some(result);
        }
        let mut k = 0;
        let half = self.values.len() >> 1;
        while k < half {
            let mut child = 2 * k + 1;
            let right = child + 1;
            if right < self.values.len()
                && Self::score(self.values[child], b) > Self::score(self.values[right], b)
            {
                child = right
            }
            if Self::score(last, b) <= Self::score(self.values[child], b) {
                break;
            }
            self.values[k] = self.values[child];
            k = child;
        }
        self.values[k] = last;
        Some(result)
    }
}

fn midpoint_fractal(
    random: &mut Xoroshiro,
    bisections: usize,
    sx: f64,
    sz: f64,
    dx: f64,
    dz: f64,
) -> Vec<(f64, f64)> {
    let mut points = vec![(sx, sz), (dx, dz)];
    for _ in 0..bisections {
        let mut next = Vec::with_capacity(points.len() * 2 - 1);
        next.push(points[0]);
        for w in points.windows(2) {
            let (x0, z0) = w[0];
            let (x1, z1) = w[1];
            let norm = (x0 - x1).abs().max((z0 - z1).abs());
            let jitter = |r: &mut Xoroshiro| {
                let v = 2.0 * r.next_double() - 1.0;
                (JITTER_MAX - JITTER_MIN) * v + if v < 0.0 { -JITTER_MIN } else { JITTER_MIN }
            };
            let mx = jitter(random) * norm + (x0 + x1) * 0.5f32 as f64;
            let mz = jitter(random) * norm + (z0 + z1) * 0.5f32 as f64;
            next.push((mx, mz));
            next.push((x1, z1));
        }
        points = next;
    }
    points
}

fn norm2(x: f64, z: f64) -> f64 {
    x * x + z * z
}
fn norm_inf(x: f64, z: f64) -> f64 {
    x.abs().max(z.abs())
}
fn distance_point_line_sq(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    let l2 = norm2(a.0 - b.0, a.1 - b.1);
    if l2 == 0.0 {
        return 0.0;
    }
    let t = (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / l2).clamp(0.0, 1.0);
    let x = a.0 + t * (b.0 - a.0);
    let z = a.1 + t * (b.1 - a.1);
    norm2(x - p.0, z - p.1)
}
fn distance_edge_to_vertex(edge: Edge, point: Vertex) -> f64 {
    distance_point_line_sq(
        (edge.source.x, edge.source.z),
        (edge.drain.x, edge.drain.z),
        (point.x, point.z),
    )
}
fn orientation(p: Vertex, q: Vertex, r: Vertex) -> i32 {
    let v = (q.z - p.z) * (r.x - q.x) - (q.x - p.x) * (r.z - q.z);
    if v == 0.0 {
        0
    } else if v > 0.0 {
        1
    } else {
        2
    }
}
fn on_segment(p: Vertex, q: Vertex, r: Vertex) -> bool {
    q.x <= p.x.max(r.x) && q.x >= p.x.min(r.x) && q.z <= p.z.max(r.z) && q.z >= p.z.min(r.z)
}
fn segments_intersect(p1: Vertex, q1: Vertex, p2: Vertex, q2: Vertex) -> bool {
    let (o1, o2, o3, o4) = (
        orientation(p1, q1, p2),
        orientation(p1, q1, q2),
        orientation(p2, q2, p1),
        orientation(p2, q2, q1),
    );
    (o1 != o2 && o3 != o4)
        || (o1 == 0 && on_segment(p1, p2, q1))
        || (o2 == 0 && on_segment(p1, q2, q1))
        || (o3 == 0 && on_segment(p2, p1, q2))
        || (o4 == 0 && on_segment(p2, q1, q2))
}
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}
fn grid_to_part(v: i32) -> i32 {
    v.div_euclid(3)
}
fn mth_sin_table() -> &'static [f32] {
    static SIN: OnceLock<Vec<f32>> = OnceLock::new();
    SIN.get_or_init(|| {
        (0..65536)
            .map(|i| (i as f64 * std::f64::consts::TAU / 65536.0).sin() as f32)
            .collect()
    })
}
fn mth_index(angle: f32, cos: bool) -> usize {
    // Mth adds the cosine quarter-turn as a float, then truncates. Truncating first shifts
    // negative non-integral angles by a table entry and moves entire river valleys.
    let phase = angle * 10430.378f32;
    let shifted = if cos { phase + 16384.0f32 } else { phase };
    ((shifted as i32) as u32 as usize) & 65535
}

fn mth_sin(angle: f32) -> f32 {
    mth_sin_table()[mth_index(angle, false)]
}
fn mth_cos(angle: f32) -> f32 {
    mth_sin_table()[mth_index(angle, true)]
}

/// Quickly rejects a point outside the fractal's chord plus its maximum deviation bound.
pub fn river_maybe_intersects(edge: &RiverEdge, x: f64, z: f64, distance: f64) -> bool {
    // ENCOMPASSING_RANGES[4] is generated by the same recurrence as MidpointFractal.
    let range = encompassing_ranges()[EDGE_BISECTIONS];
    let norm = range * norm_inf(edge.source.0 - edge.drain.0, edge.source.1 - edge.drain.1);
    let d = distance_point_line_sq(edge.source, edge.drain, (x, z));
    let t = distance + norm;
    d <= t * t
}
fn encompassing_ranges() -> &'static [f64; 10] {
    static TABLE: OnceLock<[f64; 10]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.; 10];
        let mut delta = 0.0f64;
        let mut midpoint = 0.0f64;
        let mut norm = 1.0f64;
        let sqrt2 = 2f64.sqrt();
        for entry in &mut table {
            *entry = delta.max(midpoint);
            let prev = midpoint;
            midpoint = 0.5f32 as f64 * (delta + midpoint) + sqrt2 * JITTER_MAX * norm;
            delta = prev.max(delta);
            norm *= 0.5f32 as f64 + JITTER_MAX;
        }
        table
    })
}

/// Exact minimum squared distance from a block-space sample to this edge's bisected river path,
/// expressed in TFC grid².
pub fn river_distance_sq(edge: &RiverEdge, x: f64, z: f64) -> f64 {
    edge.segments
        .windows(2)
        .map(|w| distance_point_line_sq(w[0], w[1], (x, z)))
        .fold(f64::MAX, f64::min)
}
fn project_along(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    let l2 = norm2(a.0 - b.0, a.1 - b.1);
    if l2 == 0.0 {
        return 0.0;
    }
    (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / l2).clamp(0.0, 1.0)
}
fn width_sq_at(edges: &[RiverEdge], index: usize, x: f64, z: f64) -> f64 {
    let edge = &edges[index];
    let t = project_along(edge.source, (edge.drain.1, edge.drain.1), (x, z));
    // RiverEdge.widthSq takes doubles and delegates to Mth.lerp(double, double, double).
    // Keep the projection and interpolation in f64 even though the stored widths are integers.
    let width = edge.width as f64 + t * (edge.drain_width as f64 - edge.width as f64);
    width * width
}

/// Matches TFC's `ChunkHeightFiller.sampleRiverEdge`, including width-normalized selection and
/// the source game's drain-Y-for-both-coordinates width interpolation behavior.
pub fn sample_river_edge(edges: &[RiverEdge], block_x: f64, block_z: f64) -> Option<RiverInfo> {
    const LIMIT: f64 = 2500.0 / (128.0 * 128.0);
    let (x, z) = (block_x / 128.0, block_z / 128.0);
    let mut min_dist = LIMIT;
    let mut min_adjusted = f64::MAX;
    let mut best = None;
    for (i, edge) in edges.iter().enumerate() {
        if !river_maybe_intersects(edge, x, z, min_dist) {
            continue;
        }
        let d = river_distance_sq(edge, x, z);
        if d >= LIMIT {
            continue;
        }
        let adjusted = d / (edge.width as f64 * edge.width as f64);
        if adjusted < min_adjusted {
            min_dist = d;
            min_adjusted = adjusted;
            best = Some(i)
        }
    }
    best.map(|edge_index| RiverInfo {
        edge_index,
        dist_sq_blocks: min_dist * 128.0 * 128.0,
        width_sq: width_sq_at(edges, edge_index, x, z),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(
        source: (f64, f64),
        drain: (f64, f64),
        width: u8,
        drain_edge: Option<usize>,
    ) -> RiverEdge {
        RiverEdge {
            source,
            drain,
            segments: vec![source, drain],
            width,
            drain_width: width,
            source_edge: false,
            drain_edge,
            min_part_x: -1,
            min_part_z: -1,
            max_part_x: 1,
            max_part_z: 1,
        }
    }

    #[test]
    fn midpoint_fractal_keeps_source_and_drain_and_bisects_four_times() {
        let mut rng = Xoroshiro::new(7);
        let points = midpoint_fractal(&mut rng, EDGE_BISECTIONS, 0.0, 0.0, 1.0, 0.0);
        assert_eq!(points.len(), 17);
        assert_eq!(points.first(), Some(&(0.0, 0.0)));
        assert_eq!(points.last(), Some(&(1.0, 0.0)));
        assert!(points[1..points.len() - 1].iter().any(|(_, z)| *z != 0.0));
    }

    #[test]
    fn river_sampler_uses_fractal_distance_and_interpolated_width() {
        let mut edges = vec![
            edge((0.0, 0.0), (1.0, 1.0), 8, Some(1)),
            edge((2.0, 0.0), (3.0, 1.0), 10, None),
        ];
        edges[0].drain_width = 10;
        edges[0].segments = vec![(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)];
        let info = sample_river_edge(&edges, 64.0, 32.0).unwrap();
        assert_eq!(info.edge_index, 0);
        assert!(info.dist_sq_blocks < 0.001);
        // The upstream drain-y-for-both-coordinates interpolation gives t=.375, width 8.75.
        assert_eq!(info.width_sq, 8.75f32.powi(2) as f64);
    }
}
