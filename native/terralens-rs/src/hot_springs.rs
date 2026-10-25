//! TFC's rare, shaped HotSpringFeature surface projection.
//!
//! The bundled profile follows the installed TFG Core 0.9.25 placed/configured
//! features (see `hot_springs/assets/features.json`). It projects spring-water
//! pools, dry basin floors, lava pools and hardened rims without map markers.
//! Only the base height and regional biome/rock samplers are queried here: this
//! module deliberately does not call `world.volcanic_surface`, ground overlays,
//! or vegetation, which keeps it safe under those samplers.

use crate::random::Xoroshiro;

mod fluid_veins;
use crate::{
    dimension::Dimension,
    vegetation::{self, FeatureRandom},
    world::World,
};
use lru::LruCache;
use serde::Deserialize;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};

const DATA: &str = include_str!("hot_springs/assets/features.json");
#[cfg(test)]
const FEATURE_ORDER: &str = include_str!("vegetation/assets/feature-order.json");
const STEP: i32 = 8;
const ORIGIN_Y: i32 = -64;
const SOURCE_CACHE_CAPACITY: usize = 512;
const TARGET_CACHE_CAPACITY: usize = 256;

type EmptyPositions = HashSet<(i32, i32, i32)>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Surface {
    pub y: i32,
    pub block: Arc<str>,
    pub floor: Option<Floor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Floor {
    pub y: i32,
    pub block: Arc<str>,
}

#[derive(Deserialize)]
struct Profile {
    features: Vec<FeatureProfile>,
}

#[derive(Deserialize)]
struct FeatureProfile {
    id: String,
    chance: i32,
    step: Option<i32>,
    #[serde(default)]
    fluid_vein: Option<String>,
    #[serde(default)]
    variants: Vec<VariantProfile>,
    radius: Option<i32>,
    fluid: Option<String>,
    #[serde(default)]
    allow_underwater: bool,
}

#[derive(Deserialize)]
struct VariantProfile {
    id: String,
    radius: i32,
    fluid: String,
}

fn profile() -> &'static Profile {
    static PROFILE: OnceLock<Profile> = OnceLock::new();
    PROFILE.get_or_init(|| serde_json::from_str(DATA).expect("bundled hot spring data is valid"))
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Key {
    seed: i64,
    settings: [u64; 6],
    x: i32,
    z: i32,
    before_forest: bool,
}

impl Key {
    fn new(world: &World, x: i32, z: i32, before_forest: bool) -> Self {
        let s = &world.region.settings;
        Self {
            seed: world.seed,
            settings: [
                s.temperature_scale.to_bits(),
                s.temperature_constant.to_bits(),
                s.rainfall_scale.to_bits(),
                s.rainfall_constant.to_bits(),
                s.continentalness.to_bits(),
                s.grass_density.to_bits(),
            ],
            x,
            z,
            before_forest,
        }
    }
}

#[derive(Clone)]
struct Patch {
    feature_step: i32,
    feature_index: u32,
    cells: HashMap<(i32, i32), Surface>,
}

#[derive(Default)]
struct Source {
    patches: Vec<Patch>,
}

enum TargetData {
    Empty,
    Cells(Vec<Option<Surface>>),
}

type Target = Arc<TargetData>;

thread_local! {
    static SOURCE_CACHE: RefCell<LruCache<Key, Arc<Source>>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(SOURCE_CACHE_CAPACITY).unwrap())
    );
    static TARGET_CACHE: RefCell<LruCache<Key, Target>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(TARGET_CACHE_CAPACITY).unwrap())
    );
}

/// Return the generated spring surface block for a column, if any.
pub(crate) fn sample(world: &World, x: i32, z: i32) -> Option<Surface> {
    sample_cancelable(world, x, z, &|| false).flatten()
}

/// Return the spring surface at forest decoration time (stage 8, feature index
/// 25). Later empty hot springs are excluded so trees see only earlier terrain
/// changes, matching the real feature order.
pub(crate) fn sample_before_forest(world: &World, x: i32, z: i32) -> Option<Surface> {
    sample_mode_cancelable(world, x, z, true, &|| false).flatten()
}

