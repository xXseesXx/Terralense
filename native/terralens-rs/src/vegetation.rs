//! Seed-driven TFC/TFG surface decoration predictions for close-up map tiles.
//!
//! Worldgen feature RNG matches TFC's installed `Helpers.seedLargeFeatures` contract.
//! Terrain columns are explicitly
//! predictions from TerraLens' height sampler; they are not read from a generated chunk.

use crate::prism::{SurfaceChunk, SurfaceColumn};
use crate::random::Xoroshiro;
use crate::surface_water::SEA_SURFACE_BLOCK_Y;
use crate::world::World;
use lru::LruCache;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};

mod canopy;
mod climate;
pub mod crops;
pub mod templates;
mod trees;

pub use canopy::{Canopy, CanopyKind, canopy_at};

/// Generate one TFC forest-feature source chunk with an explicit sorted feature
/// index. This diagnostic hook supports comparing the canonical FeatureSorter
/// profile with saved chunks when the live pack has dynamic biome registration.
#[doc(hidden)]
#[cfg(feature = "diagnostics")]
pub fn tree_source_chunk_with_feature_index(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    index: u32,
    cancel: &impl Fn() -> bool,
) -> Vec<SurfacePlant> {
    let mut plants = trees::generate_with_index(world, chunk_x, chunk_z, index, cancel);
    if !cancel() {
        plants.extend(crops::generate_tree_patches(
            world, chunk_x, chunk_z, cancel,
        ));
    }
    plants
}

/// Generate only the TFC forest feature for a chosen FeatureSorter index and
/// optionally apply the approximate height-support check. Physical fluid gates
/// remain active. Diagnostics compare support-gate effects with saved chunks.
#[doc(hidden)]
#[cfg(feature = "diagnostics")]
pub fn forest_source_chunk_with_options(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    index: u32,
    full_height_support_proxy: bool,
    first_available_origin: bool,
    cancel: &impl Fn() -> bool,
) -> Vec<SurfacePlant> {
    trees::generate_with_index_options(
        world,
        chunk_x,
        chunk_z,
        index,
        full_height_support_proxy,
        first_available_origin,
        cancel,
    )
}

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
    pub(super) fn next_double(&mut self) -> f64 {
        let high = self.next(26) as i64;
        let low = self.next(27) as i64;
        ((high << 27) + low) as f64 / (1_u64 << 53) as f64
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
    TreeWood,
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

/// ForestFeature's top-level ground debris produced after a successful tree pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ForestLitterBlock {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: String,
}

