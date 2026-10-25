//! TFC's installed noise-cave graph, sampled on its 4×8×4 interpolation lattice.
use crate::planetary::noise::NormalNoise;
use lru::LruCache;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
};

pub(super) struct Density {
    nodes: [Node; 5],
    noises: HashMap<String, NormalNoise>,
    corners: LruCache<(i32, i32, i32), [f64; 5]>,
}
impl Density {
    pub fn new(seed: i64) -> Self {
        let definitions: Value = serde_json::from_str(include_str!("assets/density.json"))
            .expect("TFC cave density profile");
        let parameters: Value = serde_json::from_str(include_str!(
            "../planetary/assets/minecraft_1_20_1_noise_parameters.json"
        ))
        .expect("Minecraft noise parameters");
        let nodes = [
            "noise_caves",
            "noodle_caves",
            "noodle_thickness",
            "noodle_ridge_a",
            "noodle_ridge_b",
        ]
        .map(|id| Node::compile(&definitions[id]));
        let mut used = HashSet::new();
        collect_noises(&definitions, &mut used);
        let noises = parameters
            .as_object()
            .unwrap()
            .iter()
            .filter(|(id, _)| used.contains(id.as_str()))
            .map(|(id, p)| {
                let amps = p["amplitudes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect::<Vec<_>>();
                (
                    id.clone(),
                    NormalNoise::new(seed, id, p["firstOctave"].as_i64().unwrap() as i32, &amps),
                )
            })
            .collect();
        Self {
            nodes,
            noises,
            corners: LruCache::new(NonZeroUsize::new(16_384).unwrap()),
        }
    }
    fn corner(&mut self, x: i32, y: i32, z: i32) -> [f64; 5] {
        let key = (x, y, z);
        if let Some(v) = self.corners.get(&key) {
            return *v;
        }
        let v = std::array::from_fn(|i| {
            self.nodes[i].sample(&self.noises, x as f64, y as f64, z as f64)
        });
        self.corners.put(key, v);
        v
    }
    pub fn sample(&mut self, x: i32, y: i32, z: i32, terrain: f64) -> f64 {
        let (x0, y0, z0) = (
            x.div_euclid(4) * 4,
            y.div_euclid(8) * 8,
            z.div_euclid(4) * 4,
        );
        let (dx, dy, dz) = (
            (x - x0) as f64 / 4.,
            (y - y0) as f64 / 8.,
            (z - z0) as f64 / 4.,
        );
        let corners: [[[f64; 5]; 2]; 4] = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                self.corner(
                    x0 + (i as i32 & 1) * 4,
                    y0 + j as i32 * 8,
                    z0 + (i as i32 >> 1) * 4,
                )
            })
        });
        let v: [f64; 5] = std::array::from_fn(|k| {
            // TrilinearInterpolator.updateForXZ runs X, then Z, then Y.
            let low = lerp(
                dz,
                lerp(dx, corners[0][0][k], corners[1][0][k]),
                lerp(dx, corners[2][0][k], corners[3][0][k]),
            );
            let high = lerp(
                dz,
                lerp(dx, corners[0][1][k], corners[1][1][k]),
                lerp(dx, corners[2][1][k], corners[3][1][k]),
            );
            lerp(dy, low, high)
        });
        let mut out = terrain.min(v[0]);
        if v[1] >= 0. {
            let thickness = 0.05 + ((v[2] + 1.) / 2.).clamp(0., 1.) * 0.05;
            out = out.min((1.5 * v[3]).abs().max((1.5 * v[4]).abs()) - thickness);
        }
        out
    }
}
fn collect_noises(value: &Value, used: &mut HashSet<String>) {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if matches!(key.as_str(), "noise" | "noise1" | "noise2") {
                if let Some(id) = value.as_str() {
                    used.insert(id.to_owned());
                }
            } else {
                collect_noises(value, used);
            }
        }
    }
}
fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}
#[derive(Clone)]
enum Node {
    Const(f64),
    Y,
    Noise(String, f64, f64),
    Add(Box<Node>, Box<Node>),
    Mul(Box<Node>, Box<Node>),
    Min(Box<Node>, Box<Node>),
    Clamp(Box<Node>, f64, f64),
    Abs(Box<Node>),
    Square(Box<Node>),
    Gradient(f64, f64, f64, f64),
    Range(Box<Node>, f64, f64, Box<Node>, Box<Node>),
    Spaghetti2(Box<Node>, Box<Node>, Box<Node>, String),
    Spaghetti3(Box<Node>, Box<Node>, String, String),
}
impl Node {
    fn compile(v: &Value) -> Self {
        if let Some(n) = v.as_f64() {
            return Self::Const(n);
        }
        if v.as_str() == Some("minecraft:y") {
            return Self::Y;
        }
        let child = |k: &str| Box::new(Self::compile(&v[k]));
        match v["type"].as_str().expect("density type") {
            "minecraft:add" => Self::Add(child("argument1"), child("argument2")),
            "minecraft:mul" => Self::Mul(child("argument1"), child("argument2")),
            "minecraft:min" => Self::Min(child("argument1"), child("argument2")),
            "minecraft:cache_once" | "minecraft:cache_2d" => *child("argument"),
            "minecraft:abs" => Self::Abs(child("argument")),
            "minecraft:square" => Self::Square(child("argument")),
            "minecraft:clamp" => Self::Clamp(
                child("input"),
                v["min"].as_f64().unwrap(),
                v["max"].as_f64().unwrap(),
            ),
            "minecraft:noise" => Self::Noise(
                v["noise"].as_str().unwrap().into(),
                v["xz_scale"].as_f64().unwrap(),
                v["y_scale"].as_f64().unwrap(),
            ),
            "minecraft:y_clamped_gradient" => Self::Gradient(
                v["from_y"].as_f64().unwrap(),
                v["to_y"].as_f64().unwrap(),
                v["from_value"].as_f64().unwrap(),
                v["to_value"].as_f64().unwrap(),
            ),
            "minecraft:range_choice" => Self::Range(
                child("input"),
                v["min_inclusive"].as_f64().unwrap(),
                v["max_exclusive"].as_f64().unwrap(),
                child("when_in_range"),
                child("when_out_of_range"),
            ),
            "tfc:spaghetti2d" => Self::Spaghetti2(
                child("rarity"),
                child("thickness"),
                child("elevation"),
                v["noise"].as_str().unwrap().into(),
            ),
            "tfc:spaghetti3d" => Self::Spaghetti3(
                child("rarity"),
                child("thickness"),
                v["noise1"].as_str().unwrap().into(),
                v["noise2"].as_str().unwrap().into(),
            ),
            other => panic!("unsupported TFC cave density node {other}"),
        }
    }
    fn sample(&self, n: &HashMap<String, NormalNoise>, x: f64, y: f64, z: f64) -> f64 {
        let s = |v: &Node| v.sample(n, x, y, z);
        match self {
            Self::Const(v) => *v,
            Self::Y => y,
            Self::Noise(id, sx, sy) => n[id].sample(x * sx, y * sy, z * sx),
            Self::Add(a, b) => s(a) + s(b),
            Self::Mul(a, b) => s(a) * s(b),
            Self::Min(a, b) => s(a).min(s(b)),
            Self::Clamp(a, lo, hi) => s(a).clamp(*lo, *hi),
            Self::Abs(a) => s(a).abs(),
            Self::Square(a) => {
                let v = s(a);
                v * v
            }
            Self::Gradient(a, b, c, d) => lerp(((y - a) / (b - a)).clamp(0., 1.), *c, *d),
            Self::Range(a, lo, hi, b, c) => {
                let v = s(a);
                if v >= *lo && v < *hi { s(b) } else { s(c) }
            }
            Self::Spaghetti2(r, t, e, id) => {
                let r = s(r);
                let r = if r < -0.75 {
                    0.5
                } else if r < -0.5 {
                    0.75
                } else if r < 0.5 {
                    1.
                } else if r < 0.75 {
                    2.
                } else {
                    3.
                };
                let t = s(t);
                let left = (s(e) - y / 8.).abs() - t;
                let right = (r * n[id].sample(x / r, y / r, z / r)).abs() - 0.083 * t;
                (left * left * left).max(right).clamp(-1., 1.)
            }
            Self::Spaghetti3(r, t, a, b) => {
                let r = s(r);
                let r = if r < -0.5 {
                    0.75
                } else if r < 0. {
                    1.
                } else if r < 0.5 {
                    1.5
                } else {
                    2.
                };
                ((r * n[a].sample(x / r, y / r, z / r))
                    .abs()
                    .max((r * n[b].sample(x / r, y / r, z / r)).abs())
                    - s(t))
                .clamp(-1., 1.)
            }
        }
    }
}
