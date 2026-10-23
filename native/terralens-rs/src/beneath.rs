//! Exact seed-based sampler for TerraFirmaGreg's Beneath dimension biomes.
//!
//! This ports the dimension's legacy random noise routing and climate search. It predicts the
//! seed's biome map; a parsed chunk biome palette may validate that prediction, while close-up
//! surface display reads actual saved blocks. The map sampler itself never reads save data.

use crate::random::JavaRandom;
use serde_json::Value;
use std::collections::HashMap;

const DIMENSION: &str = include_str!("beneath/dimension.json");

const BIOMES: [&str; 11] = [
    "tfg:nether/webbed_lair",
    "tfg:nether/salt_caves",
    "tfg:nether/muggy_bog",
    "tfg:nether/decaying_caverns",
    "tfg:nether/lush_hollow",
    "tfg:nether/geothermal_springs",
    "tfg:nether/thorny_brambles",
    "tfg:nether/tar_pools",
    "tfg:nether/lava_floes",
    "tfg:nether/ash_forest",
    "tfg:nether/basalt_deltas",
];

#[derive(Clone, Copy)]
struct Bounds {
    min: i32,
    max: i32,
}
struct ClimateEntry {
    id: u16,
    axes: [Bounds; 6],
    offset_sq: i64,
}

#[derive(Clone)]
struct ImprovedNoise {
    xo: f64,
    yo: f64,
    zo: f64,
    perm: [u8; 256],
}

impl ImprovedNoise {
    fn new(r: &mut JavaRandom) -> Self {
        let xo = r.next_double() * 256.0;
        let yo = r.next_double() * 256.0;
        let zo = r.next_double() * 256.0;
        let mut perm = [0u8; 256];
        for (i, p) in perm.iter_mut().enumerate() {
            *p = i as u8;
        }
        for i in 0..256 {
            let j = i + r.next_int_bound((256 - i) as i32) as usize;
            perm.swap(i, j);
        }
        Self { xo, yo, zo, perm }
    }
    #[inline]
    fn p(&self, n: i32) -> usize {
        self.perm[(n & 255) as usize] as usize
    }
    #[inline]
    fn grad(hash: usize, x: f64, y: f64, z: f64) -> f64 {
        let h = hash & 15;
        let u = if h < 8 { x } else { y };
        let v = if h < 4 {
            y
        } else if h == 12 || h == 14 {
            x
        } else {
            z
        };
        (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
    }
    #[inline]
    fn fade(t: f64) -> f64 {
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }
    #[inline]
    fn lerp(t: f64, a: f64, b: f64) -> f64 {
        a + t * (b - a)
    }
    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let xp = x + self.xo;
        let yp = y + self.yo;
        let zp = z + self.zo;
        let xi = xp.floor() as i32;
        let yi = yp.floor() as i32;
        let zi = zp.floor() as i32;
        let xd = xp - xi as f64;
        let yd = yp - yi as f64;
        let zd = zp - zi as f64;
        let a = self.p(xi) as i32;
        let b = self.p(xi.wrapping_add(1)) as i32;
        let aa = self.p(a.wrapping_add(yi)) as i32;
        let ab = self.p(a.wrapping_add(yi).wrapping_add(1)) as i32;
        let ba = self.p(b.wrapping_add(yi)) as i32;
        let bb = self.p(b.wrapping_add(yi).wrapping_add(1)) as i32;
        let n000 = Self::grad(self.p(aa.wrapping_add(zi)), xd, yd, zd);
        let n100 = Self::grad(self.p(ba.wrapping_add(zi)), xd - 1.0, yd, zd);
        let n010 = Self::grad(self.p(ab.wrapping_add(zi)), xd, yd - 1.0, zd);
        let n110 = Self::grad(self.p(bb.wrapping_add(zi)), xd - 1.0, yd - 1.0, zd);
        let n001 = Self::grad(
            self.p(aa.wrapping_add(zi).wrapping_add(1)),
            xd,
            yd,
            zd - 1.0,
        );
        let n101 = Self::grad(
            self.p(ba.wrapping_add(zi).wrapping_add(1)),
            xd - 1.0,
            yd,
            zd - 1.0,
        );
        let n011 = Self::grad(
            self.p(ab.wrapping_add(zi).wrapping_add(1)),
            xd,
            yd - 1.0,
            zd - 1.0,
        );
        let n111 = Self::grad(
            self.p(bb.wrapping_add(zi).wrapping_add(1)),
            xd - 1.0,
            yd - 1.0,
            zd - 1.0,
        );
        let u = Self::fade(xd);
        let v = Self::fade(yd);
        let w = Self::fade(zd);
        Self::lerp(
            w,
            Self::lerp(v, Self::lerp(u, n000, n100), Self::lerp(u, n010, n110)),
            Self::lerp(v, Self::lerp(u, n001, n101), Self::lerp(u, n011, n111)),
        )
    }
}

