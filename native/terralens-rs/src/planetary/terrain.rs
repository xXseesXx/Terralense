//! Density-router surface sampler for TFG's Ad Astra Moon and Mars.
//!
//! The sampler uses the installed 1.20.1 router resources. It evaluates the
//! density grid at Minecraft's configured noise-cell corners (4x8x4 on Moon,
//! 4x4x4 on Mars), then trilinearly interpolates the requested block column.
//! This is the same grid shape used
//! by NoiseChunk, and avoids a full density evaluation for every block.
use super::noise::{self, ImprovedNoise, NormalNoise};
use crate::{dimension::Dimension, random::Xoroshiro};
use lru::LruCache;
use serde_json::Value;
use std::{collections::HashMap, num::NonZeroUsize, sync::Mutex};

const RESOURCES: &str = include_str!("assets/resources.json");
const VANILLA_NOISES: &str = include_str!("assets/minecraft_1_20_1_noise_parameters.json");
const VANILLA_CAVES: &str = include_str!("assets/minecraft_1_20_1_builtin_cave_density.json");
const CELL_XZ: i32 = 4;

/// Density-derived highest solid block estimate for the active TFG planetary router.
///
/// `None` is returned for unsupported dimensions or an invalid/non-finite sample.
pub(super) struct Terrain {
    dimension: Dimension,
    min_y: i32,
    max_y: i32,
    cell_y: i32,
    root: Node,
    registry: HashMap<String, Node>,
    noises: HashMap<String, NormalNoise>,
    blended: HashMap<String, BlendedNoise>,
    craters: Option<super::craters::Craters>,
    height_cache: Mutex<LruCache<(i32, i32), f64>>,
    surface_cache: Mutex<LruCache<(i32, i32), f64>>,
    density_cache: Mutex<LruCache<(usize, i32, i32, i32), f64>>,
}

