//! Seed based Moon and Mars biome maps for the installed TFG/Ad Astra datapacks.
//! The climate router and biomes are copied from extracted registry resources; sampled saved
//! palettes in the seed-zero fixture validate the port independently.
mod craters;
mod density;
pub mod features;
pub(crate) mod noise;
pub(super) mod terrain;

use crate::dimension::Dimension;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

const PROFILES: &str = include_str!("planetary/assets/climate_profiles.json");
#[cfg(test)]
const FIXTURE: &str = include_str!("planetary/seed0_biomes.json");

const MOON_BIOMES: [&str; 10] = [
    "tfg:moon/lunar_asurine_dense",
    "tfg:moon/lunar_asurine_sparse",
    "tfg:moon/lunar_chorus_dense",
    "tfg:moon/lunar_chorus_sparse",
    "tfg:moon/lunar_corals_dense",
    "tfg:moon/lunar_corals_sparse",
    "tfg:moon/lunar_lights_dense",
    "tfg:moon/lunar_lights_sparse",
    "tfg:moon/lunar_plains",
    "tfg:moon/lunar_sands",
];
const MARS_BIOMES: [&str; 14] = [
    "tfg:mars/amber_edge",
    "tfg:mars/amber_hills",
    "tfg:mars/amber_plains",
    "tfg:mars/martian_deep_desert",
    "tfg:mars/martian_dune_edge",
    "tfg:mars/martian_dunes",
    "tfg:mars/martian_mountains",
    "tfg:mars/martian_river",
    "tfg:mars/rusticus_edge",
    "tfg:mars/rusticus_hills",
    "tfg:mars/rusticus_plains",
    "tfg:mars/sangnum_edge",
    "tfg:mars/sangnum_hills",
    "tfg:mars/sangnum_plains",
];

#[derive(Clone, Copy)]
struct Range {
    min: i32,
    max: i32,
}
struct ClimateEntry {
    id: u16,
    axes: [Range; 6],
    offset_sq: i64,
}
const AXES: [&str; 6] = [
    "temperature",
    "humidity",
    "continentalness",
    "erosion",
    "depth",
    "weirdness",
];
const ROUTER: [&str; 6] = [
    "temperature",
    "vegetation",
    "continents",
    "erosion",
    "depth",
    "ridges",
];

/// Exact Xoroshiro/NormalNoise multi-noise biome source for Moon or Mars.
pub struct Planetary {
    seed: i64,
    dimension: Dimension,
    min_y: i32,
    noises: HashMap<String, noise::NormalNoise>,
    router: [density::Density; 6],
    entries: Vec<ClimateEntry>,
    terrain: OnceLock<terrain::Terrain>,
}
impl Planetary {
    pub fn new(seed: i64, dimension: Dimension) -> Self {
        assert!(
            dimension.is_planetary(),
            "Planetary sampler requires Moon or Mars"
        );
        let profile_name = if dimension == Dimension::Moon {
            "moon"
        } else {
            "mars"
        };
        let profiles: Value = serde_json::from_str(PROFILES).expect("planetary climate profiles");
        let profile = &profiles[profile_name];
        let mut noise_ids = HashSet::new();
        collect_noise_ids(&profile["router"], &mut noise_ids);
        let mut noises = HashMap::new();
        for name in noise_ids {
            let p = &profile["noises"][&name];
            assert!(
                !p.is_null(),
                "climate profile missing noise parameters for {name}"
            );
            let first = p["firstOctave"].as_i64().expect("noise firstOctave") as i32;
            let amplitudes = p["amplitudes"]
                .as_array()
                .expect("noise amplitudes")
                .iter()
                .map(|x| x.as_f64().expect("noise amplitude"))
                .collect::<Vec<_>>();
            noises.insert(
                name.clone(),
                noise::NormalNoise::new(seed, &name, first, &amplitudes),
            );
        }
        let router =
            std::array::from_fn(|i| density::Density::compile(&profile["router"][ROUTER[i]]));
        let names = biome_names(dimension);
        let mut entries = Vec::new();
        for item in profile["biomes"].as_array().expect("planet biomes") {
            let name = item["biome"].as_str().expect("biome id");
            let id = names
                .iter()
                .position(|n| *n == name)
                .expect("stable planetary biome id");
            let p = &item["parameters"];
            let axes = std::array::from_fn(|i| {
                let value = &p[AXES[i]];
                let (a, b) = if let Some(pair) = value.as_array() {
                    (pair[0].as_f64().unwrap(), pair[1].as_f64().unwrap())
                } else {
                    let v = value.as_f64().unwrap_or(0.);
                    (v, v)
                };
                Range {
                    min: quantize(a),
                    max: quantize(b),
                }
            });
            let offset = quantize(p["offset"].as_f64().unwrap_or(0.)) as i64;
            entries.push(ClimateEntry {
                id: (if dimension == Dimension::Moon {
                    2000
                } else {
                    2100
                }) + id as u16,
                axes,
                offset_sq: offset * offset,
            });
        }
        Self {
            seed,
            dimension,
            min_y: profile["minY"].as_i64().unwrap() as i32,
            noises,
            router,
            entries,
            terrain: OnceLock::new(),
        }
    }
    /// Surface biome map sample. Planet climate fields are configured with zero y scale.
    pub fn biome_id(&self, x: i32, z: i32) -> u16 {
        self.biome_id_at(x, self.min_y, z)
    }
    /// Biome sample at a block position, rounded down to Minecraft's quart cell origin.
    pub fn biome_id_at(&self, x: i32, y: i32, z: i32) -> u16 {
        let (x, y, z) = (
            (x >> 2 << 2) as f64,
            (y >> 2 << 2) as f64,
            (z >> 2 << 2) as f64,
        );
        let climate = std::array::from_fn::<_, 6, _>(|i| {
            quantize(self.router[i].sample(&self.noises, x, y, z) as f32 as f64)
        });
        nearest_entry(&self.entries, climate, biome_tree_order(self.dimension))
    }
    /// Surface height including supported crater carving, initialized only on demand.
    pub fn height(&self, x: i32, z: i32) -> Option<f64> {
        self.terrain
            .get_or_init(|| terrain::Terrain::new(self.seed, self.dimension))
            .height(x, z, &|cx, cz| self.crater_eligible(cx, cz))
    }