/// `None` means cancelled; `Some(None)` is a completed column without a spring.
pub(crate) fn sample_cancelable(
    world: &World,
    x: i32,
    z: i32,
    cancelled: &impl Fn() -> bool,
) -> Option<Option<Surface>> {
    sample_mode_cancelable(world, x, z, false, cancelled)
}

fn sample_mode_cancelable(
    world: &World,
    x: i32,
    z: i32,
    before_forest: bool,
    cancelled: &impl Fn() -> bool,
) -> Option<Option<Surface>> {
    if cancelled() {
        return None;
    }
    if world.dimension != Dimension::Overworld {
        return Some(None);
    }
    let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
    let key = Key::new(world, cx, cz, before_forest);
    let target = match TARGET_CACHE.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        Some(target) => target,
        None => {
            let target = Arc::new(build_target(world, cx, cz, before_forest, cancelled)?);
            if cancelled() {
                return None;
            }
            TARGET_CACHE.with(|cache| cache.borrow_mut().put(key, Arc::clone(&target)));
            target
        }
    };
    let TargetData::Cells(cells) = target.as_ref() else {
        return Some(None);
    };
    let index = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
    Some(cells[index].clone())
}

fn source(world: &World, cx: i32, cz: i32, cancelled: &impl Fn() -> bool) -> Option<Arc<Source>> {
    if cancelled() {
        return None;
    }
    let key = Key::new(world, cx, cz, false);
    if let Some(source) = SOURCE_CACHE.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        return Some(source);
    }
    let generated = generate_source(world, cx, cz, cancelled)?;
    let source = Arc::new(generated);
    if !cancelled() {
        SOURCE_CACHE.with(|cache| cache.borrow_mut().put(key, Arc::clone(&source)));
    }
    Some(source)
}

fn build_target(
    world: &World,
    cx: i32,
    cz: i32,
    before_forest: bool,
    cancelled: &impl Fn() -> bool,
) -> Option<TargetData> {
    let min_x = cx.saturating_mul(16);
    let min_z = cz.saturating_mul(16);
    let mut sources = Vec::with_capacity(9);
    // The largest spring radius is 14 and each origin lies within its source
    // chunk, so only the surrounding 3x3 placement chunks can touch this cell.
    for sz in cz.saturating_sub(1)..=cz.saturating_add(1) {
        for sx in cx.saturating_sub(1)..=cx.saturating_add(1) {
            if cancelled() {
                return None;
            }
            sources.push(source(world, sx, sz, cancelled)?);
        }
    }
    let has_relevant_patches = sources
        .iter()
        .flat_map(|source| &source.patches)
        .any(|patch| !before_forest || (patch.feature_step, patch.feature_index) < (8, 25));
    if !has_relevant_patches {
        return Some(TargetData::Empty);
    }
    let mut patches = sources
        .iter()
        .flat_map(|source| &source.patches)
        .filter(|patch| !before_forest || (patch.feature_step, patch.feature_index) < (8, 25))
        .collect::<Vec<_>>();
    patches.sort_by_key(|patch| (patch.feature_step, patch.feature_index));
    let mut cells = vec![None; 256];
    for patch in patches {
        for (&(x, z), surface) in &patch.cells {
            let dx = i64::from(x) - i64::from(min_x);
            let dz = i64::from(z) - i64::from(min_z);
            if !(0..16).contains(&dx) || !(0..16).contains(&dz) {
                continue;
            }
            let index = (dz * 16 + dx) as usize;
            // Feature order is authoritative: a later configured feature may
            // carve a lower basin or air-filled floor over an earlier pool.
            // Keep last-write-wins instead of preserving the highest prior
            // surface. Patches from neighboring origin chunks are kept in
            // stable source-chunk order above.
            cells[index] = Some(surface.clone());
        }
    }
    Some(TargetData::Cells(cells))
}