impl Terrain {
    pub(super) fn new(seed: i64, dimension: Dimension) -> Self {
        assert!(
            dimension.is_planetary(),
            "terrain sampler requires Moon or Mars"
        );
        let name = if dimension == Dimension::Moon {
            "tfg:moon_noise"
        } else {
            "tfg:mars_noise"
        };
        let resources: Value = serde_json::from_str(RESOURCES).expect("planetary terrain assets");
        let settings = &resources["noise_settings"][name];
        assert!(
            !settings.is_null(),
            "missing planetary noise setting {name}"
        );
        let min_y = settings["noise"]["min_y"].as_i64().expect("noise min_y") as i32;
        let _noise_height = settings["noise"]["height"].as_i64().expect("noise height") as i32;
        let definitions: HashMap<String, Value> = resources["density_functions"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, def)| (id.clone(), def.clone()))
            .collect();
        // Resolve the vanilla cave dependencies as well as the custom datapack functions.
        let mut definitions = definitions;
        let vanilla_caves: Value =
            serde_json::from_str(VANILLA_CAVES).expect("vanilla cave density functions");
        for (id, def) in vanilla_caves["definitions"].as_object().unwrap() {
            definitions.insert(id.clone(), def.clone());
        }
        let mut noises = HashMap::new();
        for (id, def) in resources["noises"].as_object().unwrap() {
            let first = def["firstOctave"].as_i64().unwrap() as i32;
            let amplitudes: Vec<f64> = def["amplitudes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            noises.insert(id.clone(), NormalNoise::new(seed, id, first, &amplitudes));
        }
        // Exact 1.20.1 NoiseData bootstrap parameters, extracted from the installed
        // official client jar and matched through that version's client mappings.
        let vanilla: Value =
            serde_json::from_str(VANILLA_NOISES).expect("vanilla noise parameters");
        for (id, def) in vanilla.as_object().unwrap() {
            let first = def["firstOctave"].as_i64().unwrap() as i32;
            let amplitudes = def["amplitudes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>();
            noises.insert(id.clone(), NormalNoise::new(seed, id, first, &amplitudes));
        }
        let mut blended = HashMap::new();
        build_blended(
            seed,
            &settings["noise_router"]["final_density"],
            &definitions,
            &mut blended,
        );
        let registry = definitions
            .iter()
            .map(|(id, def)| (id.clone(), Node::compile(def)))
            .collect();
        let root = Node::compile(&settings["noise_router"]["final_density"]);
        Self {
            dimension,
            min_y,
            // Dimension bounds are physical build limits. The router grid is separately
            // aligned to noise.min_y, which can differ (Mars starts at -32 in the router).
            max_y: dimension.min_y() + dimension.height() - 1,
            cell_y: settings["noise"]["size_vertical"].as_i64().unwrap() as i32 * 4,
            root,
            registry,
            noises,
            blended,
            craters: (dimension == Dimension::Moon).then(|| super::craters::Craters::new(seed)),
            height_cache: Mutex::new(LruCache::new(NonZeroUsize::new(8192).unwrap())),
            surface_cache: Mutex::new(LruCache::new(NonZeroUsize::new(8192).unwrap())),
            density_cache: Mutex::new(LruCache::new(NonZeroUsize::new(65_536).unwrap())),
        }
    }

    /// Returns the surface after density generation and the supported surface carvers.
    pub(super) fn height(
        &self,
        x: i32,
        z: i32,
        eligible: &impl Fn(i32, i32) -> bool,
    ) -> Option<f64> {
        let Some(craters) = &self.craters else {
            return self.base_height(x, z);
        };
        if let Some(height) = self.surface_cache.lock().ok()?.get(&(x, z)).copied() {
            return Some(height);
        }
        let base = self.base_height(x, z)? as i32;
        let column = craters.column(
            x,
            z,
            &|x, z| self.base_height(x, z).map(|height| height as i32),
            eligible,
        );
        let y = (self.dimension.min_y().max(self.min_y)..=base)
            .rev()
            .find(|&y| {
                !column.carves(y) && self.root.positive(self, x as f64, y as f64, z as f64, 0)
            })? as f64;
        if let Ok(mut cache) = self.surface_cache.lock() {
            cache.put((x, z), y);
        }
        Some(y)
    }

    /// Returns the highest solid density block in this column, before surface decoration.
    /// The terrain router is sampled at block x/z and the 1.20.1 noise grid.
    pub(super) fn base_height(&self, x: i32, z: i32) -> Option<f64> {
        if !self.dimension.is_planetary() {
            return None;
        }
        let key = (x, z);
        if let Some(v) = self.height_cache.lock().ok()?.get(&key).copied() {
            return Some(v);
        }
        // Explicit `interpolated` nodes sample their input at the configured cell corners;
        // operators outside that marker still evaluate at the block position.
        let bottom = self.dimension.min_y().max(self.min_y);
        let mut found = None;
        let mut cell = (self.max_y - self.min_y).div_euclid(self.cell_y);
        while self.min_y + cell * self.cell_y >= bottom {
            let low = self.min_y + cell * self.cell_y;
            let high = (low + self.cell_y - 1).min(self.max_y);
            // Interpolated density is affine in Y inside each noise cell. Reject a cell
            // only if its density envelope cannot contain a solid block. Unlike skipping
            // blocks, this preserves thin shelves and disconnected surface islands.
            if self
                .root
                .may_be_positive(self, x as f64, low as f64, high as f64, z as f64, 0)
            {
                found = (low..=high)
                    .rev()
                    .find(|&y| self.root.positive(self, x as f64, y as f64, z as f64, 0))
                    .map(|y| y as f64);
                if found.is_some() {
                    break;
                }
            }
            cell -= 1;
        }
        let value = found?;
        if let Ok(mut cache) = self.height_cache.lock() {
            cache.put(key, value);
        }
        Some(value)
    }

    fn interpolate(&self, node: &Node, x: f64, y: f64, z: f64, depth: u8) -> f64 {
        let (xi, yi, zi) = (x as i32, y as i32, z as i32);
        let x0 = xi.div_euclid(CELL_XZ) * CELL_XZ;
        let y0 = self.min_y + (yi - self.min_y).div_euclid(self.cell_y) * self.cell_y;
        let z0 = zi.div_euclid(CELL_XZ) * CELL_XZ;
        let (fx, fy, fz) = (
            (x - x0 as f64) / 4.,
            (y - y0 as f64) / self.cell_y as f64,
            (z - z0 as f64) / 4.,
        );
        let q = |ix: i32, iy: i32, iz: i32| {
            self.sample_cached(
                node,
                (x0 + ix * 4) as f64,
                (y0 + iy * self.cell_y) as f64,
                (z0 + iz * 4) as f64,
                depth + 1,
            )
        };
        lerp(
            fx,
            lerp(
                fy,
                lerp(fz, q(0, 0, 0), q(0, 0, 1)),
                lerp(fz, q(0, 1, 0), q(0, 1, 1)),
            ),
            lerp(
                fy,
                lerp(fz, q(1, 0, 0), q(1, 0, 1)),
                lerp(fz, q(1, 1, 0), q(1, 1, 1)),
            ),
        )
    }

    fn sample_cached(&self, node: &Node, x: f64, y: f64, z: f64, depth: u8) -> f64 {
        let key = (node as *const Node as usize, x as i32, y as i32, z as i32);
        if let Ok(mut cache) = self.density_cache.lock() {
            if let Some(v) = cache.get(&key) {
                return *v;
            }
        }
        let value = node.sample(self, x, y, z, depth);
        if let Ok(mut cache) = self.density_cache.lock() {
            cache.put(key, value);
        }
        value
    }
}

#[derive(Clone)]
enum Node {
    Const(f64),
    Y,
    Ref(String),
    OldBlend(String),
    Noise(String, f64, f64),
    Pass(Box<Node>),
    FlatCache(Box<Node>),
    Interpolated(Box<Node>),
    Add(Box<Node>, Box<Node>),
    Mul(Box<Node>, Box<Node>),
    Min(Box<Node>, Box<Node>),
    Max(Box<Node>, Box<Node>),
    Abs(Box<Node>),
    Square(Box<Node>),
    Cube(Box<Node>),
    HalfNegative(Box<Node>),
    QuarterNegative(Box<Node>),
    Clamp(Box<Node>, f64, f64),
    Squeeze(Box<Node>),
    Gradient(f64, f64, f64, f64),
    Shifted(String, f64, f64, Box<Node>, Box<Node>, Box<Node>),
    Spline(Box<Node>, Vec<(f32, f32, Box<Node>)>),
    WeirdScaled(Box<Node>, String, bool),
    Range(Box<Node>, f64, f64, Box<Node>, Box<Node>),
}
impl Node {
    fn positive(&self, t: &Terrain, x: f64, y: f64, z: f64, depth: u8) -> bool {
        match self {
            Self::Min(a, b) => {
                a.positive(t, x, y, z, depth + 1) && b.positive(t, x, y, z, depth + 1)
            }
            Self::Max(a, b) => {
                a.positive(t, x, y, z, depth + 1) || b.positive(t, x, y, z, depth + 1)
            }
            Self::Squeeze(a) | Self::Pass(a) => a.positive(t, x, y, z, depth + 1),
            _ => self.sample(t, x, y, z, depth) > 0.,
        }
    }