    fn crater_eligible(&self, chunk_x: i32, chunk_z: i32) -> bool {
        // Vanilla decoration uses the union of quart biomes in the surrounding 3x3 chunks.
        // Planetary climate is Y-independent. Lunar chorus dense is the sole Moon biome
        // whose crater-feature list is empty; any other biome in the union enables the pass.
        for z in (chunk_z * 16 - 16..chunk_z * 16 + 32).step_by(4) {
            for x in (chunk_x * 16 - 16..chunk_x * 16 + 32).step_by(4) {
                if self.biome_id(x, z) != 2002 {
                    return true;
                }
            }
        }
        false
    }
}

pub fn biome_ids(dimension: Dimension) -> Vec<u16> {
    let start = if dimension == Dimension::Moon {
        2000
    } else {
        2100
    };
    (0..biome_names(dimension).len())
        .map(|i| start + i as u16)
        .collect()
}
pub fn biome_name(id: u16) -> &'static str {
    if (2000..2010).contains(&id) {
        MOON_BIOMES[(id - 2000) as usize]
    } else if (2100..2114).contains(&id) {
        MARS_BIOMES[(id - 2100) as usize]
    } else {
        "unknown"
    }
}
pub fn biome_color(id: u16) -> [u8; 3] {
    match id {
        2000 => [142, 104, 175],
        2001 => [112, 82, 147],
        2002 => [132, 170, 205],
        2003 => [102, 139, 177],
        2004 => [76, 164, 169],
        2005 => [59, 130, 138],
        2006 => [233, 211, 133],
        2007 => [198, 174, 107],
        2008 => [135, 135, 139],
        2009 => [183, 169, 150],
        2100 => [198, 133, 67],
        2101 => [174, 111, 59],
        2102 => [208, 150, 79],
        2103 => [158, 91, 49],
        2104 => [199, 133, 77],
        2105 => [183, 103, 52],
        2106 => [130, 86, 71],
        2107 => [78, 119, 139],
        2108 => [143, 77, 57],
        2109 => [126, 72, 58],
        2110 => [163, 91, 61],
        2111 => [127, 62, 67],
        2112 => [108, 61, 66],
        2113 => [145, 67, 68],
        _ => [128, 128, 128],
    }
}

fn biome_names(dimension: Dimension) -> &'static [&'static str] {
    if dimension == Dimension::Moon {
        &MOON_BIOMES
    } else {
        &MARS_BIOMES
    }
}
fn quantize(v: f64) -> i32 {
    (v * 10000.).trunc() as i32
}

/// `Climate.ParameterList` flattens its `RTree` leaves in a stable order when it builds the tree.
/// These permutations were captured by constructing Minecraft 1.20.1's actual
/// `Climate.ParameterList` from the bundled TFG 0.13.8 parameters, then reflecting the resulting
/// `Climate.RTree` leaf order. Oracle source: local Forge 1.20.1-47.4.13 joined/SRG jar
/// SHA-256 `e40da17a48e588f7da97705f7b6c61e4b6b3800f343890f4213f597e6f3bc809`; climate profile
/// SHA-256 `69c6aec2d3bc33842604582c81799f037fe32e4dd8ef8d9e2ec6219a5ebc3974`. Fitness ties retain
/// the first leaf visited (`<`, never `<=`); this matters at shared biome-box endpoints.
fn biome_tree_order(dimension: Dimension) -> &'static [usize] {
    const MOON: [usize; 10] = [4, 5, 6, 8, 7, 9, 3, 1, 2, 0];
    const MARS: [usize; 27] = [
        14, 13, 16, 18, 15, 17, 6, 8, 7, 10, 12, 9, 20, 25, 22, 24, 21, 11, 4, 2, 1, 5, 3, 0, 19,
        26, 23,
    ];
    if dimension == Dimension::Moon {
        &MOON
    } else {
        &MARS
    }
}