/// Source-chunk groundcover and fallen-leaf placements from the same ForestFeature
/// RNG simulation used for the optional predicted canopy.
pub(crate) fn forest_litter_source_chunk(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Vec<ForestLitterBlock> {
    trees::generate_litter(world, chunk_x, chunk_z, cancel)
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
    let mut plants = canopy::source_trees(world, chunk_x, chunk_z, cancel)
        .as_ref()
        .clone();
    if !cancel() {
        let crop_plants = crops::generate_crop_only(
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
        );
        // Forest and palm trees were already composed by the shared canopy
        // source; detail-mode crop work only computes ordinary crop patches.
        plants.extend(crop_plants);
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

    (feature_index as i64)
        .wrapping_mul(random.next_long())
        .wrapping_mul(203_704_237)
        ^ (decoration_step as i64)
            .wrapping_mul(random.next_long())
            .wrapping_mul(758_031_792)
        ^ base
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
            let mut probe = world.sample(x, z, true);
            crate::ground_view::populate(world, x, z, &mut probe, false);
            let y = probe
                .ground_y
                .unwrap_or_else(|| probe.height.unwrap_or(63.0).round() as i32);
            let lava = probe.surface_lava;
            let magma = probe.surface_magma;
            let water = lava.is_none() && magma.is_none() && probe.is_water();
            let block = if lava.is_some() {
                "minecraft:lava".to_owned()
            } else if magma.is_some() {
                "tfc:rock/magma/basalt".to_owned()
            } else if water {
                "minecraft:water".to_owned()
            } else {
                probe
                    .ground_block
                    .clone()
                    .unwrap_or_else(|| "minecraft:stone".to_owned())
            };
            columns.push(SurfaceColumn {
                // Terrain height remains the predicted ground height. Fluid occupies
                // blocks below sea level, so its top block is Y=62, not Y=63.
                // Above-sea river/lake columns retain their sampled local height.
                y: lava
                    .or(magma)
                    .or(probe.surface_water_y)
                    .unwrap_or_else(|| if water { y.max(SEA_SURFACE_BLOCK_Y) } else { y }),
                block,
                tree: None,
                crop: None,
                color: surface_tint(&probe, lava.or(magma).unwrap_or(y)),
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
                    matches!(
                        plant.kind,
                        SurfacePlantKind::Tree | SurfacePlantKind::TreeWood
                    ),
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
        // Fluid surfaces and exposed magma cannot support these plant annotations.
        if crate::volcanic_lava::Surface::from_block(&column.block, column.y).is_some() {
            return;
        }
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

/// TFC merges feature holders from the source chunk and its eight neighboring
/// biome palettes before decorating. Feature presence in that union is the gate.
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

/// Exact BiomePlacement predicate for features whose placed-feature chain includes
/// `tfc:biome`. Unlike the chunk-union gate above, this checks the biome at the
/// candidate's own block position.
pub(super) fn feature_biome_at_position_allowed(
    world: &World,
    x: i32,
    z: i32,
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
                Some((
                    id.clone(),
                    value
                        .as_array()?
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect(),
                ))
            })
            .collect()
    });
    mapping.get(feature_id).is_some_and(|allowed| {
        let name = crate::biome::biome_name(world.region.biome(x, z));
        allowed.contains(name)
    })
}

pub(super) fn is_water_biome_id(id: &str) -> bool {
    id.ends_with("/river") || id.ends_with("lake") || id.ends_with("/ocean") || id.ends_with("sea")
}

pub(super) fn feature_index_in_step(id: &str, step: i32) -> Option<u32> {
    static INDICES: OnceLock<HashMap<i32, HashMap<String, u32>>> = OnceLock::new();
    let stages = INDICES.get_or_init(|| {
        let data: Value = serde_json::from_str(FEATURE_ORDER)
            .expect("bundled TFG FeatureSorter profile must be valid JSON");
        data["stages"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(step, features)| {
                let indices = features
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, id)| Some((id.as_str()?.to_owned(), index as u32)))
                    .collect();
                (step.parse().expect("numeric decoration step"), indices)
            })
            .collect()
    });
    stages.get(&step)?.get(id).copied()
}

fn canonical_feature_index(id: &str) -> Option<(i32, u32)> {
    feature_index_in_step(id, 9).map(|index| (9, index))
}