    fn may_be_positive(&self, t: &Terrain, x: f64, low: f64, high: f64, z: f64, depth: u8) -> bool {
        match self {
            Self::Min(a, b) => {
                a.may_be_positive(t, x, low, high, z, depth + 1)
                    && b.may_be_positive(t, x, low, high, z, depth + 1)
            }
            Self::Max(a, b) => {
                a.may_be_positive(t, x, low, high, z, depth + 1)
                    || b.may_be_positive(t, x, low, high, z, depth + 1)
            }
            Self::Squeeze(a) | Self::Pass(a) => a.may_be_positive(t, x, low, high, z, depth + 1),
            Self::Mul(a, b) => match (&**a, &**b) {
                (Self::Const(c), other) | (other, Self::Const(c)) if *c > 0. => {
                    other.may_be_positive(t, x, low, high, z, depth + 1)
                }
                (Self::Const(c), _) | (_, Self::Const(c)) if *c == 0. => false,
                _ => true,
            },
            Self::Const(v) => *v > 0.,
            Self::Gradient(..) | Self::Interpolated(_) => {
                self.sample(t, x, low, z, depth) > 0. || self.sample(t, x, high, z, depth) > 0.
            }
            // Unproven nodes are conservatively scanned, never treated as empty.
            _ => true,
        }
    }