fn generate_source(
    world: &World,
    cx: i32,
    cz: i32,
    cancelled: &impl Fn() -> bool,
) -> Option<Source> {
    let mut output = Source::default();
    if cancelled() {
        return None;
    }
    // Check rarity first: nearly every source chunk returns through this cheap
    // random path without querying biome unions or terrain columns.
    for feature in &profile().features {
        if cancelled() {
            return None;
        }
        let step = feature.step.unwrap_or(STEP);
        let Some(index) = vegetation::feature_index_in_step(&feature.id, step) else {
            continue;
        };
        let mut random =
            FeatureRandom::new(vegetation::feature_seed(cx, cz, ORIGIN_Y, step, index));
        // Minecraft 1.20.1 RarityFilter is `nextFloat() < 1.0F / chance`.
        // This is intentionally not nextInt(chance): the two have different
        // RNG consumption and would shift InSquare placement and the shape.
        if random.next_float() >= 1.0_f32 / feature.chance as f32 {
            continue;
        }
        // The spring-water placed feature belongs to all 109 pinned Overworld
        // biome feature lists; its fluid-vein modifier is the actual gate. Skip
        // the expensive 144-point biome union and check vein assignment after
        // the cheap rarity test. No random draws are needed for a rejected
        // placement because this feature has its own local seed.
        if feature.fluid_vein.as_deref() == Some("tfg:spring_water") {
            if !fluid_veins::is_spring_water(world, cx, cz) {
                continue;
            }
        } else if !vegetation::feature_biome_allowed(world, cx, cz, &feature.id) {
            continue;
        }
        let origin_x = cx.wrapping_mul(16).wrapping_add(random.next_int_bound(16));
        let origin_z = cz.wrapping_mul(16).wrapping_add(random.next_int_bound(16));
        let (radius, fluid, allow_underwater) = if feature.variants.is_empty() {
            let radius = feature.radius?;
            (
                radius,
                feature.fluid.as_deref().unwrap_or("minecraft:air"),
                feature.allow_underwater,
            )
        } else {
            let variant_index = random.next_int_bound(feature.variants.len() as i32) as usize;
            let variant = &feature.variants[variant_index];
            // Preserve the simple-selector draw before the child feature's
            // HotSpringFeature fork. Active springs can select either air or
            // spring water; all variants use the source radius of 14.
            let _variant_id = &variant.id;
            (variant.radius, variant.fluid.as_str(), false)
        };
        let patch = generate_patch(
            world,
            origin_x,
            origin_z,
            radius,
            fluid,
            allow_underwater,
            step,
            index,
            &mut random,
            cancelled,
        )?;
        output.patches.push(patch);
    }
    Some(output)
}

