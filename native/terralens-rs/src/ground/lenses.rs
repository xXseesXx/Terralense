//! Surface exposure of TFC's intrusive rock dikes.
//!
//! These are registered as `tfc:pipe_vein` features rather than as part of the
//! host-rock layer sampler. The host sampler remains useful for ore eligibility;
//! this module overlays the deterministic placed-feature result on surface
//! blocks that the dike configurations can actually replace.

use crate::{
    random::Xoroshiro,
    vegetation::{self, FeatureRandom},
    world::World,
};
use lru::LruCache;
use serde_json::Value;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex, OnceLock},
};

const VEINS: &str = include_str!("../../assets/data/tfg/veins.json");
const FEATURE_STEP: i32 = 6;
const WORLDGEN_ORIGIN_Y: i32 = -64;
const FEATURES: [&str; 3] = [
    "tfc:vein/granite_dike",
    "tfc:vein/diorite_dike",
    "tfc:vein/gabbro_dike",
];

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct CacheKey {
    seed: i64,
    settings: [u64; 6],
    cx: i32,
    cz: i32,
}

#[derive(Clone)]
struct Pipe {
    id: String,
    random_name: String,
    rarity: i32,
    density: f32,
    radius: i32,
    height: i32,
    min_y: i32,
    max_y: i32,
    min_skew: i32,
    max_skew: i32,
    min_slant: i32,
    max_slant: i32,
    sign: f32,
    replacements: HashMap<String, Arc<str>>,
}

#[derive(Clone, Copy)]
struct Dike {
    x: i32,
    y: i32,
    z: i32,
    sign: f64,
    skew_x: f32,
    skew_z: f32,
    skew: i32,
    slant: i32,
    radius: i32,
    height: i32,
}

impl Dike {
    fn contains(self, x: i32, y: i32, z: i32) -> bool {
        let dy = y - self.y;
        if dy.abs() >= self.height {
            return false;
        }
        let ys = dy as f64 / self.height as f64;
        let dx = ((x - self.x) as f64 + f64::from(self.skew as f32 * self.skew_x) * ys) as i32;
        let dz = ((z - self.z) as f64 + f64::from(self.skew as f32 * self.skew_z) * ys) as i32;
        let yf = self.sign * ys + 0.5;
        let radius = self.radius as f64 * (1.0 - yf) + (self.radius - self.slant) as f64 * yf;
        f64::from(dx * dx + dz * dz) < radius * radius
    }
}

fn pipes() -> &'static [Pipe] {
    static PIPES: OnceLock<Vec<Pipe>> = OnceLock::new();
    PIPES.get_or_init(|| {
        let data: Value = serde_json::from_str(VEINS).expect("bundled vein data is valid");
        FEATURES
            .iter()
            .filter_map(|feature| {
                let id = feature.rsplit('/').next()?;
                let raw = data["veins"][id].as_object()?;
                let replacements = raw["blocks"]
                    .as_object()?
                    .iter()
                    .filter_map(|(state, choices)| {
                        let output = choices.as_array()?.first()?["block"].as_str()?;
                        Some((state.clone(), Arc::<str>::from(output)))
                    })
                    .collect();
                Some(Pipe {
                    id: feature.to_string(),
                    random_name: raw["random_name"].as_str()?.to_string(),
                    rarity: raw["rarity"].as_i64()? as i32,
                    density: raw["density"].as_f64()? as f32,
                    radius: raw["radius"].as_i64()? as i32,
                    height: raw["height"].as_i64()? as i32,
                    min_y: raw["min_y"].as_i64()? as i32,
                    max_y: raw["max_y"].as_i64()? as i32,
                    min_skew: raw["min_skew"].as_i64()? as i32,
                    max_skew: raw["max_skew"].as_i64()? as i32,
                    min_slant: raw["min_slant"].as_i64()? as i32,
                    max_slant: raw["max_slant"].as_i64()? as i32,
                    sign: raw["sign"].as_f64()? as f32,
                    replacements,
                })
            })
            .collect()
    })
}