    fn compile(v: &Value) -> Self {
        if let Some(n) = v.as_f64() {
            return Self::Const(n);
        }
        if let Some(id) = v.as_str() {
            return if id == "minecraft:y" {
                Self::Y
            } else {
                Self::Ref(id.into())
            };
        }
        if v.get("coordinate").is_some() && v.get("points").is_some() {
            let points = v["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    (
                        p["location"].as_f64().unwrap() as f32,
                        p["derivative"].as_f64().unwrap() as f32,
                        Box::new(Self::compile(&p["value"])),
                    )
                })
                .collect();
            return Self::Spline(Box::new(Self::compile(&v["coordinate"])), points);
        }
        let typ = v["type"].as_str().unwrap_or("");
        let child = |k: &str| Box::new(Self::compile(&v[k]));
        match typ {
            "minecraft:old_blended_noise" => Self::OldBlend(blend_key(v)),
            "minecraft:noise" => Self::Noise(
                v["noise"].as_str().unwrap().into(),
                v["xz_scale"].as_f64().unwrap_or(1.),
                v["y_scale"].as_f64().unwrap_or(1.),
            ),
            "minecraft:shifted_noise" => Self::Shifted(
                v["noise"].as_str().unwrap().into(),
                v["xz_scale"].as_f64().unwrap_or(1.),
                v["y_scale"].as_f64().unwrap_or(1.),
                child("shift_x"),
                child("shift_y"),
                child("shift_z"),
            ),
            "minecraft:constant" => Self::Const(v["argument"].as_f64().unwrap()),
            "minecraft:y_clamped_gradient" => Self::Gradient(
                v["from_y"].as_f64().unwrap(),
                v["to_y"].as_f64().unwrap(),
                v["from_value"].as_f64().unwrap(),
                v["to_value"].as_f64().unwrap(),
            ),
            "minecraft:flat_cache" => Self::FlatCache(child("argument")),
            "minecraft:cache_2d" | "minecraft:cache_once" | "minecraft:blend_density" => {
                Self::Pass(child("argument"))
            }
            "minecraft:interpolated" => Self::Interpolated(child("argument")),
            "minecraft:add" => Self::Add(child("argument1"), child("argument2")),
            "minecraft:mul" => Self::Mul(child("argument1"), child("argument2")),
            "minecraft:min" => Self::Min(child("argument1"), child("argument2")),
            "minecraft:max" => Self::Max(child("argument1"), child("argument2")),
            "minecraft:abs" => Self::Abs(child("argument")),
            "minecraft:square" => Self::Square(child("argument")),
            "minecraft:cube" => Self::Cube(child("argument")),
            "minecraft:half_negative" => Self::HalfNegative(child("argument")),
            "minecraft:quarter_negative" => Self::QuarterNegative(child("argument")),
            "minecraft:blend_alpha" => Self::Const(1.0),
            "minecraft:blend_offset" => Self::Const(0.0),
            "minecraft:spline" => {
                let spline = &v["spline"];
                if let Some(v) = spline.as_f64() {
                    Self::Const(v)
                } else {
                    Self::compile(spline)
                }
            }
            "minecraft:clamp" => Self::Clamp(
                child("input"),
                v["min"].as_f64().unwrap(),
                v["max"].as_f64().unwrap(),
            ),
            "minecraft:squeeze" => Self::Squeeze(child("argument")),
            "minecraft:range_choice" => Self::Range(
                child("input"),
                v["min_inclusive"].as_f64().unwrap(),
                v["max_exclusive"].as_f64().unwrap(),
                child("when_in_range"),
                child("when_out_of_range"),
            ),
            "minecraft:weird_scaled_sampler" => Self::WeirdScaled(
                child("input"),
                v["noise"].as_str().unwrap().into(),
                v["rarity_value_mapper"].as_str().unwrap() == "type_1",
            ),
            _ => panic!("unsupported terrain density node {typ}"),
        }
    }
    fn sample(&self, t: &Terrain, x: f64, y: f64, z: f64, depth: u8) -> f64 {
        assert!(depth < 64, "terrain density reference cycle");
        let s = |n: &Node| n.sample(t, x, y, z, depth + 1);
        match self {
            Self::Const(v) => *v,
            Self::Y => y,
            Self::Ref(id) => match id.as_str() {
                "minecraft:shift_x" => {
                    t.noises["minecraft:shift"].sample(x * 0.25, 0., z * 0.25) * 4.
                }
                "minecraft:shift_z" => {
                    t.noises["minecraft:shift"].sample(z * 0.25, x * 0.25, 0.) * 4.
                }
                "minecraft:shift" => {
                    t.noises["minecraft:shift"].sample(x * 0.25, y * 0.25, z * 0.25) * 4.
                }
                _ => t
                    .registry
                    .get(id)
                    .unwrap_or_else(|| panic!("missing terrain density {id}"))
                    .sample(t, x, y, z, depth + 1),
            },
            Self::OldBlend(id) => t
                .blended
                .get(id)
                .unwrap_or_else(|| panic!("missing blended terrain noise {id}"))
                .sample(x, y, z),
            Self::Noise(id, xs, ys) => t
                .noises
                .get(id)
                .unwrap_or_else(|| panic!("missing terrain noise {id}"))
                .sample(x * xs, y * ys, z * xs),
            Self::Pass(a) => s(a),
            Self::FlatCache(a) => t.sample_cached(
                a,
                (x as i32).div_euclid(4) as f64 * 4.,
                0.,
                (z as i32).div_euclid(4) as f64 * 4.,
                depth + 1,
            ),
            Self::Interpolated(a) => t.interpolate(a, x, y, z, depth + 1),
            Self::Add(a, b) => s(a) + s(b),
            Self::Mul(a, b) => {
                let v = s(a);
                if v == 0. { 0. } else { v * s(b) }
            }
            Self::Min(a, b) => s(a).min(s(b)),
            Self::Max(a, b) => s(a).max(s(b)),
            Self::Abs(a) => s(a).abs(),
            Self::Square(a) => {
                let v = s(a);
                v * v
            }
            Self::Cube(a) => {
                let v = s(a);
                v * v * v
            }
            Self::HalfNegative(a) => {
                let v = s(a);
                if v < 0. { v * 0.5 } else { v }
            }
            Self::QuarterNegative(a) => {
                let v = s(a);
                if v < 0. { v * 0.25 } else { v }
            }
            Self::Shifted(id, xz, ys, sx, sy, sz) => t
                .noises
                .get(id)
                .unwrap_or_else(|| panic!("missing terrain noise {id}"))
                .sample(x * xz + s(sx), y * ys + s(sy), z * xz + s(sz)),
            Self::Spline(coord, points) => sample_spline(coord, points, t, x, y, z, depth),
            Self::WeirdScaled(input, id, type1) => {
                let v = s(input);
                let scale = if *type1 {
                    if v < -0.5 {
                        0.75
                    } else if v < 0. {
                        1.
                    } else if v < 0.5 {
                        1.5
                    } else {
                        2.
                    }
                } else if v < -0.75 {
                    0.5
                } else if v < -0.5 {
                    0.75
                } else if v < 0.5 {
                    1.
                } else if v < 0.75 {
                    2.
                } else {
                    3.
                };
                scale
                    * t.noises
                        .get(id)
                        .unwrap_or_else(|| panic!("missing terrain noise {id}"))
                        .sample(x / scale, y / scale, z / scale)
                        .abs()
            }
            Self::Clamp(a, lo, hi) => s(a).clamp(*lo, *hi),
            Self::Squeeze(a) => {
                let v = s(a).clamp(-1., 1.);
                v / 2. - v * v * v / 24.
            }
            Self::Gradient(a, b, c, d) => {
                if y <= *a {
                    *c
                } else if y >= *b {
                    *d
                } else {
                    c + (y - a) / (b - a) * (d - c)
                }
            }
            Self::Range(input, lo, hi, inside, outside) => {
                let v = s(input);
                if v >= *lo && v < *hi {
                    s(inside)
                } else {
                    s(outside)
                }
            }
        }
    }
}