fn generate_patch(
    world: &World,
    origin_x: i32,
    origin_z: i32,
    radius: i32,
    fluid: &str,
    allow_underwater: bool,
    feature_step: i32,
    feature_index: u32,
    random: &mut FeatureRandom,
    cancelled: &impl Fn() -> bool,
) -> Option<Patch> {
    let noise = Metaballs::simple(random, radius);
    // HotSpringFeature obtains one rock from the origin column and uses its
    // hardened/gravel states for every column in this spring.
    let rock_name = rock_name(world, origin_x, origin_z);
    // The lava config's replacement map is keyed by raw rock, while the
    // feature looks up the origin's hardened rock. Java eagerly evaluates the
    // getRandomValue default argument, so this one bounded draw is consumed
    // even though the lookup falls back to a random replacement entry.
    let magma_rock = (fluid == "minecraft:lava").then(|| {
        const CONTACT_MAP_ROCKS: [&str; 7] = [
            "rhyolite", "basalt", "andesite", "dacite", "granite", "diorite", "gabbro",
        ];
        CONTACT_MAP_ROCKS[random.next_int_bound(CONTACT_MAP_ROCKS.len() as i32) as usize].to_owned()
    });
    let mut touched_water = false;
    let mut heights = HashMap::new();
    let mut filled = HashMap::<(i32, i32), Surface>::new();
    let mut edge_holes = EmptyPositions::new();
    let wall: Arc<str> = format!("tfc:rock/hardened/{rock_name}").into();
    let air: Arc<str> = "minecraft:air".into();
    // HotSpringFeature treats positions already filled with air as solid for
    // its edge test. Without this set, every later cell in an empty spring
    // sees the previously carved basin as an open edge, distorting the rim
    // and, more importantly, consuming a different random stream.
    let mut filled_empty_positions = EmptyPositions::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if cancelled() {
                return None;
            }
            if !noise.inside(dx, dz) {
                continue;
            }
            let x = origin_x.wrapping_add(dx);
            let z = origin_z.wrapping_add(dz);
            let y = *heights
                .entry((x, z))
                .or_insert_with(|| world.height(x, z).map(|height| height.trunc() as i32))
                .as_ref()?;
            if !allow_underwater && y <= crate::surface_water::SEA_LEVEL {
                continue;
            }

            let mut edge = false;
            for (nx, nz) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let neighbor = (x.saturating_add(nx), z.saturating_add(nz));
                if let Some(surface) = filled.get(&neighbor)
                    && surface.y == y
                    && is_fluid(&surface.block)
                {
                    // The source checks BlockState.liquid() before testing the
                    // configured empty state. Any touching liquid, including
                    // an earlier lava pool cell, latches this spring into its
                    // fluid-contact replacement mode.
                    touched_water = true;
                    continue;
                }
                if fluid == "minecraft:air"
                    && filled_empty_positions.contains(&(neighbor.0, y, neighbor.1))
                {
                    continue;
                }
                if edge_holes.contains(&(neighbor.0, y, neighbor.1)) {
                    // Rare rim holes are air too, but are deliberately absent
                    // from filledEmptyPositions. They must affect later edge
                    // checks and their random draws, unlike interior carving.
                    edge = true;
                    break;
                }
                let neighbor_y = *heights
                    .entry(neighbor)
                    .or_insert_with(|| {
                        world
                            .height(neighbor.0, neighbor.1)
                            .map(|height| height.trunc() as i32)
                    })
                    .as_ref()?;
                if neighbor_y < y {
                    if allow_underwater && y <= crate::surface_water::SEA_SURFACE_BLOCK_Y {
                        // At or below the ocean surface the lower neighbor is
                        // water, which is liquid but not an empty block for an
                        // underwater spring.
                        touched_water = true;
                    } else {
                        edge = true;
                        break;
                    }
                }
            }

            // Match HotSpringFeature's per-cell sequence: calculate depth
            // after checking neighbors, then consume the optional edge-hole
            // draw. Edge detection is deterministic; the important RNG
            // difference comes from excluding already-filled air cells above,
            // which avoids extra edge-hole draws in dry basins.
            let distance_ratio = (2 * (dx * dx + dz * dz)) as f32 / (radius * radius) as f32;
            let center_factor = 1.0_f32 - 0.7_f32 * distance_ratio.clamp(0., 1.);
            let _surface_depth = ((8 + random.next_int_bound(3)) as f32 * center_factor) as i32;

            if edge {
                // A rare edge hole removes only the top block; its floor is
                // the preceding builder state, which can still be soil.
                let opens_hole = random.next_int_bound(12) == 0;
                if fluid == "minecraft:air" && opens_hole {
                    edge_holes.insert((x, y, z));
                }
                filled.insert(
                    (x, z),
                    Surface {
                        y: if opens_hole { y.saturating_sub(1) } else { y },
                        floor: None,
                        block: if opens_hole {
                            crate::ground::source_block_at(world, x, y - 1, z)
                                .unwrap_or_else(|| air.clone())
                        } else {
                            wall.clone()
                        },
                    },
                );
            } else if fluid == "minecraft:air" {
                // An empty HotSpringFeature carves its water-free center: the
                // exposed top after replacing block Y is the gravel at Y-1.
                filled.insert(
                    (x, z),
                    Surface {
                        y: y.saturating_sub(1),
                        floor: None,
                        block: Arc::from(format!("tfc:rock/gravel/{rock_name}")),
                    },
                );
                filled_empty_positions.insert((x, y, z));
            } else if touched_water && allow_underwater {
                // TFC chooses magma/hardened through the weighted contact map
                // once any liquid has been touched. Each filled cell consumes
                // its own weighted double draw.
                let magma = next_feature_double(random) < (1.0 / 3.0);
                let rock = magma_rock.as_deref().unwrap_or(rock_name.as_str());
                let material = if magma { "magma" } else { "hardened" };
                filled.insert(
                    (x, z),
                    Surface {
                        y,
                        floor: Some(Floor {
                            y: y.saturating_sub(1),
                            block: Arc::from(format!("tfc:rock/gravel/{rock_name}")),
                        }),
                        block: Arc::from(format!("tfc:rock/{material}/{rock}")),
                    },
                );
            } else {
                filled.insert(
                    (x, z),
                    Surface {
                        y,
                        floor: Some(Floor {
                            y: y.saturating_sub(1),
                            block: Arc::from(format!("tfc:rock/gravel/{rock_name}")),
                        }),
                        block: Arc::from(fluid),
                    },
                );
            }
        }
    }
    Some(Patch {
        feature_step,
        feature_index,
        cells: filled,
    })
}

