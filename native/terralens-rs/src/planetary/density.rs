//! Small compiled subset of Minecraft density functions used by planetary climate routers.
use super::noise::NormalNoise;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone)]
pub(super) enum Density {
    Constant(f64),
    YGradient(f64, f64, f64, f64),
    Pass(Box<Density>),
    Shifted {
        noise: String,
        xz: f64,
        ys: f64,
        sx: Box<Density>,
        sy: Box<Density>,
        sz: Box<Density>,
    },
    Shift(String),
    ShiftA(String),
    ShiftB(String),
    Add(Box<Density>, Box<Density>),
    Mul(Box<Density>, Box<Density>),
    Spline(Box<Density>, Vec<Point>),
}
#[derive(Clone)]
pub(super) struct Point {
    location: f32,
    derivative: f32,
    value: Box<Density>,
}

impl Density {
    pub fn compile(value: &Value) -> Self {
        if let Some(n) = value.as_f64() {
            return Self::Constant(n);
        }
        let kind = value["type"].as_str().unwrap_or("");
        let child = |key: &str| Box::new(Self::compile(&value[key]));
        match kind {
            "minecraft:constant" => Self::Constant(value["argument"].as_f64().unwrap_or(0.)),
            "minecraft:y_clamped_gradient" => Self::YGradient(
                value["from_y"].as_f64().unwrap(),
                value["to_y"].as_f64().unwrap(),
                value["from_value"].as_f64().unwrap(),
                value["to_value"].as_f64().unwrap(),
            ),
            "minecraft:flat_cache"
            | "minecraft:cache_2d"
            | "minecraft:cache_once"
            | "minecraft:interpolated"
            | "minecraft:blend_density" => Self::Pass(child("argument")),
            "minecraft:shifted_noise" => Self::Shifted {
                noise: value["noise"].as_str().unwrap().to_owned(),
                xz: value["xz_scale"].as_f64().unwrap_or(1.),
                ys: value["y_scale"].as_f64().unwrap_or(1.),
                sx: child("shift_x"),
                sy: child("shift_y"),
                sz: child("shift_z"),
            },
            "minecraft:shift" => Self::Shift(value["argument"].as_str().unwrap().to_owned()),
            "minecraft:shift_a" => Self::ShiftA(value["argument"].as_str().unwrap().to_owned()),
            "minecraft:shift_b" => Self::ShiftB(value["argument"].as_str().unwrap().to_owned()),
            "minecraft:add" => Self::Add(child("argument1"), child("argument2")),
            "minecraft:mul" => Self::Mul(child("argument1"), child("argument2")),
            "minecraft:spline" => {
                let spline = &value["spline"];
                let points = spline["points"]
                    .as_array()
                    .expect("spline points")
                    .iter()
                    .map(|p| Point {
                        location: p["location"].as_f64().unwrap() as f32,
                        derivative: p["derivative"].as_f64().unwrap() as f32,
                        value: Box::new(Self::compile(&p["value"])),
                    })
                    .collect();
                Self::Spline(Box::new(Self::compile(&spline["coordinate"])), points)
            }
            _ => panic!("unported planetary climate density node `{kind}`"),
        }
    }
    pub fn sample(&self, noises: &HashMap<String, NormalNoise>, x: f64, y: f64, z: f64) -> f64 {
        let noise = |id: &str| {
            noises
                .get(id)
                .unwrap_or_else(|| panic!("missing planetary noise {id}"))
        };
        match self {
            Self::Constant(v) => *v,
            Self::YGradient(from, to, a, b) => clamped_map(y, *from, *to, *a, *b),
            Self::Pass(child) => child.sample(noises, x, y, z),
            Self::Shifted {
                noise: id,
                xz,
                ys,
                sx,
                sy,
                sz,
            } => noise(id).sample(
                x * xz + sx.sample(noises, x, y, z),
                y * ys + sy.sample(noises, x, y, z),
                z * xz + sz.sample(noises, x, y, z),
            ),
            Self::Shift(id) => noise(id).sample(x * 0.25, y * 0.25, z * 0.25) * 4.,
            Self::ShiftA(id) => noise(id).sample(x * 0.25, 0., z * 0.25) * 4.,
            Self::ShiftB(id) => noise(id).sample(z * 0.25, x * 0.25, 0.) * 4.,
            Self::Add(a, b) => a.sample(noises, x, y, z) + b.sample(noises, x, y, z),
            Self::Mul(a, b) => {
                let v = a.sample(noises, x, y, z);
                if v == 0. {
                    0.
                } else {
                    v * b.sample(noises, x, y, z)
                }
            }
            Self::Spline(coord, points) => sample_spline(coord, points, noises, x, y, z),
        }
    }
}
fn clamped_map(v: f64, a: f64, b: f64, x: f64, y: f64) -> f64 {
    if v <= a {
        x
    } else if v >= b {
        y
    } else {
        x + (v - a) / (b - a) * (y - x)
    }
}
fn sample_spline(
    coord: &Density,
    points: &[Point],
    noises: &HashMap<String, NormalNoise>,
    x: f64,
    y: f64,
    z: f64,
) -> f64 {
    let f = coord.sample(noises, x, y, z) as f32;
    let mut i = None;
    for (n, p) in points.iter().enumerate() {
        if p.location <= f {
            i = Some(n)
        } else {
            break;
        }
    }
    match i {
        None => {
            let p = &points[0];
            (p.value.sample(noises, x, y, z) as f32 + p.derivative * (f - p.location)) as f64
        }
        Some(k) if k + 1 == points.len() => {
            let p = &points[k];
            (p.value.sample(noises, x, y, z) as f32 + p.derivative * (f - p.location)) as f64
        }
        Some(k) => {
            let (a, b) = (&points[k], &points[k + 1]);
            let span = b.location - a.location;
            let t = (f - a.location) / span;
            let v0 = a.value.sample(noises, x, y, z) as f32;
            let v1 = b.value.sample(noises, x, y, z) as f32;
            let d0 = a.derivative * span - (v1 - v0);
            let d1 = -b.derivative * span + (v1 - v0);
            let linear = v0 + t * (v1 - v0);
            let bend = d0 + t * (d1 - d0);
            (linear + t * (1. - t) * bend) as f64
        }
    }
}