fn nearest_entry(entries: &[ClimateEntry], climate: [i32; 6], order: &[usize]) -> u16 {
    let mut best = 0;
    let mut score = i64::MAX;
    for &index in order {
        let Some(entry) = entries.get(index) else {
            continue;
        };
        let mut distance = entry.offset_sq;
        for (value, axis) in climate.iter().zip(entry.axes) {
            let delta = if *value < axis.min {
                axis.min - value
            } else if *value > axis.max {
                value - axis.max
            } else {
                0
            };
            distance += delta as i64 * delta as i64;
            if distance >= score {
                break;
            }
        }
        if distance < score {
            score = distance;
            best = entry.id;
        }
    }
    best
}
fn collect_noise_ids(value: &Value, out: &mut HashSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(name) = map.get("noise").and_then(Value::as_str) {
                out.insert(name.to_owned());
            }
            if map.get("type").and_then(Value::as_str).is_some_and(|kind| {
                matches!(
                    kind,
                    "minecraft:shift" | "minecraft:shift_a" | "minecraft:shift_b"
                )
            }) && let Some(name) = map.get("argument").and_then(Value::as_str)
            {
                out.insert(name.to_owned());
            }
            for v in map.values() {
                collect_noise_ids(v, out);
            }
        }
        Value::Array(values) => {
            for v in values {
                collect_noise_ids(v, out)
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_ids_cover_all_planet_biomes() {
        assert_eq!(biome_ids(Dimension::Moon).len(), 10);
        assert_eq!(biome_ids(Dimension::Mars).len(), 14);
        for id in biome_ids(Dimension::Moon)
            .into_iter()
            .chain(biome_ids(Dimension::Mars))
        {
            assert_ne!(biome_name(id), "unknown");
            assert_ne!(biome_color(id), [128, 128, 128]);
        }
    }
    #[test]
    fn vanilla_tree_order_controls_equal_fitness_ties() {
        let zero = Range { min: 0, max: 0 };
        let make_entry = |id| ClimateEntry {
            id,
            axes: [zero; 6],
            offset_sq: 0,
        };
        // The order is the order visited by the Java Climate.RTree for this exact two-box tree.
        // Both entries fit equally, so the first visited leaf wins.
        assert_eq!(
            nearest_entry(&[make_entry(10), make_entry(11)], [0; 6], &[1, 0]),
            11
        );

        // At the Mars shared endpoint the installed tree visits the dunes leaf before deep desert,
        // although the source parameter records list deep desert first.
        let mars = Planetary::new(0, Dimension::Mars);
        assert_eq!(
            nearest_entry(
                &mars.entries,
                [0, 349, -4550, 2260, 0, 0],
                biome_tree_order(Dimension::Mars)
            ),
            2105
        );

        let direct = Planetary::new(0, Dimension::Mars).biome_id_at(840, 80, -272);
        let reordered = Planetary::new(0, Dimension::Mars);
        let _ = reordered.biome_id_at(500, 100, 500);
        let after_other_query = reordered.biome_id_at(840, 80, -272);
        assert_eq!(direct, 2105);
        assert_eq!(after_other_query, direct);
    }
    #[test]
    fn matches_seed_zero_saved_planet_palettes() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            seed: i64,
            dimensions: Vec<DimFixture>,
        }
        #[derive(serde::Deserialize)]
        struct DimFixture {
            dimension: String,
            samples: Vec<Sample>,
        }
        #[derive(serde::Deserialize)]
        struct Sample {
            x: i32,
            y: i32,
            z: i32,
            biome: String,
        }
        let f: Fixture = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(f.seed, 0);
        for (id, dim) in [
            ("ad_astra:moon", Dimension::Moon),
            ("ad_astra:mars", Dimension::Mars),
        ] {
            let sampler = Planetary::new(f.seed, dim);
            let data = f.dimensions.iter().find(|d| d.dimension == id).unwrap();
            for s in &data.samples {
                assert_eq!(
                    biome_name(sampler.biome_id_at(s.x, s.y, s.z)),
                    s.biome,
                    "{id} at {},{},{}",
                    s.x,
                    s.y,
                    s.z
                );
            }
        }
    }
}