#[derive(Clone)]
struct Perlin {
    levels: Vec<Option<ImprovedNoise>>,
    amps: Vec<f64>,
    input: f64,
    value: f64,
}
impl Perlin {
    fn new(r: &mut JavaRandom, first: i32, amps: &[f64]) -> Self {
        let positional_seed = r.next_long();
        let mut levels = Vec::with_capacity(amps.len());
        for (i, amp) in amps.iter().enumerate() {
            if *amp == 0.0 {
                levels.push(None);
                continue;
            }
            let name = format!("octave_{}", first + i as i32);
            let seed = (java_hash(&name) as i64) ^ positional_seed;
            levels.push(Some(ImprovedNoise::new(&mut JavaRandom::new(seed))));
        }
        let input = 2f64.powi(first);
        let value = 2f64.powi(amps.len() as i32 - 1) / (2f64.powi(amps.len() as i32) - 1.0);
        Self {
            levels,
            amps: amps.to_vec(),
            input,
            value,
        }
    }
    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut total = 0.0;
        let mut freq = self.input;
        let mut amp = self.value;
        for (noise, weight) in self.levels.iter().zip(&self.amps) {
            if let Some(noise) = noise {
                total +=
                    weight * noise.sample(wrap(x * freq), wrap(y * freq), wrap(z * freq)) * amp;
            }
            freq *= 2.0;
            amp *= 0.5;
        }
        total
    }
}

#[derive(Clone)]
struct NormalNoise {
    a: Perlin,
    b: Perlin,
    factor: f64,
}
impl NormalNoise {
    fn new(random: &mut JavaRandom, first: i32, amps: &[f64]) -> Self {
        let a = Perlin::new(random, first, amps);
        let b = Perlin::new(random, first, amps);
        let active: Vec<usize> = amps
            .iter()
            .enumerate()
            .filter_map(|(i, a)| (*a != 0.).then_some(i))
            .collect();
        let span = active
            .last()
            .zip(active.first())
            .map(|(hi, lo)| hi - lo + 1)
            .unwrap_or(1);
        Self {
            a,
            b,
            factor: (1.0 / 6.0) / (0.1 * (1.0 + 1.0 / span as f64)),
        }
    }
    #[inline]
    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        (self.a.sample(x, y, z)
            + self.b.sample(
                x * 1.0181268882175227,
                y * 1.0181268882175227,
                z * 1.0181268882175227,
            ))
            * self.factor
    }
}

#[derive(Clone, Copy)]
struct RouterNoise {
    noise: usize,
    scale: f64,
}

/// Seed based exact sampler for TFG's 1.20.1 Beneath multi-noise biome source.
pub struct Beneath {
    noises: Vec<NormalNoise>,
    router: [RouterNoise; 6],
    entries: Vec<ClimateEntry>,
}

