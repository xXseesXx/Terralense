//! Seed-driven TFC/TFG surface decoration predictions for close-up map tiles.
//!
//! Worldgen feature RNG matches TFC's installed `Helpers.seedLargeFeatures` contract.
//! Terrain columns are explicitly
//! predictions from TerraLens' height sampler; they are not read from a generated chunk.

use crate::prism::{SurfaceChunk, SurfaceColumn};
use crate::random::Xoroshiro;
use crate::world::World;
use lru::LruCache;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};

mod climate;
pub mod crops;
pub mod templates;
mod trees;

const FEATURE_ORDER: &str = include_str!("vegetation/assets/feature-order.json");
const OVERWORLD_MIN_Y: i32 = -64;

/// Minecraft's `WorldgenRandom` wraps the random source but inherits the legacy
/// `RandomSource` convenience methods. In particular `nextLong` is two calls to
/// `next(32)`, and bounded ints use Java's rejection algorithm rather than the
/// Xoroshiro source's Lemire helper.
#[derive(Clone, Debug)]
pub(super) struct FeatureRandom(Xoroshiro);

impl FeatureRandom {
    pub(super) fn new(seed: i64) -> Self {
        Self(Xoroshiro::new(seed))
    }
    pub(super) fn next(&mut self, bits: u32) -> i32 {
        self.0.next(bits)
    }
    pub(super) fn next_long(&mut self) -> i64 {
        ((self.next(32) as i64) << 32).wrapping_add(self.next(32) as i64)
    }
    pub(super) fn next_float(&mut self) -> f32 {
        (self.next(24) as f32) / (1u32 << 24) as f32
    }
    pub(super) fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if bound & -bound == bound {
            return (((bound as i64) * (self.next(31) as i64)) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfacePlantKind {
    Tree,
    Bush,
    Crop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfacePlant {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub kind: SurfacePlantKind,
    /// Stable namespaced feature/species ID, for example `tfc:tree/oak_entry`.
    pub variant: String,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct PlantCacheKey {
    seed: i64,
    settings: [u64; 6],
    chunk_x: i32,
    chunk_z: i32,
}

thread_local! {
    /// Vegetation requests routinely overlap at tile boundaries. Cache the source
    /// decoration chunk rather than regenerating its features per target chunk.
    static PLANT_CACHE: RefCell<LruCache<PlantCacheKey, Arc<Vec<SurfacePlant>>>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(4096).expect("non-zero plant cache capacity"))
    );
    static BIOME_UNION_CACHE: RefCell<LruCache<PlantCacheKey, Arc<HashSet<String>>>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(4096).expect("non-zero biome cache capacity"))
    );
}

fn source_plants(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Arc<Vec<SurfacePlant>> {
    let key = plant_cache_key(world, chunk_x, chunk_z);
    if let Some(cached) = PLANT_CACHE.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        return cached;
    }
    let mut plants = trees::generate(world, chunk_x, chunk_z, cancel);
    if !cancel() {
        plants.extend(crops::generate(
            world,
            chunk_x,
            chunk_z,
            |id| {
                if !feature_biome_allowed(world, chunk_x, chunk_z, id) {
                    return None;
                }
                let (step, index) = canonical_feature_index(id)?;
                Some(feature_seed(chunk_x, chunk_z, OVERWORLD_MIN_Y, step, index))
            },
            cancel,
        ));
    }
    let plants = Arc::new(plants);
    if !cancel() {
        PLANT_CACHE.with(|cache| {
            cache.borrow_mut().put(key, Arc::clone(&plants));
        });
    }
    plants
}

fn plant_cache_key(world: &World, chunk_x: i32, chunk_z: i32) -> PlantCacheKey {
    let settings = &world.region.settings;
    PlantCacheKey {
        seed: world.seed,
        settings: [
            settings.temperature_scale.to_bits(),
            settings.temperature_constant.to_bits(),
            settings.rainfall_scale.to_bits(),
            settings.rainfall_constant.to_bits(),
            settings.continentalness.to_bits(),
            settings.grass_density.to_bits(),
        ],
        chunk_x,
        chunk_z,
    }
}

/// Compute the exact TFC/TFG block-position hash used before chunk decoration.
pub fn decoration_seed(chunk_x: i32, chunk_z: i32, origin_y: i32) -> i64 {
    const PRIME_X: i64 = 501_125_321;
    const PRIME_Y: i64 = 1_136_930_381;
    const HASH_MUL: i64 = 0x27d4_eb2d;
    let block_x = (chunk_x as i64).wrapping_mul(16);
    let block_z = (chunk_z as i64).wrapping_mul(16);
    let hash = 128_739_412_341i64
        ^ block_x.wrapping_mul(PRIME_X)
        ^ (origin_y as i64).wrapping_mul(PRIME_Y)
        ^ block_z;
    (hash.wrapping_mul(HASH_MUL) as i32) as i64
}

/// Compute the per-feature seed used by TFC's `Helpers.seedLargeFeatures`.
pub fn feature_seed(
    chunk_x: i32,
    chunk_z: i32,
    origin_y: i32,
    decoration_step: i32,
    feature_index: u32,
) -> i64 {
    let base = decoration_seed(chunk_x, chunk_z, origin_y);
    let mut random = FeatureRandom::new(base);
    let seed = (feature_index as i64)
        .wrapping_mul(random.next_long())
        .wrapping_mul(203_704_237)
        ^ (decoration_step as i64)
            .wrapping_mul(random.next_long())
            .wrapping_mul(758_031_792)
        ^ base;
    seed
}

#[cfg(test)]
mod feature_seed_tests {
    use super::*;