fn rock_name(world: &World, x: i32, z: i32) -> String {
    world
        .rock_at(x, 0, z)
        .as_deref()
        .and_then(|rock| rock.rsplit('/').next())
        .filter(|rock| !rock.is_empty())
        .unwrap_or("granite")
        .to_owned()
}

/// The WorldgenRandom compatibility wrapper exposes RandomSource's legacy
/// nextDouble, assembled from two `next(bits)` calls.
fn next_feature_double(random: &mut FeatureRandom) -> f64 {
    let high = random.next(26) as u64;
    let low = random.next(27) as u64;
    ((high << 27) + low) as f64 * (1.0 / ((1_u64 << 53) as f64))
}

fn is_fluid(block: &str) -> bool {
    matches!(
        block,
        "minecraft:water" | "minecraft:lava" | "tfc:fluid/spring_water"
    )
}

struct Ball {
    x: f64,
    z: f64,
    weight: f64,
}

struct Metaballs(Vec<Ball>);

impl Metaballs {
    /// Source equivalent of `Metaballs2D.simple(Helpers.fork(random), size)`.
    fn simple(parent: &mut FeatureRandom, size: i32) -> Self {
        // Helpers.fork(RandomSource) reads two longs from the parent's
        // WorldgenRandom, then gives the raw state to XoroshiroRandomSource.
        let lo = parent.next_long() as u64;
        let hi = parent.next_long() as u64;
        let mut fork = Xoroshiro::from_state(lo, hi);
        let count = 3 + fork.next_int_bound(5);
        let min_size = f64::from(0.1_f32 * size as f32);
        let max_size = f64::from(0.3_f32 * size as f32);
        let ball_radius = f64::from(0.5_f32 * size as f32);
        let mut balls = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let x = (fork.next_double() - fork.next_double()) * ball_radius;
            let z = (fork.next_double() - fork.next_double()) * ball_radius;
            let weight = fork.next_double() * (max_size - min_size) + min_size;
            balls.push(Ball { x, z, weight });
        }
        Self(balls)
    }

    fn inside(&self, x: i32, z: i32) -> bool {
        let (x, z) = (f64::from(x), f64::from(z));
        let mut value = 0.0;
        for ball in &self.0 {
            let distance = (x - ball.x).powi(2) + (z - ball.z).powi(2);
            value += ball.weight * ball.weight / distance;
        }
        value > 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Settings;

    #[derive(Deserialize)]
    struct JavaReference {
        chunk: [i32; 2],
        step: i32,
        index: u32,
        feature_seed: i64,
        rarity_float: f64,
        origin: [i32; 2],
        selector: i32,
        balls: Vec<[f64; 3]>,
        inside_grid_xz_m14_to_p14: String,
        parent_next_int_1000: i32,
    }

    #[test]
    fn low_cost_rarity_gate_rejects_most_chunks_before_biome_sampling() {
        let empty = profile()
            .features
            .iter()
            .find(|feature| feature.id == "tfc:random_empty_hot_spring")
            .unwrap();
        let index = vegetation::feature_index_in_step(&empty.id, STEP).unwrap();
        let rejected = (0..256)
            .filter(|cx| {
                let mut random =
                    FeatureRandom::new(vegetation::feature_seed(*cx, 0, ORIGIN_Y, STEP, index));
                random.next_float() >= 1.0_f32 / empty.chance as f32
            })
            .count();
        assert!(rejected > 240, "rarity is the cheap dominant gate");
    }

    #[test]
    fn spring_profile_keeps_the_installed_empty_selection_and_pool_sizes() {
        let empty = &profile().features[0];
        assert_eq!(empty.id, "tfc:random_empty_hot_spring");
        assert_eq!(empty.chance, 70);
        assert_eq!(empty.variants.len(), 4);
        assert!(
            empty
                .variants
                .iter()
                .all(|v| v.radius == 14 && v.fluid == "minecraft:air")
        );
        let lava = profile()
            .features
            .iter()
            .find(|feature| feature.id == "tfc:lava_hot_spring")
            .unwrap();
        assert_eq!(lava.id, "tfc:lava_hot_spring");
        assert_eq!(lava.chance, 20);
        assert_eq!(lava.radius, Some(10));
        assert_eq!(lava.fluid.as_deref(), Some("minecraft:lava"));

        let active = profile()
            .features
            .iter()
            .find(|feature| feature.id == "tfg:earth/fluid_vein/spring_water")
            .unwrap();
        assert_eq!(active.step, Some(3));
        assert_eq!(active.chance, 30);
        assert_eq!(active.fluid_vein.as_deref(), Some("tfg:spring_water"));
        assert_eq!(active.variants.len(), 16);
        assert_eq!(
            active
                .variants
                .iter()
                .filter(|variant| variant.fluid == "tfc:fluid/spring_water")
                .count(),
            12
        );
        assert_eq!(
            vegetation::feature_index_in_step(&active.id, active.step.unwrap()),
            Some(2)
        );
        let order: serde_json::Value = serde_json::from_str(FEATURE_ORDER).unwrap();
        assert_eq!(
            order["featureBiomes"][&active.id].as_array().unwrap().len(),
            109,
            "the fluid-vein wrapper is a member of all pinned Earth biome lists"
        );
    }

    #[test]
    fn world0_spring_footprint_uses_reference_calibrated_feature_order() {
        let world = World::new(0, Settings::default());
        let (cx, cz, step) = (-19_954, 1_006, 8);
        let index = vegetation::feature_index_in_step("tfc:random_empty_hot_spring", step).unwrap();
        assert_eq!(index, 30);
        assert!(vegetation::feature_biome_allowed(
            &world,
            cx,
            cz,
            "tfc:random_empty_hot_spring"
        ));
        let mut random =
            FeatureRandom::new(vegetation::feature_seed(cx, cz, ORIGIN_Y, step, index));
        let rarity = random.next_float();
        assert!(rarity < 1.0 / 70.0);
        let origin_x = cx.wrapping_mul(16).wrapping_add(random.next_int_bound(16));
        let origin_z = cz.wrapping_mul(16).wrapping_add(random.next_int_bound(16));
        assert_eq!((origin_x, origin_z), (-319_250, 16_107));
        let selector = random.next_int_bound(4);
        assert_eq!(selector, 0);
        let patch = generate_patch(
            &world,
            origin_x,
            origin_z,
            14,
            "minecraft:air",
            false,
            step,
            index,
            &mut random,
            &|| false,
        )
        .expect("non-cancelled candidate patch");
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/ground/dike-user-site-seed0.json"
        ))
        .unwrap();
        let mut actual_rock = 0;
        let mut matched_material = 0;
        let mut matched_height = 0;
        let mut observed_rock = HashMap::<String, usize>::new();
        for chunk in reference["chunks"].as_array().unwrap() {
            for column in chunk["columns"].as_array().unwrap() {
                let x = column["world_x"].as_i64().unwrap() as i32;
                let z = column["world_z"].as_i64().unwrap() as i32;
                let block = column["ground_block"].as_str().unwrap();
                if !matches!(
                    block,
                    "tfc:rock/gravel/quartzite" | "tfc:rock/hardened/quartzite"
                ) {
                    continue;
                }
                actual_rock += 1;
                *observed_rock.entry(block.to_owned()).or_default() += 1;
                let Some(predicted) = patch.cells.get(&(x, z)) else {
                    continue;
                };
                if predicted.block.as_ref() == block {
                    matched_material += 1;
                    let saved_y = column["ground_y"].as_i64().unwrap() as i32;
                    if (predicted.y - saved_y).abs() <= 1 {
                        matched_height += 1;
                    }
                }
            }
        }
        println!(
            "index30 dry patch origin={origin_x},{origin_z} footprint={} actual quartzite columns={actual_rock} observed={observed_rock:?} material_matches={matched_material} height_matches={matched_height}",
            patch.cells.len()
        );
        assert!(
            actual_rock == 163,
            "independent FULL fixture contains a substantial basin"
        );
        assert!(
            matched_material >= 159,
            "saved exposed quartzite materials must retain the measured 159/163 match"
        );
    }

    #[test]
    fn java_hot_spring_reference_matches_active_placement_and_metaballs_rng() {
        let reference: JavaReference =
            serde_json::from_str(include_str!("hot_springs/assets/java-reference.json")).unwrap();
        let seed = vegetation::feature_seed(
            reference.chunk[0],
            reference.chunk[1],
            ORIGIN_Y,
            reference.step,
            reference.index,
        );
        assert_eq!(seed, reference.feature_seed);

        let mut random = FeatureRandom::new(seed);
        let rarity = random.next_float();
        assert_eq!(rarity.to_bits(), (reference.rarity_float as f32).to_bits());
        assert!(rarity < 1.0 / 30.0);
        let origin = [
            reference.chunk[0] * 16 + random.next_int_bound(16),
            reference.chunk[1] * 16 + random.next_int_bound(16),
        ];
        assert_eq!(origin, reference.origin);
        assert_eq!(random.next_int_bound(16), reference.selector);
        assert!((4..16).contains(&reference.selector));

        let shape = Metaballs::simple(&mut random, 14);
        assert_eq!(shape.0.len(), reference.balls.len());
        for (ball, expected) in shape.0.iter().zip(reference.balls) {
            assert_eq!(ball.x.to_bits(), expected[0].to_bits());
            assert_eq!(ball.z.to_bits(), expected[1].to_bits());
            assert_eq!(ball.weight.to_bits(), expected[2].to_bits());
        }
        let mut grid = String::with_capacity(29 * 29);
        for z in -14..=14 {
            for x in -14..=14 {
                grid.push(if shape.inside(x, z) { '1' } else { '0' });
            }
        }
        assert_eq!(grid, reference.inside_grid_xz_m14_to_p14);
        assert_eq!(random.next_int_bound(1000), reference.parent_next_int_1000);
    }

    #[test]
    fn active_spring_is_present_in_full_and_pre_forest_projections() {
        let world = World::new(0, Settings::default());
        let full = sample(&world, 14_386, 13_643).expect("World0 active spring witness");
        let before_forest =
            sample_before_forest(&world, 14_386, 13_643).expect("pre-forest active spring witness");
        for spring in [full, before_forest] {
            assert_eq!(spring.y, 96);
            assert_eq!(spring.block.as_ref(), "tfc:fluid/spring_water");
            let floor = spring.floor.expect("spring water has gravel beneath it");
            assert_eq!(floor.y, 95);
            assert_eq!(floor.block.as_ref(), "tfc:rock/gravel/granite");
        }
    }
}