fn surface_tint(probe: &crate::model::Probe, _y: i32) -> [u8; 3] {
    if probe.surface_magma.is_some() {
        return crate::volcanic_lava::MAGMA_COLOR;
    }
    if probe.surface_lava.is_some() {
        return crate::volcanic_lava::COLOR;
    }
    if probe.ocean {
        return [40, 93, 119];
    }
    if probe.is_water() {
        return [73, 151, 164];
    }
    crate::ground_view::probe_color(probe).unwrap_or([128, 128, 128])
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
    fn plants_do_not_replace_a_flooded_surface_column() {
        let mut columns = vec![SurfaceColumn {
            y: SEA_SURFACE_BLOCK_Y,
            block: "minecraft:water".into(),
            tree: None,
            crop: None,
            color: [73, 151, 164],
        }];
        annotate_column(&mut columns, 0, 0, 0, 80, 0, "tfc:tree/oak", true);
        annotate_column(&mut columns, 0, 0, 0, 64, 0, "tfc:wild_crop/wheat", false);
        assert_eq!(columns[0].y, SEA_SURFACE_BLOCK_Y);
        assert_eq!(columns[0].block, "minecraft:water");
        assert!(columns[0].tree.is_none() && columns[0].crop.is_none());
    }

    #[test]
    fn exposed_magma_stays_solid_and_has_no_tree_or_crop_annotations() {
        for block in ["minecraft:magma_block", "tfc:rock/magma/basalt"] {
            let mut columns = vec![SurfaceColumn {
                y: 103,
                block: block.into(),
                tree: None,
                crop: None,
                color: crate::volcanic_lava::MAGMA_COLOR,
            }];
            annotate_column(&mut columns, 0, 0, 0, 120, 0, "tfc:tree/oak", true);
            annotate_column(&mut columns, 0, 0, 0, 120, 0, "tfc:wild_crop/wheat", false);
            assert_eq!(columns[0].block, block);
            assert_eq!(columns[0].y, 103);
            assert!(columns[0].tree.is_none() && columns[0].crop.is_none());
        }
    }

    #[test]
    fn caldera_detail_renders_lava_at_the_fluid_level_and_keeps_plants_off_it() {
        let world = World::new(0, crate::model::Settings::default());
        let (x, z) = (5511, -6368);
        let chunk = generate_surface(&world, x >> 4, z >> 4, &|| false);
        let index = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
        let column = &chunk.columns[index];
        assert_eq!(column.block, "minecraft:lava");
        assert_eq!(column.y, 92);
        assert_eq!(column.color, crate::volcanic_lava::COLOR);
        assert!(column.tree.is_none() && column.crop.is_none());
        let mut columns = vec![column.clone()];
        annotate_column(&mut columns, 0, 0, 0, 120, 0, "tfc:tree/oak", true);
        assert_eq!(columns[0].block, "minecraft:lava");
    }

    #[test]
    fn seed_zero_lowland_probe_renders_sea_level_water_without_raising_terrain() {
        let world = World::new(0, crate::model::Settings::default());
        // Search a small fixed seed-zero window for non-ocean land below sea level.
        // This exercises the independently sampled height/water mask rather than a
        // synthetic biome fixture. The chosen probe's source height remains intact.
        let mut flooded = None;
        'search: for z in 15_808..15_904 {
            for x in 15_808..15_904 {
                let probe = world.sample(x, z, true);
                if probe.surface_water && !probe.ocean && probe.height.is_some_and(|h| h < 63.0) {
                    flooded = Some((x, z, probe));
                    break 'search;
                }
            }
        }
        let (x, z, probe) = flooded.expect("seed-zero window should contain flooded lowland");
        let chunk = generate_surface(&world, x >> 4, z >> 4, &|| false);
        let column = &chunk.columns[(z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize];
        assert!(
            probe.height.unwrap() < 63.0,
            "raw terrain height was changed"
        );
        assert_eq!(column.y, SEA_SURFACE_BLOCK_Y);
        assert_eq!(column.block, "minecraft:water");
        assert!(column.tree.is_none() && column.crop.is_none());
    }

    #[test]
    fn detailed_floodplain_does_not_turn_the_observed_dry_bank_into_water() {
        let world = World::new(0, crate::model::Settings::default());
        let chunk = generate_surface(&world, 8833 >> 4, 11107 >> 4, &|| false);
        let bank =
            &chunk.columns[(11107_i32.rem_euclid(16) * 16 + 8833_i32.rem_euclid(16)) as usize];
        assert_ne!(bank.block, "minecraft:water");
        let wet_chunk = generate_surface(&world, 8827 >> 4, 11107 >> 4, &|| false);
        let bed =
            &wet_chunk.columns[(11107_i32.rem_euclid(16) * 16 + 8827_i32.rem_euclid(16)) as usize];
        assert_eq!(bed.block, "minecraft:water");
        assert_eq!(bed.y, SEA_SURFACE_BLOCK_Y);
        assert!(bed.tree.is_none() && bed.crop.is_none());
        assert_eq!(
            world.height(8833, 11107),
            Some(61.),
            "fixing surface water must not alter the structure-height model"
        );
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