fn sample_spline(
    coordinate: &Node,
    points: &[(f32, f32, Box<Node>)],
    terrain: &Terrain,
    x: f64,
    y: f64,
    z: f64,
    depth: u8,
) -> f64 {
    let f = coordinate.sample(terrain, x, y, z, depth + 1) as f32;
    let mut previous = None;
    for (i, point) in points.iter().enumerate() {
        if point.0 > f {
            let (loc, deriv, value) = previous.unwrap_or(point);
            if previous.is_none() {
                return (value.sample(terrain, x, y, z, depth + 1) as f32 + deriv * (f - loc))
                    as f64;
            }
            let (next_loc, next_deriv, next_value) = point;
            let span = next_loc - loc;
            let t = (f - loc) / span;
            let v0 = value.sample(terrain, x, y, z, depth + 1) as f32;
            let v1 = next_value.sample(terrain, x, y, z, depth + 1) as f32;
            let d0 = deriv * span - (v1 - v0);
            let d1 = -next_deriv * span + (v1 - v0);
            return (v0 + t * (v1 - v0) + t * (1. - t) * (d0 + t * (d1 - d0))) as f64;
        }
        previous = Some(point);
        if i + 1 == points.len() {
            return (point.2.sample(terrain, x, y, z, depth + 1) as f32 + point.1 * (f - point.0))
                as f64;
        }
    }
    0.0
}