/// Return the block written by the intrusive dike feature at this surface
/// position, if the source surface state is one of the dike's replaceable rocks.
/// The result is cached per generated chunk; it includes exact density draws and
/// honors the placed-feature order shared by TFC's biome decorator.
pub(super) fn replace_surface_block(
    world: &World,
    x: i32,
    y: i32,
    z: i32,
    source_block: &str,
) -> Option<Arc<str>> {
    if !is_replaceable_rock(source_block)
        || !pipes()
            .iter()
            .any(|pipe| (pipe.min_y..=pipe.max_y).contains(&y))
    {
        return None;
    }
    let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
    let overlay = overlay_for_chunk(world, cx, cz)?;
    overlay
        .get(&(x, y, z))
        .and_then(|index| pipes().get(*index))
        .and_then(|pipe| pipe.replacements.get(source_block))
        .cloned()
}

fn is_replaceable_rock(block: &str) -> bool {
    ["tfc:rock/raw/", "tfc:rock/gravel/", "tfc:rock/hardened/"]
        .iter()
        .any(|prefix| block.starts_with(prefix))
}

fn overlay_for_chunk(
    world: &World,
    cx: i32,
    cz: i32,
) -> Option<Arc<HashMap<(i32, i32, i32), usize>>> {
    type Overlay = Arc<HashMap<(i32, i32, i32), usize>>;
    // Results contain only exposed hits. Sharing this small bounded cache lets
    // another engine worker reuse them after a pan or layer/style change.
    static CACHE: OnceLock<Mutex<LruCache<CacheKey, Overlay>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(LruCache::new(NonZeroUsize::new(1024).unwrap())));
    let settings = &world.region.settings;
    let key = CacheKey {
        seed: world.seed,
        settings: [
            settings.temperature_scale.to_bits(),
            settings.temperature_constant.to_bits(),
            settings.rainfall_scale.to_bits(),
            settings.rainfall_constant.to_bits(),
            settings.continentalness.to_bits(),
            settings.grass_density.to_bits(),
        ],
        cx,
        cz,
    };
    if let Some(value) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned()
    {
        return Some(value);
    }
    let value = Arc::new(generate_chunk::<true>(world, cx, cz)?);
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .put(key, Arc::clone(&value));
    Some(value)
}