impl Beneath {
    pub fn new(seed: i64) -> Self {
        let data: Value = serde_json::from_str(DIMENSION).expect("embedded Beneath climate data");
        let noise_data = data["noises"].as_object().unwrap();
        let mut root = JavaRandom::new(seed);
        let positional_seed = root.next_long();
        // Constructor order is router evaluation order, not map order. This list mirrors TS.
        let names = [
            "tfg:nether_biome_bottom",
            "tfg:nether_forest_middle",
            "tfg:nether_biome_top",
            "tfg:nether_biome_middle",
            "tfg:nether_depth",
            "tfg:nether_forest_top",
            "minecraft:offset",
        ]
        .into_iter();
        let mut noises = Vec::new();
        let mut indexes = HashMap::new();
        for name in names {
            let params = if name == "minecraft:offset" {
                None
            } else {
                Some(noise_data.get(name).expect("noise in embedded router"))
            };
            let (first, amps) = if let Some(p) = params {
                (
                    p["firstOctave"].as_i64().unwrap() as i32,
                    p["amplitudes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_f64().unwrap())
                        .collect::<Vec<_>>(),
                )
            } else {
                (0, vec![0.0])
            };
            let seed = (java_hash(&name) as i64) ^ positional_seed;
            indexes.insert(name, noises.len());
            noises.push(NormalNoise::new(&mut JavaRandom::new(seed), first, &amps));
        }
        let router_names = [
            "tfg:nether_biome_bottom",
            "tfg:nether_forest_middle",
            "tfg:nether_biome_top",
            "tfg:nether_biome_middle",
            "tfg:nether_depth",
            "tfg:nether_forest_top",
        ];
        let scales = [0.6, 0.35, 0.6, 0.6, 0.6, 0.35];
        let router = std::array::from_fn(|i| RouterNoise {
            noise: indexes[router_names[i]],
            scale: scales[i],
        });
        let mut entries = Vec::new();
        for item in data["biomes"].as_array().unwrap() {
            let name = item["biome"].as_str().unwrap();
            let id = biome_id_by_name(name).expect("dimension biome is indexed");
            let p = &item["parameters"];
            let axes = [
                "temperature",
                "humidity",
                "continentalness",
                "erosion",
                "depth",
                "weirdness",
            ]
            .map(|axis| {
                let v = &p[axis];
                let (min, max) = if let Some(a) = v.as_array() {
                    (a[0].as_f64().unwrap(), a[1].as_f64().unwrap())
                } else {
                    (v.as_f64().unwrap(), v.as_f64().unwrap())
                };
                Bounds {
                    min: quantize(min),
                    max: quantize(max),
                }
            });
            let off = quantize(p["offset"].as_f64().unwrap_or(0.0)) as i64;
            entries.push(ClimateEntry {
                id,
                axes,
                offset_sq: off * off,
            });
        }
        Self {
            noises,
            router,
            entries,
        }
    }

    /// Biome at a block coordinate. Router noise is sampled at the quart cell's origin.
    pub fn biome_id(&self, x: i32, y: i32, z: i32) -> u16 {
        let qx = x >> 2;
        let qy = y >> 2;
        let qz = z >> 2;
        self.biome_id_quart(qx, qy, qz)
    }
    pub fn biome_id_quart(&self, qx: i32, qy: i32, qz: i32) -> u16 {
        let (x, y, z) = ((qx << 2) as f64, (qy << 2) as f64, (qz << 2) as f64);
        let mut target = [0i32; 6];
        for i in 0..6 {
            let r = self.router[i];
            let value = self.noises[r.noise].sample(x * r.scale, y * 0.0, z * r.scale);
            let raw = if i == 4 {
                value + clamped_gradient(y, 42., 170., 2., -2.)
            } else {
                value
            };
            target[i] = quantize(raw as f32 as f64);
        }
        let mut best = 0u16;
        let mut best_fitness = i64::MAX;
        for entry in &self.entries {
            let mut fitness = entry.offset_sq;
            for i in 0..6 {
                let delta = if target[i] < entry.axes[i].min {
                    entry.axes[i].min - target[i]
                } else if target[i] > entry.axes[i].max {
                    target[i] - entry.axes[i].max
                } else {
                    0
                };
                fitness += delta as i64 * delta as i64;
                if fitness >= best_fitness {
                    break;
                }
            }
            if fitness < best_fitness {
                best_fitness = fitness;
                best = entry.id;
            }
        }
        best
    }
}

pub fn biome_name(id: u16) -> &'static str {
    BIOMES
        .get(id.checked_sub(1000).unwrap_or(u16::MAX) as usize)
        .copied()
        .unwrap_or("tfg:nether/unknown")
}
pub fn biome_color(id: u16) -> [u8; 3] {
    match id {
        1000 => [90, 83, 117],
        1001 => [168, 145, 105],
        1002 => [94, 119, 85],
        1003 => [111, 89, 72],
        1004 => [77, 143, 88],
        1005 => [211, 112, 56],
        1006 => [137, 105, 71],
        1007 => [70, 66, 60],
        1008 => [221, 74, 35],
        1009 => [116, 106, 92],
        1010 => [83, 81, 86],
        _ => [128, 128, 128],
    }
}

fn biome_id_by_name(name: &str) -> Option<u16> {
    BIOMES
        .iter()
        .position(|n| *n == name)
        .map(|i| 1000 + i as u16)
}
fn quantize(v: f64) -> i32 {
    (v * 10000.0).trunc() as i32
}
fn clamped_gradient(y: f64, from: f64, to: f64, from_v: f64, to_v: f64) -> f64 {
    if y <= from {
        from_v
    } else if y >= to {
        to_v
    } else {
        from_v + ((y - from) / (to - from)) * (to_v - from_v)
    }
}
fn wrap(v: f64) -> f64 {
    v - (v / 33_554_432.0 + 0.5).floor() * 33_554_432.0
}
fn java_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Fixture {
        seed: String,
        samples: Vec<Sample>,
    }
    #[derive(Deserialize)]
    struct Sample {
        qx: i32,
        qy: i32,
        qz: i32,
        biome: String,
    }
    #[test]
    fn matches_real_beneath_palette_fixture() {
        let f: Fixture =
            serde_json::from_str(include_str!("beneath/real_biome_fixture.json")).unwrap();
        let w = Beneath::new(f.seed.parse().unwrap());
        for s in f.samples {
            assert_eq!(
                biome_name(w.biome_id_quart(s.qx, s.qy, s.qz)),
                s.biome,
                "quart {},{},{}",
                s.qx,
                s.qy,
                s.qz
            );
        }
    }
    #[test]
    fn block_coordinates_select_same_quart_cell() {
        let w = Beneath::new(-6696614430994881185);
        for (x, y, z) in [(-213, 212, 64), (-220, 8, -92), (-1, 0, -1)] {
            assert_eq!(
                w.biome_id(x, y, z),
                w.biome_id_quart(x >> 2, y >> 2, z >> 2)
            );
        }
    }
    #[test]
    fn biome_ids_and_colors_are_defined() {
        for id in 1000..=1010 {
            assert_ne!(biome_name(id), "tfg:nether/unknown");
            assert_ne!(biome_color(id), [128, 128, 128]);
        }
    }
}
