//! Seeded TFC cave/canyon trajectories, projected to each target chunk.
use super::aquifer::positional;
use crate::{
    planetary::noise::root_positional,
    random::{JavaRandom, Xoroshiro},
    world::World,
};
use lru::LruCache;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};

#[derive(Deserialize)]
struct Profiles {
    carvers: HashMap<String, Value>,
    biomes: HashMap<String, Vec<String>>,
}
fn profiles() -> &'static Profiles {
    static P: OnceLock<Profiles> = OnceLock::new();
    P.get_or_init(|| serde_json::from_str(include_str!("assets/carvers.json")).unwrap())
}
#[derive(Clone)]
pub(super) struct Shape {
    x: f64,
    y: f64,
    z: f64,
    h: f64,
    v: f64,
    floor: f64,
    widths: Option<Arc<Vec<f32>>>,
    pub karst: bool,
    pub debug: bool,
}
impl Shape {
    fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        if !(-62..=312).contains(&y) {
            return false;
        }
        let lower = (self.y - self.v).floor() as i32 - 1;
        let upper = (self.y + self.v).floor() as i32 + 1;
        if y <= lower || y > upper {
            return false;
        }
        let (dx, dy, dz) = (
            (x as f64 + 0.5 - self.x) / self.h,
            (y as f64 - 0.5 - self.y) / self.v,
            (z as f64 + 0.5 - self.z) / self.h,
        );
        let d = dx * dx + dz * dz;
        if d >= 1. {
            return false;
        }
        match &self.widths {
            Some(w) => d * f64::from(w[(y + 64 - 1).clamp(0, 383) as usize]) + dy * dy / 6. < 1.,
            None => dy > self.floor && d + dy * dy < 1.,
        }
    }
    fn intersects(&self, cx: i32, cz: i32) -> bool {
        (self.x - (cx * 16 + 8) as f64).abs() <= 16. + self.h * 2.
            && (self.z - (cz * 16 + 8) as f64).abs() <= 16. + self.h * 2.
    }
}
pub(super) struct Carvers {
    lo: u64,
    hi: u64,
    sources: LruCache<(i32, i32), Arc<Vec<Shape>>>,
    targets: LruCache<(i32, i32), Arc<Vec<Shape>>>,
}
impl Carvers {
    pub fn new(seed: i64) -> Self {
        let (lo, hi) = root_positional(seed);
        Self {
            lo,
            hi,
            sources: LruCache::new(NonZeroUsize::new(2048).unwrap()),
            targets: LruCache::new(NonZeroUsize::new(128).unwrap()),
        }
    }
    pub fn shapes(&mut self, world: &World, cx: i32, cz: i32) -> Arc<Vec<Shape>> {
        if let Some(s) = self.targets.get(&(cx, cz)) {
            return s.clone();
        }
        let mut target = Vec::new();
        for sx in cx - 8..=cx + 8 {
            for sz in cz - 8..=cz + 8 {
                let shapes = if let Some(s) = self.sources.get(&(sx, sz)) {
                    s.clone()
                } else {
                    let name =
                        crate::biome::biome_name(world.region.biome_at_quart(sx * 4, sz * 4))
                            .rsplit('/')
                            .next()
                            .unwrap_or("");
                    let mut shapes = Vec::new();
                    for (i, id) in profiles()
                        .biomes
                        .get(name)
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        let Some(p) = profiles().carvers.get(id.rsplit(':').next().unwrap()) else {
                            continue;
                        };
                        let mut r = positional(self.lo, self.hi, sx, i as i32 + 1, sz);
                        let c = &p["config"];
                        if r.next_float() > c["probability"].as_f64().unwrap() as f32 {
                            continue;
                        }
                        let karst = id.ends_with("karst_caves");
                        let debug = c["debug_settings"]["debug_mode"].as_bool().unwrap_or(false);
                        if p["type"].as_str() == Some("tfc:canyon") {
                            canyon(&mut shapes, &mut r, sx, sz, c)
                        } else {
                            cave(&mut shapes, &mut r, sx, sz, c, karst, debug)
                        }
                    }
                    let shapes = Arc::new(shapes);
                    self.sources.put((sx, sz), shapes.clone());
                    shapes
                };
                target.extend(shapes.iter().filter(|s| s.intersects(cx, cz)).cloned());
            }
        }
        let target = Arc::new(target);
        self.targets.put((cx, cz), target.clone());
        target
    }
}
pub(super) fn at(shapes: &[Shape], world: &World, x: i32, y: i32, z: i32) -> Option<bool> {
    shapes
        .iter()
        .find(|s| {
            s.contains(x, y, z)
                && (!s.karst
                    || world.rock_at(x, y, z).is_some_and(|r| {
                        matches!(
                            r.as_str(),
                            "limestone" | "chalk" | "dolomite" | "marble" | "basalt"
                        )
                    }))
        })
        .map(|s| s.debug)
}
fn uniform(r: &mut Xoroshiro, v: &Value) -> f32 {
    if let Some(n) = v.as_f64() {
        return n as f32;
    }
    let v = &v["value"];
    let a = v["min_inclusive"].as_f64().unwrap() as f32;
    let b = v["max_exclusive"].as_f64().unwrap() as f32;
    a + r.next_float() * (b - a)
}
fn height(r: &mut Xoroshiro, c: &Value) -> i32 {
    let a = c["y"]["min_inclusive"]["absolute"].as_i64().unwrap() as i32;
    let b = c["y"]["max_inclusive"]["absolute"].as_i64().unwrap() as i32;
    a + r.next_int_bound(b - a + 1)
}
fn sin(x: f32) -> f32 {
    static LUT: OnceLock<Vec<f32>> = OnceLock::new();
    let t = LUT.get_or_init(|| {
        (0..65536)
            .map(|i| ((i as f64) * std::f64::consts::TAU / 65536.).sin() as f32)
            .collect()
    });
    t[((x * 10430.378_f32) as i32 & 65535) as usize]
}
fn cos(x: f32) -> f32 {
    static LUT: OnceLock<Vec<f32>> = OnceLock::new();
    let t = LUT.get_or_init(|| {
        (0..65536)
            .map(|i| ((i as f64) * std::f64::consts::TAU / 65536.).sin() as f32)
            .collect()
    });
    t[(((x * 10430.378_f32 + 16384.) as i32) & 65535) as usize]
}
fn cave(
    out: &mut Vec<Shape>,
    r: &mut Xoroshiro,
    cx: i32,
    cz: i32,
    c: &Value,
    karst: bool,
    debug: bool,
) {
    let b0 = r.next_int_bound(15) + 1;
    let b1 = r.next_int_bound(b0) + 1;
    let count = r.next_int_bound(b1);
    for _ in 0..count {
        let x = (cx * 16 + r.next_int_bound(16)) as f64;
        let y = height(r, c) as f64;
        let z = (cz * 16 + r.next_int_bound(16)) as f64;
        let hm = uniform(r, &c["horizontal_radius_multiplier"]) as f64;
        let vm = uniform(r, &c["vertical_radius_multiplier"]) as f64;
        let floor = uniform(r, &c["floor_level"]) as f64;
        let mut tunnels = 1;
        if r.next_int_bound(4) == 0 {
            let ratio = uniform(r, &c["yScale"]) as f64;
            let radius = 1. + r.next_float() * 6.;
            let h = 1.5 + f64::from(sin(std::f32::consts::FRAC_PI_2) * radius);
            out.push(Shape {
                x: x + 1.,
                y,
                z,
                h,
                v: h * ratio,
                floor,
                widths: None,
                karst,
                debug,
            });
            tunnels += r.next_int_bound(4);
        }
        for _ in 0..tunnels {
            let yaw = r.next_float() * std::f32::consts::TAU;
            let pitch = (r.next_float() - 0.5) / 4.;
            let mut thick = r.next_float() * 2. + r.next_float();
            if r.next_int_bound(10) == 0 {
                thick *= r.next_float() * r.next_float() * 3. + 1.;
            }
            let count = 112 - r.next_int_bound(28);
            let seed = r.next_long();
            tunnel(
                out, seed, x, y, z, hm, vm, thick, yaw, pitch, 0, count, floor, karst, debug,
            );
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn tunnel(
    out: &mut Vec<Shape>,
    seed: i64,
    mut x: f64,
    mut y: f64,
    mut z: f64,
    hm: f64,
    vm: f64,
    thick: f32,
    mut yaw: f32,
    mut pitch: f32,
    start: i32,
    count: i32,
    floor: f64,
    karst: bool,
    debug: bool,
) {
    let mut r = JavaRandom::new(seed);
    let split = r.next_int_bound(count / 2) + count / 4;
    let steep = r.next_int_bound(6) == 0;
    let (mut a, mut b) = (0_f32, 0_f32);
    for j in start..count {
        let h = 1.5 + f64::from(sin(std::f32::consts::PI * j as f32 / count as f32) * thick);
        let cp = cos(pitch);
        x += f64::from(cos(yaw) * cp);
        y += f64::from(sin(pitch));
        z += f64::from(sin(yaw) * cp);
        pitch *= if steep { 0.92 } else { 0.7 };
        pitch += b * 0.1;
        yaw += a * 0.1;
        b *= 0.9;
        a *= 0.75;
        b += (r.next_float() - r.next_float()) * r.next_float() * 2.;
        a += (r.next_float() - r.next_float()) * r.next_float() * 4.;
        if j == split && thick > 1. {
            let seed = r.next_long();
            let t = r.next_float() * 0.5 + 0.5;
            tunnel(
                out,
                seed,
                x,
                y,
                z,
                hm,
                vm,
                t,
                yaw - std::f32::consts::FRAC_PI_2,
                pitch / 3.,
                j,
                count,
                floor,
                karst,
                debug,
            );
            let seed = r.next_long();
            let t = r.next_float() * 0.5 + 0.5;
            tunnel(
                out,
                seed,
                x,
                y,
                z,
                hm,
                vm,
                t,
                yaw + std::f32::consts::FRAC_PI_2,
                pitch / 3.,
                j,
                count,
                floor,
                karst,
                debug,
            );
            return;
        }
        if r.next_int_bound(4) != 0 {
            out.push(Shape {
                x,
                y,
                z,
                h: h * hm,
                v: h * vm,
                floor,
                widths: None,
                karst,
                debug,
            });
        }
    }
}
fn canyon(out: &mut Vec<Shape>, r: &mut Xoroshiro, cx: i32, cz: i32, c: &Value) {
    let mut x = (cx * 16 + r.next_int_bound(16)) as f64;
    let mut y = height(r, c) as f64;
    let mut z = (cz * 16 + r.next_int_bound(16)) as f64;
    let mut yaw = r.next_float() * std::f32::consts::TAU;
    let mut pitch = uniform(r, &c["vertical_rotation"]);
    // TrapezoidFloat [0,6], plateau2: sum of uniform ranges4 and2.
    let thick = r.next_float() * 4. + r.next_float() * 2.;
    let count = (112_f32 * uniform(r, &c["shape"]["distance_factor"])) as i32;
    let mut r = JavaRandom::new(r.next_long());
    let mut w = vec![0.; 384];
    let mut f = 1_f32;
    for (j, v) in w.iter_mut().enumerate() {
        if j == 0 || r.next_int_bound(3) == 0 {
            f = 1. + r.next_float() * r.next_float()
        }
        *v = f * f;
    }
    let widths = Arc::new(w);
    let (mut a, mut b) = (0_f32, 0_f32);
    for j in 0..count {
        let h = 1.5 + f64::from(sin(j as f32 * std::f32::consts::PI / count as f32) * thick);
        let hfactor = f64::from(0.75 + r.next_float() * 0.25);
        let v = h * 3. * f64::from(0.75 + r.next_float() * 0.25);
        let h = h * hfactor;
        let cp = cos(pitch);
        x += f64::from(cos(yaw) * cp);
        y += f64::from(sin(pitch));
        z += f64::from(sin(yaw) * cp);
        pitch *= 0.7;
        pitch += b * 0.05;
        yaw += a * 0.05;
        b *= 0.8;
        a *= 0.5;
        b += (r.next_float() - r.next_float()) * r.next_float() * 2.;
        a += (r.next_float() - r.next_float()) * r.next_float() * 4.;
        if r.next_int_bound(4) != 0 {
            out.push(Shape {
                x,
                y,
                z,
                h,
                v,
                floor: -1.,
                widths: Some(widths.clone()),
                karst: false,
                debug: false,
            });
        }
    }
}