fn generate_chunk<const TRIM_TAIL: bool>(
    world: &World,
    cx: i32,
    cz: i32,
) -> Option<HashMap<(i32, i32, i32), usize>> {
    let mut out = HashMap::<(i32, i32, i32), usize>::new();
    let pipes = pipes();
    let chunk_min_x = cx.wrapping_mul(16);
    let chunk_min_z = cz.wrapping_mul(16);
    let mut active = Vec::new();
    for (pipe_index, pipe) in pipes.iter().enumerate() {
        if !vegetation::feature_biome_allowed(world, cx, cz, &pipe.id) {
            continue;
        }
        let index = vegetation::feature_index_in_step(&pipe.id, FEATURE_STEP)?;
        let veins = nearby_dikes(world.seed, cx, cz, pipe)
            .into_iter()
            .filter(|dike| {
                dike.y + dike.height >= pipe.min_y
                    && dike.y - dike.height <= pipe.max_y
                    && dike.x + dike.radius + dike.skew >= chunk_min_x
                    && dike.x - dike.radius - dike.skew <= chunk_min_x + 15
                    && dike.z + dike.radius + dike.skew >= chunk_min_z
                    && dike.z - dike.radius - dike.skew <= chunk_min_z + 15
            })
            .collect::<Vec<_>>();
        if !veins.is_empty() {
            active.push((pipe_index, pipe, index, veins));
        }
    }
    if active.is_empty() {
        return Some(out);
    }
    // The rendered layer needs only blocks on the modeled visible surface. We
    // still consume the source feature's full RNG stream below; keeping other
    // underground placed voxels out of the cache bounds memory per chunk.
    let mut source_samples = Vec::with_capacity(256);
    let mut stratified = Vec::with_capacity(256);
    for dx in 0..16 {
        for dz in 0..16 {
            let x = chunk_min_x + dx;
            let z = chunk_min_z + dz;
            source_samples.push(crate::ground::source_sample(world, x, z));
            stratified.push(crate::ground::dike_source_stratified(world, x, z));
        }
    }
    // Nearby pipes often pass entirely below a chunk or beneath soil. If none
    // can touch a replaceable surface block, their internal RNG stream cannot
    // affect this surface-only result and no voxel replay is needed.
    active.retain(|(_, pipe, _, veins)| {
        source_samples.iter().enumerate().any(|(column, sample)| {
            let Some(sample) = sample else {
                return false;
            };
            if !(pipe.min_y..=pipe.max_y).contains(&sample.y)
                || !pipe.replacements.contains_key(sample.block.as_ref())
            {
                return false;
            }
            let x = chunk_min_x + (column / 16) as i32;
            let z = chunk_min_z + (column % 16) as i32;
            veins.iter().any(|dike| dike.contains(x, sample.y, z))
        })
    });
    if active.is_empty() {
        return Some(out);
    }
    // Every installed dike replaces the same 60 rock states, and its outputs
    // remain replaceable by the next dike. Only eligibility is needed for the
    // provider's RNG draw. A compact tri-state grid avoids thousands of rock
    // strings, Arc allocations and 3D hash entries during this replay.
    let min_source_y = pipes.iter().map(|p| p.min_y).min()?;
    let max_source_y = pipes.iter().map(|p| p.max_y).max()?;
    let column_height = (max_source_y - min_source_y + 1) as usize;
    let mut source_states = vec![0u8; 256 * column_height];
    let basal = basal_columns(cx, cz);
    for (pipe_index, pipe, index, veins) in active {
        // This feature has an independent RNG seed. Its stream is needed only
        // through its last possible surface write, including all preceding
        // veins and underground provider draws. Later draws cannot affect any
        // exposed block or any other feature's independently seeded stream.
        let mut last_write = None;
        for (vein_index, dike) in veins.iter().enumerate() {
            for (column, sample) in source_samples.iter().enumerate() {
                let Some(sample) = sample else {
                    continue;
                };
                let x = chunk_min_x + (column / 16) as i32;
                let z = chunk_min_z + (column % 16) as i32;
                if (pipe.min_y..=pipe.max_y).contains(&sample.y)
                    && pipe.replacements.contains_key(sample.block.as_ref())
                    && dike.contains(x, sample.y, z)
                {
                    last_write = last_write.max(Some((vein_index, x, z, sample.y)));
                }
            }
        }
        let Some(last_write) = last_write else {
            continue;
        };
        let mut random = FeatureRandom::new(vegetation::feature_seed(
            cx,
            cz,
            WORLDGEN_ORIGIN_Y,
            FEATURE_STEP,
            index,
        ));
        'veins: for (vein_index, dike) in veins.into_iter().enumerate() {
            // VeinFeature's bounding box is radius + skew horizontally and
            // height vertically; all positions in its chunk intersection draw
            // a float, including points outside the pipe body.
            let min_x = (dike.x - dike.radius - dike.skew).max(chunk_min_x);
            let max_x = (dike.x + dike.radius + dike.skew).min(chunk_min_x + 15);
            let min_z = (dike.z - dike.radius - dike.skew).max(chunk_min_z);
            let max_z = (dike.z + dike.radius + dike.skew).min(chunk_min_z + 15);
            let min_y = (dike.y - dike.height).max(pipe.min_y);
            let max_y = (dike.y + dike.height).min(pipe.max_y);
            for vx in min_x..=max_x {
                for vz in min_z..=max_z {
                    for vy in min_y..=max_y {
                        if TRIM_TAIL && (vein_index, vx, vz, vy) > last_write {
                            break 'veins;
                        }
                        let selected = random.next_float() < pipe.density;
                        if selected && dike.contains(vx, vy, vz) {
                            let column_index =
                                ((vx - chunk_min_x) * 16 + vz - chunk_min_z) as usize;
                            let state_index =
                                column_index * column_height + (vy - min_source_y) as usize;
                            if source_states[state_index] == 0 {
                                let bottom_offset = vy - WORLDGEN_ORIGIN_Y;
                                let is_basal = (0..14).contains(&bottom_offset)
                                    && basal[column_index] & (1 << bottom_offset) != 0;
                                let eligible = !is_basal
                                    && source_samples[column_index].as_ref().is_some_and(
                                        |sample| {
                                            crate::ground::source_is_dike_replaceable_from_sample(
                                                world,
                                                vx,
                                                vy,
                                                vz,
                                                sample,
                                                stratified[column_index],
                                            )
                                        },
                                    );
                                source_states[state_index] = if eligible { 2 } else { 1 };
                            }
                            if source_states[state_index] == 2 {
                                // IWeighted.get(RandomSource) always calls
                                // nextDouble(), even for singleton output lists.
                                let _ = random.next_double();
                                if source_samples[column_index]
                                    .as_ref()
                                    .is_some_and(|sample| sample.y == vy)
                                {
                                    out.insert((vx, vy, vz), pipe_index);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Some(out)
}

/// Core's buildSurface HEAD injection replaces the bottom stone with hornfels.
/// Below -54, remaining blocks are bedrock, hornfels, fluids or carved space;
/// none are eligible dike hosts. The next four layers use a separate RNG stream.
fn basal_columns(cx: i32, cz: i32) -> [u16; 256] {
    let mut random = Xoroshiro::from_state(
        i64::from(cx).wrapping_mul(2_369_412_341) as u64,
        i64::from(cz).wrapping_mul(8_192_836_412_341) as u64,
    );
    let mut columns = [(1 << 10) - 1; 256];
    for column in &mut columns {
        for y in 10..14 {
            if random.bounded(4) < 14 - y {
                *column |= 1 << y;
            }
        }
    }
    columns
}

fn nearby_dikes(seed: i64, cx: i32, cz: i32, pipe: &Pipe) -> Vec<Dike> {
    let digest = md5::compute(pipe.random_name.as_bytes()).0;
    let name_seed = i64::from_be_bytes(digest[0..8].try_into().unwrap())
        ^ i64::from_be_bytes(digest[8..16].try_into().unwrap());
    let chunk_radius = 1 + (pipe.radius >> 4);
    let mut out = Vec::new();
    for source_cx in cx - chunk_radius..=cx + chunk_radius {
        for source_cz in cz - chunk_radius..=cz + chunk_radius {
            let mut random = Xoroshiro::from_state(
                (seed ^ i64::from(source_cx).wrapping_mul(61_728_364_132)) as u64,
                (name_seed ^ i64::from(source_cz).wrapping_mul(16_298_364_123)) as u64,
            );
            if random.bounded(pipe.rarity) != 0 {
                continue;
            }
            let angle = random.next_float() * std::f32::consts::PI * 2.0_f32;
            let x = source_cx.wrapping_mul(16).wrapping_add(random.bounded(16));
            let actual_y_range = pipe.max_y - pipe.min_y - 2 * pipe.height;
            let y = if actual_y_range > 0 {
                pipe.min_y + pipe.height + random.bounded(actual_y_range)
            } else {
                (pipe.min_y + pipe.max_y) / 2
            };
            let z = source_cz.wrapping_mul(16).wrapping_add(random.bounded(16));
            let sign = if pipe.sign < random.next_float() {
                1.0
            } else {
                -1.0
            };
            let skew = uniform_int(&mut random, pipe.min_skew, 1 + pipe.max_skew);
            let slant = uniform_int(&mut random, pipe.min_slant, 1 + pipe.max_slant);
            out.push(Dike {
                x,
                y,
                z,
                sign,
                skew_x: mc_cos(angle),
                skew_z: mc_sin(angle),
                skew,
                slant,
                radius: pipe.radius,
                height: pipe.height,
            });
        }
    }
    out
}

fn uniform_int(random: &mut Xoroshiro, min: i32, max: i32) -> i32 {
    min + random.bounded(max - min)
}

fn mc_sin(angle: f32) -> f32 {
    sin_table()[((angle * 10430.378_f32) as i32 & 65535) as usize]
}
fn mc_cos(angle: f32) -> f32 {
    sin_table()[((angle * 10430.378_f32 + 16384.0) as i32 & 65535) as usize]
}
fn sin_table() -> &'static [f32] {
    static SIN: OnceLock<Vec<f32>> = OnceLock::new();
    SIN.get_or_init(|| {
        (0..65536)
            .map(|i| (i as f64 * std::f64::consts::TAU / 65536.0).sin() as f32)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Settings;

    #[test]
    fn surface_prefix_matches_complete_dike_rng_replay() {
        let world = World::new(0, Settings::default());
        let mut hits = 0;
        for (cx, cz) in [(-7032, -972), (-7033, -972), (-7032, -971), (-7031, -972)] {
            let complete = generate_chunk::<false>(&world, cx, cz).unwrap();
            let trimmed = generate_chunk::<true>(&world, cx, cz).unwrap();
            assert_eq!(
                trimmed, complete,
                "surface writes differ after prefix optimization in {cx},{cz}"
            );
            hits += trimmed.len();
        }
        assert!(
            hits >= 8,
            "comparison must exercise actual exposed dike writes"
        );
    }

    #[test]
    fn basal_hornfels_exclusions_match_independent_saved_blocks() {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ground/basal-rock-world0.json"
        ))
        .unwrap();
        let mut excluded = 0;
        for chunk in reference["chunks"].as_array().unwrap() {
            let cx = chunk["chunk"][0].as_i64().unwrap() as i32;
            let cz = chunk["chunk"][1].as_i64().unwrap() as i32;
            let mask = basal_columns(cx, cz);
            for (i, value) in chunk["non_host_masks_xz"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                let observed = value.as_u64().unwrap() as u16;
                assert_eq!(
                    mask[i] & !observed,
                    0,
                    "basal exclusion is saved replaceable rock at chunk {cx},{cz}, column {i}"
                );
                excluded += mask[i].count_ones();
            }
        }
        assert!(excluded >= 512 * 11);
    }

    #[test]
    fn installed_dike_configs_have_source_rock_replacement_contract() {
        let pipes = pipes();
        assert_eq!(pipes.len(), 3);
        for (pipe, rock) in pipes.iter().zip(["granite", "diorite", "gabbro"]) {
            assert_eq!(pipe.rarity, 300);
            assert_eq!(pipe.density, 0.98);
            assert_eq!((pipe.radius, pipe.height), (18, 150));
            assert_eq!((pipe.min_y, pipe.max_y), (-64, 180));
            assert!(
                pipe.replacements
                    .values()
                    .any(|block| block.as_ref() == format!("tfc:rock/raw/{rock}"))
            );
            assert!(pipe.replacements.contains_key("tfc:rock/raw/chert"));
            assert!(pipe.replacements.contains_key("tfc:rock/gravel/chert"));
            assert!(pipe.replacements.contains_key("tfc:rock/hardened/chert"));
            assert_eq!(
                pipe.replacements["tfc:rock/raw/chert"].as_ref(),
                format!("tfc:rock/raw/{rock}")
            );
            assert_eq!(
                pipe.replacements["tfc:rock/gravel/chert"].as_ref(),
                format!("tfc:rock/raw/{rock}")
            );
            assert_eq!(
                pipe.replacements["tfc:rock/hardened/chert"].as_ref(),
                format!("tfc:rock/hardened/{rock}")
            );
        }
    }

    #[test]
    fn world0_exposed_diorite_dike_is_rendered_over_chert_host() {
        // Independent saved World 0 capture: eight adjacent raw diorite blocks
        // over the procedural chert host in a rocky plateau.
        let world = World::new(0, Settings::default());
        let saved_diorite_points = [
            (-112_512, 99, -15_549),
            (-112_512, 101, -15_548),
            (-112_512, 104, -15_547),
            (-112_512, 106, -15_546),
            (-112_511, 106, -15_546),
            (-112_512, 112, -15_539),
            (-112_511, 112, -15_539),
            (-112_512, 114, -15_538),
        ];
        for (x, y, z) in saved_diorite_points {
            assert_eq!(
                replace_surface_block(&world, x, y, z, "tfc:rock/gravel/chert").as_deref(),
                Some("tfc:rock/raw/diorite"),
                "saved dike mismatch at ({x},{y},{z})"
            );
        }
        assert_eq!(
            replace_surface_block(&world, -112_512, 99, -15_549, "tfc:grass/silt"),
            None,
            "pipe veins may replace only source-configured rock states"
        );
    }
}