    #[test]
    fn tfg_feature_seed_matches_java_worldgenrandom_wrapper_fixture() {
        // Captured with MC 1.20.1 WorldgenRandom(XoroshiroRandomSource(0)) and
        // Helpers.seedLargeFeatures for chunk (-100, 26), stage 9, index 45.
        assert_eq!(
            feature_seed(-100, 26, -64, 9, 45),
            -3_483_402_354_714_499_433
        );
    }
}

/// Build a detailed predicted surface tile. The 256 columns are z-major and each carries
/// terrain tint plus deterministic vegetation annotations for the renderer and hover UI.
pub fn generate_surface(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> SurfaceChunk {
    let mut columns = Vec::with_capacity(256);
    let origin_x = chunk_x.wrapping_mul(16);
    let origin_z = chunk_z.wrapping_mul(16);
    for local_z in 0..16 {
        for local_x in 0..16 {
            if cancel() {
                return SurfaceChunk {
                    chunk_x,
                    chunk_z,
                    columns: Vec::new(),
                };
            }
            let x = origin_x.wrapping_add(local_x);
            let z = origin_z.wrapping_add(local_z);
            let probe = world.sample(x, z, true);
            let y = probe.height.unwrap_or(63.0).round() as i32;
            let water = probe.ocean || is_water_biome_id(&probe.biome);
            let block = if water {
                "minecraft:water".to_owned()
            } else {
                let biome = probe.biome.to_ascii_lowercase();
                let category = if biome.contains("desert")
                    || biome.contains("dune")
                    || biome.contains("badlands")
                    || biome.contains("sand")
                {
                    "sand"
                } else if biome.contains("glacier")
                    || biome.contains("tundra")
                    || biome.contains("snow")
                    || probe.climate.temperature < -8.0
                {
                    "snow"
                } else if probe.climate.rainfall < 120.0 && probe.climate.temperature > 12.0 {
                    "dry_grass"
                } else {
                    "grass"
                };
                format!("terralens:predicted_surface/{category}")
            };
            columns.push(SurfaceColumn {
                y: if water { y.max(63) } else { y },
                block,
                tree: None,
                crop: None,
                color: surface_tint(&probe, y),
            });
        }
    }
    if columns.len() != 256 || cancel() {
        return SurfaceChunk {
            chunk_x,
            chunk_z,
            columns: Vec::new(),
        };
    }
    // Tree canopies reach at most ten blocks from a feature origin. The largest
    // crop spread is twenty blocks, plus a 0..15 in-square origin; source chunks
    // two cells away can therefore overlap this one, while three-away cannot.
    for source_z in chunk_z.saturating_sub(2)..=chunk_z.saturating_add(2) {
        for source_x in chunk_x.saturating_sub(2)..=chunk_x.saturating_add(2) {
            if cancel() {
                return SurfaceChunk {
                    chunk_x,
                    chunk_z,
                    columns: Vec::new(),
                };
            }
            for plant in source_plants(world, source_x, source_z, cancel).iter() {
                let dx = plant.x - chunk_x.wrapping_mul(16);
                let dz = plant.z - chunk_z.wrapping_mul(16);
                if !(0..16).contains(&dx) || !(0..16).contains(&dz) {
                    continue;
                }
                annotate_column(
                    &mut columns,
                    chunk_x,
                    chunk_z,
                    plant.x,
                    plant.y,
                    plant.z,
                    &plant.variant,
                    matches!(plant.kind, SurfacePlantKind::Tree),
                );
            }
        }
    }
    SurfaceChunk {
        chunk_x,
        chunk_z,
        columns,
    }
}

fn annotate_column(
    columns: &mut [SurfaceColumn],
    chunk_x: i32,
    chunk_z: i32,
    x: i32,
    y: i32,
    z: i32,
    variant: &str,
    tree: bool,
) {
    let dx = x - chunk_x.wrapping_mul(16);
    let dz = z - chunk_z.wrapping_mul(16);
    if (0..16).contains(&dx) && (0..16).contains(&dz) {
        let column = &mut columns[dz as usize * 16 + dx as usize];
        if tree {
            if y >= column.y {
                column.tree = Some(variant.to_owned());
                column.crop = None;
                column.y = y;
                column.block = "terralens:predicted_surface/canopy".to_owned();
            }
        } else {
            if y >= column.y {
                column.crop = Some(variant.to_owned());
                column.tree = None;
                column.y = y;
                column.block = variant.to_owned();
            }
        }
    }
}

/// The TFC decoration pass includes a feature when its holder is present in any
/// biome palette across the source chunk and its eight neighbors.
pub(super) fn feature_biome_allowed(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    feature_id: &str,
) -> bool {
    static FEATURE_BIOMES: OnceLock<HashMap<String, HashSet<String>>> = OnceLock::new();
    let mapping = FEATURE_BIOMES.get_or_init(|| {
        let data: Value = serde_json::from_str(FEATURE_ORDER)
            .expect("bundled TFG feature biome profile must be valid JSON");
        data["featureBiomes"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(id, value)| {
                let biomes = value
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                Some((id.clone(), biomes))
            })
            .collect()
    });
    let Some(allowed) = mapping.get(feature_id) else {
        return false;
    };
    let union = BIOME_UNION_CACHE.with(|cache| {
        let key = plant_cache_key(world, chunk_x, chunk_z);
        if let Some(cached) = cache.borrow_mut().get(&key).cloned() {
            return cached;
        }
        let mut biomes = HashSet::new();
        for dz in -1..=1 {
            for dx in -1..=1 {
                let source_x = chunk_x.saturating_add(dx).wrapping_mul(16);
                let source_z = chunk_z.saturating_add(dz).wrapping_mul(16);
                for qz in [2, 6, 10, 14] {
                    for qx in [2, 6, 10, 14] {
                        biomes.insert(
                            crate::biome::biome_name(
                                world
                                    .region
                                    .biome(source_x.wrapping_add(qx), source_z.wrapping_add(qz)),
                            )
                            .to_owned(),
                        );
                    }
                }
            }
        }
        let biomes = Arc::new(biomes);
        cache.borrow_mut().put(key, Arc::clone(&biomes));
        biomes
    });
    union.iter().any(|biome| allowed.contains(biome))
}

pub(super) fn is_water_biome_id(id: &str) -> bool {
    id.ends_with("/river") || id.ends_with("lake") || id.ends_with("/ocean") || id.ends_with("sea")
}

fn canonical_feature_index(id: &str) -> Option<(i32, u32)> {
    static INDICES: OnceLock<HashMap<String, (i32, u32)>> = OnceLock::new();
    INDICES
        .get_or_init(|| {
            let data: Value = serde_json::from_str(FEATURE_ORDER)
                .expect("bundled TFG FeatureSorter profile must be valid JSON");
            data["activeCropFeatures"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    Some((
                        entry["id"].as_str()?.to_owned(),
                        (
                            entry["step"].as_i64()? as i32,
                            entry["index"].as_u64()? as u32,
                        ),
                    ))
                })
                .collect()
        })
        .get(id)
        .copied()
}

fn surface_tint(probe: &crate::model::Probe, y: i32) -> [u8; 3] {
    if probe.ocean {
        return [40, 93, 119];
    }
    if is_water_biome_id(&probe.biome) {
        return [73, 151, 164];
    }
    let biome = probe.biome.to_ascii_lowercase();
    if ["desert", "dune", "badlands", "sand"]
        .iter()
        .any(|name| biome.contains(name))
    {
        return [188, 164, 117];
    }
    if ["glacier", "tundra", "snow"]
        .iter()
        .any(|name| biome.contains(name))
        || probe.climate.temperature < -8.
    {
        return [214, 225, 224];
    }
    if probe.climate.rainfall < 120. && probe.climate.temperature > 12. {
        return [166, 164, 102];
    }
    let t = ((y - 60) as f64 / 200.0).clamp(0.0, 1.0);
    let stops = [
        [117.0, 160.0, 120.0],
        [159.0, 173.0, 133.0],
        [174.0, 157.0, 126.0],
        [157.0, 144.0, 136.0],
        [220.0, 226.0, 222.0],
    ];
    let scaled = t * (stops.len() - 1) as f64;
    let low = (scaled.floor() as usize).min(stops.len() - 2);
    let high = low + 1;
    let mix = scaled - low as f64;
    std::array::from_fn(|i| (stops[low][i] + (stops[high][i] - stops[low][i]) * mix).round() as u8)
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn highest_predicted_plant_owns_surface_label_and_block() {
        let mut columns = vec![SurfaceColumn {
            y: 64,
            block: "terralens:predicted_surface/grass".into(),
            tree: None,
            crop: None,
            color: [0; 3],
        }];
        annotate_column(&mut columns, 0, 0, 0, 65, 0, "tfc:wild_crop/wheat", false);
        assert_eq!(columns[0].crop.as_deref(), Some("tfc:wild_crop/wheat"));
        assert_eq!(columns[0].block, "tfc:wild_crop/wheat");
        assert_eq!(columns[0].y, 65);

        annotate_column(&mut columns, 0, 0, 0, 80, 0, "tfc:tree/oak", true);
        assert_eq!(columns[0].tree.as_deref(), Some("tfc:tree/oak"));
        assert_eq!(columns[0].crop, None);
        assert_eq!(columns[0].block, "terralens:predicted_surface/canopy");
        assert_eq!(columns[0].y, 80);

        annotate_column(&mut columns, 0, 0, 0, 65, 0, "tfc:wild_crop/wheat", false);
        assert_eq!(columns[0].block, "terralens:predicted_surface/canopy");
        annotate_column(&mut columns, 0, 0, 0, 90, 0, "tfc:wild_crop/wheat", false);
        assert_eq!(columns[0].tree, None);
        assert_eq!(columns[0].crop.as_deref(), Some("tfc:wild_crop/wheat"));
    }

    #[test]
    fn generated_forest_surface_contains_canopies_and_real_crop_variants() {
        let world = World::new(7_787_531_943_268_589_495, crate::model::Settings::default());
        let mut canopies = 0;
        let mut crops = 0;
        for cz in -131..-127 {
            for cx in -143..-139 {
                let chunk = generate_surface(&world, cx, cz, &|| false);
                assert_eq!(chunk.columns.len(), 256);
                for column in chunk.columns {
                    if column.tree.is_some() {
                        assert_eq!(column.block, "terralens:predicted_surface/canopy");
                        assert!(column.crop.is_none());
                        canopies += 1;
                    } else if let Some(crop) = column.crop {
                        assert!(crop.starts_with("tfc:wild_crop/") || crop.starts_with("tfg:"));
                        assert_eq!(column.block, crop);
                        crops += 1;
                    }
                }
            }
        }
        assert!(
            canopies > 256,
            "seed detail did not generate the known forest"
        );
        assert!(crops > 0, "seed detail lost the known crop patches");
    }
}