fn build_blended(
    seed: i64,
    v: &Value,
    defs: &HashMap<String, Value>,
    out: &mut HashMap<String, BlendedNoise>,
) {
    if let Some(id) = v.as_str() {
        if let Some(def) = defs.get(id) {
            build_blended(seed, def, defs, out)
        }
        return;
    }
    if let Some(o) = v.as_object() {
        if o.get("type").and_then(Value::as_str) == Some("minecraft:old_blended_noise") {
            let id = blend_key(v);
            if !out.contains_key(&id) {
                out.insert(id, BlendedNoise::new(seed, v));
            }
        }
        for x in o.values() {
            build_blended(seed, x, defs, out)
        }
    } else if let Some(a) = v.as_array() {
        for x in a {
            build_blended(seed, x, defs, out)
        }
    }
}

struct BlendedNoise {
    min: Vec<Option<ImprovedNoise>>,
    max: Vec<Option<ImprovedNoise>>,
    main: Vec<Option<ImprovedNoise>>,
    xz: f64,
    y: f64,
    xzf: f64,
    yf: f64,
    smear: f64,
}
impl BlendedNoise {
    fn new(seed: i64, v: &Value) -> Self {
        let mut r = noise::positional_named_source(seed, "minecraft:terrain");
        let min = legacy_octaves(&mut r, -15, 16);
        let max = legacy_octaves(&mut r, -15, 16);
        let main = legacy_octaves(&mut r, -7, 8);
        Self {
            min,
            max,
            main,
            xz: v["xz_scale"].as_f64().unwrap(),
            y: v["y_scale"].as_f64().unwrap(),
            xzf: v["xz_factor"].as_f64().unwrap(),
            yf: v["y_factor"].as_f64().unwrap(),
            smear: v["smear_scale_multiplier"].as_f64().unwrap(),
        }
    }
    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let (sx, sy, sz) = (
            x * 684.412 * self.xz,
            y * 684.412 * self.y,
            z * 684.412 * self.xz,
        );
        let (mx, my, mz) = (sx / self.xzf, sy / self.yf, sz / self.xzf);
        let mut main = 0.;
        let mut f = 1.;
        for n in self.main.iter().rev() {
            if let Some(n) = n {
                main += n.sample_with_y_offset(
                    wrap(mx * f),
                    wrap(my * f),
                    wrap(mz * f),
                    684.412 * self.y * self.smear / self.yf * f,
                    sy / self.yf * f,
                ) / f;
            }
            f *= 0.5;
        }
        let alpha = (main / 10. + 1.) / 2.;
        let mut lo = 0.;
        let mut hi = 0.;
        f = 1.;
        for i in 0..16 {
            let xx = wrap(sx * f);
            let yy = wrap(sy * f);
            let zz = wrap(sz * f);
            if alpha < 1. {
                if let Some(n) = &self.min[15 - i] {
                    lo += n.sample_with_y_offset(
                        xx,
                        yy,
                        zz,
                        684.412 * self.y * self.smear * f,
                        sy * f,
                    ) / f;
                }
            }
            if alpha > 0. {
                if let Some(n) = &self.max[15 - i] {
                    hi += n.sample_with_y_offset(
                        xx,
                        yy,
                        zz,
                        684.412 * self.y * self.smear * f,
                        sy * f,
                    ) / f;
                }
            }
            f *= 0.5;
        }
        lerp(alpha.clamp(0., 1.), lo / 512., hi / 512.) / 128.
    }
}
fn legacy_octaves(r: &mut Xoroshiro, first: i32, len: usize) -> Vec<Option<ImprovedNoise>> {
    let mut a: Vec<Option<ImprovedNoise>> = vec![None; len];
    for index in (0..len).rev() {
        let octave = first + index as i32;
        let _ = octave;
        a[index] = Some(ImprovedNoise::new(r));
    }
    a
}
fn blend_key(v: &Value) -> String {
    format!(
        "inline:{}:{}:{}:{}:{}",
        v["xz_scale"], v["y_scale"], v["xz_factor"], v["y_factor"], v["smear_scale_multiplier"]
    )
}
#[inline]
fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}
#[inline]
fn wrap(v: f64) -> f64 {
    v - (v / 33_554_432. + 0.5).floor() * 33_554_432.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_envelopes_preserve_shelves_between_empty_cell_endpoints() {
        let mut terrain = Terrain::new(0, Dimension::Moon);
        terrain.root = Node::Min(
            Box::new(Node::Gradient(0., 8., -2., 6.)),
            Box::new(Node::Gradient(0., 8., 6., -2.)),
        );
        assert_eq!(terrain.base_height(0, 0), Some(5.));
        assert!(!terrain.root.positive(&terrain, 0., 0., 0., 0));
        assert!(!terrain.root.positive(&terrain, 0., 8., 0., 0));
    }

    #[test]
    fn blended_noise_matches_minecraft_resource_seed_and_octave_order() {
        // Minecraft 1.20.1 BlendedNoise with Xoroshiro(0).forkPositional()
        // .fromHashOf(new ResourceLocation("terrain")), captured without launching a world.
        let noise = BlendedNoise::new(
            0,
            &serde_json::json!({
                "xz_scale": 0.25, "y_scale": 0.2,
                "xz_factor": 80., "y_factor": 90., "smear_scale_multiplier": 8.
            }),
        );
        for (point, expected) in [
            ([338., 104., 512.], -0.091233653191165470),
            ([-24., 88., 648.], 0.19671770487928720),
            ([0., 0., 0.], 0.022578701874563640),
            ([1000., 128., -592.], -0.051541094140978905),
            ([-2048., -16., -4096.], -0.068416880223806790),
        ] {
            let actual = noise.sample(point[0], point[1], point[2]);
            assert!(
                (actual - expected).abs() < 1e-13,
                "{point:?}: {actual} != {expected}"
            );
        }
    }

    #[test]
    fn seed_zero_surface_matches_saved_height_samples() {
        let fixture: Value = serde_json::from_str(include_str!("seed0_biomes.json")).unwrap();
        for (dim, name) in [
            (Dimension::Moon, "ad_astra:moon"),
            (Dimension::Mars, "ad_astra:mars"),
        ] {
            let saved = fixture["dimensions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["dimension"] == name)
                .unwrap();
            let terrain = super::super::Planetary::new(0, dim);
            let samples = saved["heights"].as_array().unwrap();
            let started = std::time::Instant::now();
            let mut errors = Vec::new();
            let mut bias = 0.0;
            for sample in samples.iter() {
                let x = sample["x"].as_i64().unwrap() as i32;
                let z = sample["z"].as_i64().unwrap() as i32;
                let expected = sample["y"]
                    .as_f64()
                    .or_else(|| sample["y"].as_i64().map(|v| v as f64))
                    .unwrap();
                let got = terrain
                    .height(x, z)
                    .unwrap_or_else(|| panic!("no terrain at {x},{z}"));
                errors.push((got - expected).abs());
                bias += got - expected;
                assert!(
                    got >= dim.min_y() as f64 && got <= (dim.min_y() + dim.height() - 1) as f64,
                    "height out of physical bounds: {dim:?} {x},{z} -> {got}"
                );
            }
            errors.sort_by(f64::total_cmp);
            let mean = errors.iter().sum::<f64>() / errors.len() as f64;
            bias /= errors.len() as f64;
            let p95 = errors[(errors.len() * 95 / 100).min(errors.len() - 1)];
            eprintln!(
                "terrain {name}: n={} MAE={mean:.2} bias={bias:+.2} p95={p95:.2} max={:.2} us/column={:.0}",
                errors.len(),
                errors.last().unwrap(),
                started.elapsed().as_secs_f64() * 1_000_000.0 / errors.len() as f64
            );
            assert!(
                mean < 1.0 && p95 <= 3.0,
                "planetary mean height error must stay below one block across every reference column"
            );
        }
    }

    #[test]
    fn terrain_height_depends_on_nonzero_and_negative_seeds() {
        for dimension in [Dimension::Moon, Dimension::Mars] {
            let terrains = [-1, 0, 1].map(|seed| super::super::Planetary::new(seed, dimension));
            let probes = [(17, 31), (220, -141), (-583, 796), (1821, -322)];
            let changed = probes.iter().any(|&(x, z)| {
                let values = terrains.each_ref().map(|terrain| terrain.height(x, z));
                values[0] != values[1] || values[1] != values[2]
            });
            assert!(changed, "{dimension:?} terrain must vary with seed");
        }
    }
}
