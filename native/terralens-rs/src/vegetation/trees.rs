//! TFC ForestFeature seed-driven tree proposals and template silhouettes.
//!
//! Selection, counts, biome-climate filtering, weirdness and configured-tree
//! variant draws follow the installed TFG/TFC data and source. The terrain
//! survival test still uses TerraLens' predicted height/climate surface.

use super::{
    FeatureRandom, SurfacePlant, SurfacePlantKind,
    climate::{ForestNoise, adjusted_average_temp_by_elevation},
    feature_biome_allowed,
    templates::{self, TemplateBlockKind, TreeVariant},
};
use crate::world::World;
use lru::LruCache;
use serde::Deserialize;
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};

const FOREST_CONFIG: &str = include_str!("assets/forest_feature.json");
const FEATURE_ORDER: &str = include_str!("assets/feature-order.json");
const TREE_GROUND: &str = include_str!("assets/tree_ground.json");
const FOREST_LITTER: &str = include_str!("assets/forest_litter.json");
const ORIGIN_Y: i32 = -64;

thread_local! {
    static FOREST_NOISE_CACHE: RefCell<LruCache<i64, Arc<ForestNoise>>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(8).expect("non-zero forest noise capacity"))
    );
}

fn forest_noise(seed: i64) -> Arc<ForestNoise> {
    FOREST_NOISE_CACHE.with(|cache| {
        if let Some(value) = cache.borrow_mut().get(&seed).cloned() {
            return value;
        }
        let value = Arc::new(ForestNoise::new(seed));
        cache.borrow_mut().put(seed, Arc::clone(&value));
        value
    })
}

#[derive(Clone, Copy)]
struct CountProvider {
    min: i32,
    max: i32,
}
impl CountProvider {
    fn sample(self, random: &mut FeatureRandom) -> i32 {
        if self.min == self.max {
            self.min
        } else {
            self.min + random.next_int_bound(self.max - self.min + 1)
        }
    }
}

#[derive(Clone, Copy)]
struct ForestKindConfig {
    chance: f32,
    trees: CountProvider,
    bushes: Option<CountProvider>,
    old_growth: bool,
    spoilers: bool,
    groundcover: CountProvider,
    leaf_piles: CountProvider,
}

#[derive(Clone, Deserialize)]
struct WeightedLitterBlock {
    block: String,
    #[serde(default = "unit_weight")]
    weight: f64,
}

fn unit_weight() -> f64 {
    1.0
}

#[derive(Clone, Deserialize)]
struct ForestLitterEntry {
    #[serde(default)]
    groundcover: Vec<WeightedLitterBlock>,
    fallen_leaves: Option<String>,
    bush_log: Option<String>,
    bush_leaves: Option<String>,
}

fn forest_litter_entries() -> &'static HashMap<String, ForestLitterEntry> {
    static ENTRIES: OnceLock<HashMap<String, ForestLitterEntry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        serde_json::from_str::<Value>(FOREST_LITTER)
            .expect("forest litter source data is valid")
            .get("entries")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .expect("forest litter entry map is valid")
    })
}

fn use_weirdness() -> bool {
    static VALUE: OnceLock<bool> = OnceLock::new();
    *VALUE.get_or_init(|| {
        let root: Value = serde_json::from_str(FOREST_CONFIG).expect("embedded forest config JSON");
        root["config"]
            .get("use_weirdness")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    })
}

fn provider(value: &Value, default: i32) -> CountProvider {
    if let Some(n) = value.as_i64() {
        return CountProvider {
            min: n as i32,
            max: n as i32,
        };
    }
    if let Some(uniform) = value.get("value") {
        let min = uniform
            .get("min_inclusive")
            .and_then(Value::as_i64)
            .unwrap_or(default as i64) as i32;
        let max = uniform
            .get("max_inclusive")
            .and_then(Value::as_i64)
            .unwrap_or(min as i64) as i32;
        return CountProvider { min, max };
    }
    CountProvider {
        min: default,
        max: default,
    }
}

fn configs() -> &'static [ForestKindConfig; 5] {
    static CONFIG: OnceLock<[ForestKindConfig; 5]> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let root: Value = serde_json::from_str(FOREST_CONFIG).expect("embedded forest config JSON");
        let types = &root["config"]["types"];
        let parse = |name: &str| {
            let value = &types[name];
            ForestKindConfig {
                chance: value
                    .get("per_chunk_chance")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0) as f32,
                trees: provider(&value["tree_count"], 0),
                bushes: value.get("bush_count").map(|n| provider(n, 0)),
                old_growth: value
                    .get("allows_old_growth")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                spoilers: value
                    .get("has_spoiler_old_growth")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                groundcover: provider(&value["groundcover_count"], 0),
                leaf_piles: provider(&value["leaf_pile_count"], 0),
            }
        };
        [
            parse("none"),
            parse("sparse"),
            parse("edge"),
            parse("normal"),
            parse("old_growth"),
        ]
    })
}

fn forest_index() -> Option<u32> {
    static INDEX: OnceLock<Option<u32>> = OnceLock::new();
    *INDEX.get_or_init(|| {
        let profile: Value = serde_json::from_str(FEATURE_ORDER).ok()?;
        let entry = &profile["forestFeature"];
        (entry["id"].as_str()? == "tfg:earth/forest")
            .then(|| entry["index"].as_u64().map(|n| n as u32))
            .flatten()
    })
}

/// Generate canopy/trunk coverage for trees sourced in one decoration chunk.
pub(super) fn generate(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Vec<SurfacePlant> {
    let Some(index) = forest_index() else {
        return Vec::new();
    };
    generate_with_index_options(world, chunk_x, chunk_z, index, false, true, cancel)
}

/// Continue the exact ForestFeature RNG stream to the groundcover and leaf-pile
/// phases. This path is requested only by the optional litter overlay; tree-only
/// callers keep using `generate` and do not pay for debris simulation.
pub(super) fn generate_litter(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Vec<super::ForestLitterBlock> {
    let Some(index) = forest_index() else {
        return Vec::new();
    };
    generate_inner(world, chunk_x, chunk_z, index, false, true, true, cancel).1
}

/// Same feature simulation with an explicit step-8 sorted index. Kept available
/// to the saved-world accuracy audit because the merged runtime biome sorter can
/// assign a different index than the canonical bundled profile.
#[cfg(feature = "diagnostics")]
pub(super) fn generate_with_index(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    index: u32,
    cancel: &impl Fn() -> bool,
) -> Vec<SurfacePlant> {
    generate_with_index_options(world, chunk_x, chunk_z, index, false, true, cancel)
}

/// Diagnostic-only switches for comparing the terrain-support proxy and the
/// previous rounded height estimate with TFC's first-available origin.
pub(super) fn generate_with_index_options(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    index: u32,
    _full_height_support_proxy: bool,
    first_available_origin: bool,
    cancel: &impl Fn() -> bool,
) -> Vec<SurfacePlant> {
    generate_inner(
        world,
        chunk_x,
        chunk_z,
        index,
        _full_height_support_proxy,
        first_available_origin,
        false,
        cancel,
    )
    .0
}

fn generate_inner(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    index: u32,
    _full_height_support_proxy: bool,
    first_available_origin: bool,
    include_litter: bool,
    cancel: &impl Fn() -> bool,
) -> (Vec<SurfacePlant>, Vec<super::ForestLitterBlock>) {
    if !feature_biome_allowed(world, chunk_x, chunk_z, "tfg:earth/forest") {
        return (Vec::new(), Vec::new());
    }
    let forest = world.region.forest_type(chunk_x, chunk_z).min(4) as usize;
    if forest == 0 {
        return (Vec::new(), Vec::new());
    }
    let config = configs()[forest];
    let seed = super::feature_seed(chunk_x, chunk_z, ORIGIN_Y, 8, index);
    let mut random = FeatureRandom::new(seed);
    if random.next_float() > config.chance {
        return (Vec::new(), Vec::new());
    }
    let requested_trees = config.trees.sample(&mut random);
    let forest_noise = forest_noise(world.seed);
    let density = forest_noise.density(chunk_x, chunk_z);
    let bush_count = config
        .bushes
        .map(|p| p.sample(&mut random))
        .unwrap_or_else(|| (requested_trees as f32 * density) as i32);
    // ForestFeature samples bush count before density-scaling tree attempts.
    let weirdness = forest_noise.weirdness(chunk_x, chunk_z);
    let tree_count = (requested_trees as f32 * (0.6f32 + 0.9f32 * density)) as i32;
    let origin_x = chunk_x.wrapping_mul(16);
    let origin_z = chunk_z.wrapping_mul(16);
    let mut plants = Vec::new();
    let mut placed_trees = false;
    let mut occupied = std::collections::HashSet::<(i32, i32, i32)>::new();
    let mut root_states = HashMap::<(i32, i32, i32), Arc<str>>::new();

    for _ in 0..tree_count.max(0) {
        if cancel() {
            break;
        }
        let x = origin_x.wrapping_add(random.next_int_bound(16));
        let z = origin_z.wrapping_add(random.next_int_bound(16));
        let Some(height) = world.height(x, z) else {
            continue;
        };
        // TFC's chunk height filler truncates its sample to an integer surface
        // block, while the OCEAN_FLOOR_WG height query used by ForestFeature is
        // first-available (one above that block).
        let ground_y = if first_available_origin {
            crate::caves::column(world, x, z)
                .map(|column| column.solid_y.saturating_add(1))
                .unwrap_or(height as i32 + 1)
        } else {
            height.round() as i32
        };
        // CARVERS/FEATURES retain OCEAN_FLOOR_WG while updates target the
        // live post-feature maps. Earlier decorations affect block tests, not
        // the frozen height query (ChunkStatus.POST_FEATURES).
        let Some(entry_id) = choose_entry(
            world,
            x,
            z,
            ground_y,
            weirdness,
            use_weirdness(),
            &mut random,
        ) else {
            continue;
        };

        // ForestFeature first asks the entry for an ocean-floor position. Floating
        // entries (mangroves in the installed pack) then require the sea floor to be
        // near sea level and move the tree origin to WORLD_SURFACE_WG plus one draw.
        // Keep the biome/climate selection at ground_y, but use the relocated Y for
        // variant selection and the tree feature itself.
        let entry = templates::forest_entry(&entry_id);
        let mut y = ground_y;
        if entry.is_some_and(|entry| entry.floating) {
            if ground_y < crate::surface_water::SEA_LEVEL - 4 {
                continue;
            }
            y = world_surface_y(world, x, z).unwrap_or(ground_y);
            y = y.saturating_add(random.next_int_bound(2));
        }

        let mut selected_variant = TreeVariant::Normal;
        let krummholz = templates::entry_variant(&entry_id, TreeVariant::Krummholz);
        let krum_chance = if y > 110 {
            1.0
        } else {
            ((y - 90) as f32 / 20.0).clamp(0.0, 1.0)
        };
        if krummholz.is_some() && random.next_float() < krum_chance {
            selected_variant = TreeVariant::Krummholz;
        } else {
            let old_chance = entry.and_then(|e| e.old_growth_chance).unwrap_or(6);
            if config.old_growth && old_chance > 0 && random.next_int_bound(old_chance as i32) == 0
            {
                selected_variant = TreeVariant::OldGrowth;
            } else {
                let dead_chance = entry.and_then(|e| e.dead_chance).unwrap_or(75);
                if dead_chance > 0 && random.next_int_bound(dead_chance as i32) == 0 {
                    selected_variant = TreeVariant::Dead;
                } else {
                    let spoiler_chance = entry
                        .and_then(|e| e.spoiler_old_growth_chance)
                        .unwrap_or(200);
                    if config.spoilers
                        && spoiler_chance > 0
                        && random.next_int_bound(spoiler_chance as i32) == 0
                    {
                        selected_variant = TreeVariant::OldGrowth;
                    }
                }
            }
        }
        let Some(feature_id) = templates::entry_variant(&entry_id, selected_variant) else {
            continue;
        };
        // The template system consumes placement rotation, mirror, then weighted
        // structure choice from this same worldgen stream.
        let Some(mut template) = templates::select_with_random(feature_id, &mut random) else {
            // TFC's krummholz configured feature is a plant block, not a structure
            // template. Its configured uniform height consumes one bounded draw.
            if feature_id.ends_with("_krummholz") {
                let height = 1 + random.next_int_bound(4);
                let species = feature_id.strip_prefix("tfc:tree/").unwrap_or(feature_id);
                for dy in 0..height {
                    plants.push(SurfacePlant {
                        x,
                        y: y.saturating_add(dy),
                        z,
                        kind: SurfacePlantKind::Tree,
                        variant: species.to_owned(),
                    });
                }
                placed_trees = true;
            }
            continue;
        };
        // Feature template selection occurs before TFC checks placement. A
        // rejected location consumes its rotation/mirror/template draw but must
        // not consume roots, trunk height or palette draws for later trees.
        if !valid_tree_location(
            world,
            x,
            z,
            y,
            feature_id,
            &template,
            _full_height_support_proxy,
            &occupied,
        ) {
            continue;
        }
        if let Some(roots) = template.root_system.as_ref()
            && roots.special_placer.is_none()
        {
            let width = i32::from(roots.width).max(1);
            let height = i32::from(roots.height).max(1);
            let passes = if template.feature_type == templates::TreeFeatureType::StackedTree {
                2
            } else {
                1
            };
            for _ in 0..u32::from(roots.tries) * passes {
                let dx = random.next_int_bound(width) - random.next_int_bound(width);
                let dz = random.next_int_bound(width) - random.next_int_bound(width);
                let dy = -random.next_int_bound(height);
                let key = (
                    x.saturating_add(dx),
                    y.saturating_sub(1).saturating_add(dy),
                    z.saturating_add(dz),
                );
                let block = root_states
                    .get(&key)
                    .cloned()
                    .or_else(|| crate::ground::block_before_forest_at(world, key.0, key.1, key.2));
                if let Some(mapping) = block.as_deref().and_then(|block| {
                    roots
                        .blocks
                        .iter()
                        .find(|m| m.replace.iter().any(|r| r == block))
                }) {
                    // IWeighted.get consumes a double even for a singleton.
                    // All ordinary root mappings in the installed profile
                    // have one output state.
                    let _choice = random.next_double();
                    if let Some(state) = mapping.with.first() {
                        root_states.insert(key, Arc::from(state.as_str()));
                    }
                }
            }
        }
        let trunk_height = template.finish_trunk(&mut random);
        template.finish_palette(&mut random);
        // A top-down crown alone misses the lower trunk when a later attempt
        // tests air at its root. Record every configured trunk voxel exactly as
        // TreeHelpers.placeTrunk does, including mirrored wide trunks.
        if let Some(trunk) = &template.trunk {
            for dx in 0..=i32::from(trunk.wide) {
                for dz in 0..=i32::from(trunk.wide) {
                    let (dx, dz) = template.placement_offset(dx, dz);
                    for dy in 0..i32::from(trunk_height) {
                        occupied.insert((
                            x.saturating_add(dx),
                            y.saturating_add(dy),
                            z.saturating_add(dz),
                        ));
                    }
                }
            }
        }
        let species = feature_id.strip_prefix("tfc:tree/").unwrap_or(feature_id);
        let before_cells = plants.len();
        for cell in template.cells {
            let cell_x = x.saturating_add(cell.x as i32);
            let cell_z = z.saturating_add(cell.z as i32);
            let cell_y = y.saturating_add(cell.y as i32);
            // TFC tree templates only replace air, leaves, or other
            // worldgen-replaceable cells. The terrain sampler cannot model
            // every decoration or cave, but it can omit cells buried by the
            // known surface profile on slopes.
            if crate::ground::pre_forest_sample(world, cell_x, cell_z)
                .is_some_and(|surface| cell_y <= surface.y)
            {
                continue;
            }
            occupied.insert((cell_x, cell_y, cell_z));
            plants.push(SurfacePlant {
                x: cell_x,
                y: cell_y,
                z: cell_z,
                kind: if cell.kind == TemplateBlockKind::Wood {
                    SurfacePlantKind::TreeWood
                } else {
                    SurfacePlantKind::Tree
                },
                variant: species.to_owned(),
            });
        }
        placed_trees |= plants.len() > before_cells;
    }
    let mut litter = Vec::new();
    if include_litter && placed_trees && !cancel() {
        // ForestFeature places bushes before debris. The target's surface states
        // are not available in seed-only mode; simulate their RNG calls against
        // the same source support profile and then continue into groundcover.
        advance_bushes(
            world,
            chunk_x,
            chunk_z,
            bush_count,
            weirdness,
            &mut random,
            &mut occupied,
        );
        litter.extend(place_forest_groundcover(
            world,
            chunk_x,
            chunk_z,
            config,
            weirdness,
            &mut random,
            cancel,
            &occupied,
        ));
    }
    (plants, litter)
}

fn world_surface_y(world: &World, x: i32, z: i32) -> Option<i32> {
    let column = crate::caves::column(world, x, z)?;
    Some(
        column
            .fluid
            .map_or(column.solid_y, |(_, y)| y)
            .saturating_add(1),
    )
}

fn debris_surface(world: &World, x: i32, z: i32) -> Option<(i32, crate::ground::GroundSample)> {
    let support = crate::ground::pre_forest_sample(world, x, z)?;
    Some((support.y.saturating_add(1), support))
}

fn choose_debris_entry(
    world: &World,
    x: i32,
    z: i32,
    random: &mut FeatureRandom,
    weirdness: f32,
) -> Option<String> {
    let (y, _) = debris_surface(world, x, z)?;
    choose_entry(world, x, z, y, weirdness, use_weirdness(), random)
}

fn sturdy_support(support: &crate::ground::GroundSample) -> bool {
    !matches!(
        support.kind,
        crate::ground::GroundKind::Ice | crate::ground::GroundKind::Other
    )
}

fn bush_plantable(support: &crate::ground::GroundSample) -> bool {
    let block = support.block.as_ref();
    block == "minecraft:dirt"
        || block == "minecraft:coarse_dirt"
        || block == "minecraft:rooted_dirt"
        || block == "minecraft:grass_block"
        || block == "minecraft:mycelium"
        || block == "tfc:farmland"
        || block == "minecraft:farmland"
        || block.starts_with("tfc:grass/")
        || block.starts_with("tfc:clay_grass/")
        || block.starts_with("tfc:rooted_dirt/")
        || block.starts_with("tfg:grass/")
        || block.starts_with("tfg:clay_grass/")
}

fn replaceable_at(
    world: &World,
    x: i32,
    y: i32,
    z: i32,
    occupied: &HashSet<(i32, i32, i32)>,
) -> bool {
    !occupied.contains(&(x, y, z))
        && crate::ground::pre_forest_sample(world, x, z).is_some_and(|support| y > support.y)
}

fn advance_bush_part(
    world: &World,
    center: (i32, i32, i32),
    decay: f32,
    needs_empty_center: bool,
    random: &mut FeatureRandom,
    occupied: &mut HashSet<(i32, i32, i32)>,
) {
    let (x, y, z) = center;
    let center_replaceable = replaceable_at(world, x, y, z, occupied);
    if center_replaceable {
        occupied.insert(center);
    } else if needs_empty_center {
        return;
    }
    // Helpers.DIRECTIONS contains UP, DOWN, and the four horizontal faces;
    // placeBushPart skips DOWN. State occupancy decides which calls consume RNG.
    for (dx, dy, dz) in [(0, 1, 0), (0, 0, -1), (0, 0, 1), (-1, 0, 0), (1, 0, 0)] {
        let pos = (x + dx, y + dy, z + dz);
        if replaceable_at(world, pos.0, pos.1, pos.2, occupied) && random.next_float() < decay {
            occupied.insert(pos);
        }
    }
}

/// Advance ForestFeature's bush loop on the estimated source terrain. The generated
/// block mask is used only to retain RNG/replaceability state for the later debris
/// phase; tree/canopy output remains owned by the existing generator.
fn advance_bushes(
    world: &World,
    cx: i32,
    cz: i32,
    bush_count: i32,
    weirdness: f32,
    random: &mut FeatureRandom,
    occupied: &mut HashSet<(i32, i32, i32)>,
) {
    for _ in 0..bush_count.max(0) {
        let x = cx.wrapping_mul(16).wrapping_add(random.next_int_bound(16));
        let z = cz.wrapping_mul(16).wrapping_add(random.next_int_bound(16));
        let Some(y) = world_surface_y(world, x, z) else {
            continue;
        };
        let Some(entry_id) = choose_entry(world, x, z, y, weirdness, use_weirdness(), random)
        else {
            continue;
        };
        let Some(entry) = forest_litter_entries().get(&entry_id) else {
            continue;
        };
        let (Some(_log), Some(leaves)) = (&entry.bush_log, &entry.bush_leaves) else {
            continue;
        };
        let Some((_, support)) = debris_surface(world, x, z) else {
            continue;
        };
        if !bush_plantable(&support) || !replaceable_at(world, x, y, z, occupied) {
            continue;
        }
        advance_bush_part(world, (x, y, z), 1.0, true, random, occupied);
        let mut branch = (x, y, z);
        for _ in 0..5 {
            if random.next_int_bound(4) == 0 {
                let (dx, dz) = match random.next_int_bound(4) {
                    0 => (0, -1),
                    1 => (1, 0),
                    2 => (0, 1),
                    _ => (-1, 0),
                };
                branch = (branch.0 + dx, branch.1, branch.2 + dz);
                advance_bush_part(world, branch, 0.7, false, random, occupied);
                if random.next_int_bound(6) == 0 {
                    advance_bush_part(
                        world,
                        (branch.0, branch.1 + 1, branch.2),
                        0.6,
                        false,
                        random,
                        occupied,
                    );
                    break;
                }
            }
        }
        let _ = leaves; // State is needed only to establish the branch provider.
    }
}

fn placed_litter_block(
    world: &World,
    x: i32,
    z: i32,
    occupied: &HashSet<(i32, i32, i32)>,
) -> Option<(i32, crate::ground::GroundSample)> {
    let (y, support) = debris_surface(world, x, z)?;
    if !sturdy_support(&support) || !replaceable_at(world, x, y, z, occupied) {
        return None;
    }
    Some((y, support))
}

fn pick_groundcover(entry: &ForestLitterEntry, random: &mut FeatureRandom) -> Option<String> {
    if entry.groundcover.is_empty() {
        return None;
    }
    let total: f64 = entry
        .groundcover
        .iter()
        .filter(|v| v.weight > 0.0)
        .map(|v| v.weight)
        .sum();
    if total <= 0.0 {
        return None;
    }
    let value = random.next_double() * total;
    let mut cumulative = 0.0;
    entry
        .groundcover
        .iter()
        .find_map(|item| {
            if item.weight <= 0.0 {
                return None;
            }
            cumulative += item.weight;
            (value < cumulative).then(|| item.block.clone())
        })
        .or_else(|| entry.groundcover.last().map(|v| v.block.clone()))
}

fn place_forest_groundcover(
    world: &World,
    cx: i32,
    cz: i32,
    config: ForestKindConfig,
    weirdness: f32,
    random: &mut FeatureRandom,
    cancel: &impl Fn() -> bool,
    tree_occupied: &HashSet<(i32, i32, i32)>,
) -> Vec<super::ForestLitterBlock> {
    let origin_x = cx.wrapping_mul(16);
    let origin_z = cz.wrapping_mul(16);
    let mut occupied = tree_occupied.clone();
    let mut output = Vec::new();

    let groundcover_tries = config.groundcover.sample(random);
    if groundcover_tries > 0 {
        let sample_x = origin_x.wrapping_add(random.next_int_bound(16));
        let sample_z = origin_z.wrapping_add(random.next_int_bound(16));
        if let Some(entry_id) = choose_debris_entry(world, sample_x, sample_z, random, weirdness)
            && let Some(entry) = forest_litter_entries().get(&entry_id)
        {
            for _ in 0..groundcover_tries {
                if cancel() {
                    return output;
                }
                let Some(block) = pick_groundcover(entry, random) else {
                    continue;
                };
                let x = origin_x.wrapping_add(random.next_int_bound(16));
                let z = origin_z.wrapping_add(random.next_int_bound(16));
                if let Some((y, _)) = placed_litter_block(world, x, z, &occupied) {
                    occupied.insert((x, y, z));
                    output.push(super::ForestLitterBlock { x, y, z, block });
                }
            }
        }
    }

    // placeLeafPile always makes its entry-selection seed query, even when the
    // sampled pile count is zero; preserve that source RNG consumption.
    let leaf_tries = config.leaf_piles.sample(random);
    let sample_x = origin_x.wrapping_add(random.next_int_bound(16));
    let sample_z = origin_z.wrapping_add(random.next_int_bound(16));
    if let Some(entry_id) = choose_debris_entry(world, sample_x, sample_z, random, weirdness)
        && let Some(block) = forest_litter_entries()
            .get(&entry_id)
            .and_then(|e| e.fallen_leaves.clone())
    {
        for _ in 0..leaf_tries.max(0) {
            if cancel() {
                return output;
            }
            let origin_x = origin_x.wrapping_add(random.next_int_bound(16));
            let origin_z = origin_z.wrapping_add(random.next_int_bound(16));
            let Some((origin_y, _)) = debris_surface(world, origin_x, origin_z) else {
                continue;
            };
            for _ in 0..8 {
                let x = origin_x.wrapping_add(random.next_int_bound(5) - 2);
                let z = origin_z.wrapping_add(random.next_int_bound(5) - 2);
                let Some((y, support)) = placed_litter_block(world, x, z, &occupied) else {
                    continue;
                };
                // LeafPile keeps its original Y for all eight horizontal
                // offsets. A slope cannot move each leaf to a new height.
                if y != origin_y {
                    continue;
                }
                let Some(water_y) = world.surface_water_y(x, z) else {
                    random.next_int_bound(5); // FallenLeavesBlock layers: 1..5.
                    occupied.insert((x, y, z));
                    output.push(super::ForestLitterBlock {
                        x,
                        y,
                        z,
                        block: block.clone(),
                    });
                    continue;
                };
                // The source requires empty fluid at the target; water above or
                // at this first-air position suppresses the pile.
                if water_y < y && sturdy_support(&support) {
                    random.next_int_bound(5);
                    occupied.insert((x, y, z));
                    output.push(super::ForestLitterBlock {
                        x,
                        y,
                        z,
                        block: block.clone(),
                    });
                }
            }
        }
    }
    output
}

fn valid_tree_location(
    world: &World,
    x: i32,
    z: i32,
    y: i32,
    feature_id: &str,
    template: &templates::SelectedTemplate,
    _full_height_support_proxy: bool,
    occupied: &std::collections::HashSet<(i32, i32, i32)>,
) -> bool {
    let Some((width, _height, ground_type)) = templates::placement(feature_id) else {
        return true;
    };
    valid_tree_placement_with_occupancy(
        world,
        x,
        z,
        y,
        width,
        _height,
        ground_type,
        template,
        _full_height_support_proxy,
        occupied,
    )
}

pub(super) fn valid_tree_placement(
    world: &World,
    x: i32,
    z: i32,
    y: i32,
    width: u8,
    _height: u8,
    ground_type: &str,
    template: &templates::SelectedTemplate,
    _full_height_support_proxy: bool,
) -> bool {
    valid_tree_placement_with_occupancy(
        world,
        x,
        z,
        y,
        width,
        _height,
        ground_type,
        template,
        _full_height_support_proxy,
        &std::collections::HashSet::new(),
    )
}

fn valid_tree_placement_with_occupancy(
    world: &World,
    x: i32,
    z: i32,
    y: i32,
    width: u8,
    height: u8,
    ground_type: &str,
    template: &templates::SelectedTemplate,
    _full_height_support_proxy: bool,
    occupied: &std::collections::HashSet<(i32, i32, i32)>,
) -> bool {
    let floating = ground_type == "floating";
    // Match TreePlacementConfig.mayPlaceInWater exactly. The source notably
    // excludes SHALLOW_ALLOW_SALTWATER from this predicate.
    let water_placement = may_place_in_water(ground_type);
    let width = i32::from(template.placement_width.max(width).max(1));
    let min = (1 - width) / 2;
    let max = width / 2;
    for dz in min..=max {
        for dx in min..=max {
            let (dx, dz) = template.placement_offset(dx, dz);
            let (px, pz) = (x.saturating_add(dx), z.saturating_add(dz));
            if matches!(
                world.volcanic_surface(px, pz),
                Some(
                    crate::volcanic_lava::Surface::Lava(_)
                        | crate::volcanic_lava::Surface::Magma(_)
                )
            ) {
                return false;
            }
            let Some(surface) = crate::ground::pre_forest_sample(world, px, pz) else {
                return false;
            };
            let water_top = world.surface_water_y(px, pz).or_else(|| {
                world
                    .is_surface_water(px, pz)
                    .then_some(crate::surface_water::SEA_SURFACE_BLOCK_Y)
            });
            let water_at_origin = water_top.is_some_and(|top| y <= top);
            if water_at_origin && ground_type.starts_with("submerged") {
                let biome = world.biome_id(px, pz);
                if crate::biome::ids::has_river(biome)
                    || (ground_type == "submerged" && crate::biome::is_ocean(biome))
                {
                    return false;
                }
            }
            if floating {
                let Some(surface) = world_surface_y(world, px, pz) else {
                    return false;
                };
                if !(surface..=surface.saturating_add(1)).contains(&y)
                    || !(water_at_origin
                        || world.height(px, pz).is_some_and(|ground| {
                            ground as i32 + 1 >= crate::surface_water::SEA_LEVEL - 4
                        }))
                {
                    return false;
                }
                continue;
            }
            if water_at_origin && !water_placement {
                return false;
            }
            if occupied.contains(&(px, y, pz)) {
                return false;
            }
            if surface.y.saturating_add(1) != y {
                return false;
            }
            if !ground_supports(ground_type, water_at_origin, &surface.block) {
                return false;
            }
            if water_at_origin && !water_placement {
                return false;
            }
            // isValidTrunk checks every configured-width column for each block
            // from y=1 through height-1, independent of canopy overlap.
            let checks_trunk = matches!(
                template.feature_type,
                templates::TreeFeatureType::RandomTree | templates::TreeFeatureType::OverlayTree
            );
            for dy in 1..i32::from(height) {
                if !checks_trunk {
                    break;
                }
                if occupied.contains(&(px, y.saturating_add(dy), pz)) {
                    return false;
                }
                if crate::ground::pre_forest_sample(world, px, pz)
                    .is_some_and(|top| top.y >= y.saturating_add(dy))
                {
                    return false;
                }
            }
        }
    }
    true
}

fn may_place_in_water(ground_type: &str) -> bool {
    matches!(
        ground_type,
        "shallow_water" | "submerged" | "submerged_allow_saltwater"
    )
}

fn ground_supports(ground_type: &str, water: bool, block: &str) -> bool {
    let tree_grows_on = ground_tag("tree_grows_on", block);
    let sea_bush = ground_tag("sea_bush_plantable_on", block);
    if ground_type.starts_with("submerged") && water {
        // The underwater helper returns only this tag for a water-filled
        // origin; it does not also accept tree_grows_on.
        sea_bush
    } else {
        tree_grows_on
            || (ground_type == "sand" && ground_tag("sand", block))
            || (water && may_place_in_water(ground_type) && sea_bush)
    }
}

fn ground_tag(tag: &'static str, block: &str) -> bool {
    static TAGS: OnceLock<serde_json::Value> = OnceLock::new();
    static TREE: OnceLock<HashSet<String>> = OnceLock::new();
    static SAND: OnceLock<HashSet<String>> = OnceLock::new();
    static SEA_BUSH: OnceLock<HashSet<String>> = OnceLock::new();
    static BAMBOO: OnceLock<HashSet<String>> = OnceLock::new();
    let value =
        TAGS.get_or_init(|| serde_json::from_str(TREE_GROUND).expect("tree ground profile"));
    let set = match tag {
        "tree_grows_on" => TREE.get_or_init(|| tag_set(&value["tree_grows_on"])),
        "sand" => SAND.get_or_init(|| tag_set(&value["sand"])),
        "sea_bush_plantable_on" => {
            SEA_BUSH.get_or_init(|| tag_set(&value["sea_bush_plantable_on"]))
        }
        _ => BAMBOO.get_or_init(|| tag_set(&value["bamboo_plantable_on"])),
    };
    set.contains(block)
}

fn tag_set(value: &serde_json::Value) -> HashSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.as_str().map(str::to_owned))
        .collect()
}

/// Predicate for TFG's outer bamboo/palm patch placement tag.
pub(super) fn bamboo_plantable_on(block: &str) -> bool {
    ground_tag("bamboo_plantable_on", block)
}

fn choose_entry(
    world: &World,
    x: i32,
    z: i32,
    y: i32,
    weirdness: f32,
    use_weirdness: bool,
    random: &mut FeatureRandom,
) -> Option<String> {
    let climate = world.climate(x, z);
    let temperature = adjusted_average_temp_by_elevation(climate.temperature as f32, y);
    let rainfall = climate.rainfall as f32;
    let forest = world.region.forest_type(x >> 4, z >> 4);
    let mut candidates: Vec<String> = Vec::new();
    for id in templates::forest_entry_order() {
        let Some(entry) = templates::forest_entry(id) else {
            continue;
        };
        let range = entry.climate;
        if !(range.min_temperature <= temperature
            && temperature <= range.max_temperature
            && range.min_rainfall <= rainfall
            && rainfall <= range.max_rainfall)
        {
            continue;
        }
        // ClimatePlacement defaults to non-fuzzy in all installed forest entries.
        let _ = forest;
        let avg_temp = (range.max_temperature - range.min_temperature) / 2.0;
        let avg_rain = (range.max_rainfall - range.min_rainfall) / 2.0;
        let last_distance = (avg_rain + avg_temp - avg_temp - avg_rain) / 2.0;
        let distance = (rainfall + temperature - avg_temp - avg_rain) / 2.0;
        if last_distance < distance {
            candidates.push(id.clone());
        } else {
            candidates.insert(0, id.clone());
        }
    }
    if candidates.is_empty() {
        return None;
    }
    if use_weirdness {
        let shift = (weirdness * (candidates.len() as f32 - 1.0)) as usize;
        let candidate_count = candidates.len();
        candidates.rotate_left(shift.min(candidate_count));
        for i in (-1i32..=1).rev() {
            if candidates.len() <= 1 {
                break;
            }
            let threshold = weirdness - (0.15f32 * i as f32) + 0.1f32;
            if random.next_float() > threshold {
                candidates.pop();
            }
        }
    }
    let mut selected = 0usize;
    while selected < candidates.len() - 1 && random.next_float() < 0.6 {
        selected += 1;
    }
    candidates.get(selected).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ordered_forest_entry_has_climate_and_selectable_normal_feature() {
        let entries = templates::forest_entry_order();
        assert_eq!(entries.len(), 61, "installed forest feature order changed");
        let mut unresolved = Vec::new();
        for id in entries {
            let Some(entry) = templates::forest_entry(id) else {
                unresolved.push(format!("{id}: missing entry"));
                continue;
            };
            if entry.climate.min_temperature > entry.climate.max_temperature
                || entry.climate.min_rainfall > entry.climate.max_rainfall
            {
                unresolved.push(format!("{id}: invalid climate range"));
            }
            let Some(feature) = templates::entry_variant(id, TreeVariant::Normal) else {
                unresolved.push(format!("{id}: missing normal feature"));
                continue;
            };
            if templates::select(feature, 0).is_none() {
                unresolved.push(format!("{id}: unresolved configured feature {feature}"));
            }
        }
        assert!(
            unresolved.is_empty(),
            "unresolved forest refs: {unresolved:?}"
        );
    }

    #[test]
    fn merged_survival_tags_keep_sand_and_ordinary_ground_distinct() {
        assert!(ground_tag("tree_grows_on", "tfg:grass/mollisol"));
        assert!(ground_tag("tree_grows_on", "tfg:duff/loam"));
        assert!(ground_tag("tree_grows_on", "tfg:egh_planter"));
        assert!(!ground_tag("tree_grows_on", "minecraft:sand"));
        assert!(ground_tag("sand", "tfc:sand/pink"));
        assert!(ground_tag("sand", "minecraft:red_sand"));
        assert!(!ground_tag("sand", "tfc:grass/loam"));
    }

    #[test]
    fn bamboo_tag_keeps_vanilla_base_and_mod_additions() {
        assert!(bamboo_plantable_on("minecraft:sand"));
        assert!(bamboo_plantable_on("minecraft:gravel"));
        assert!(bamboo_plantable_on("tfc:grass/loam"));
        assert!(bamboo_plantable_on("tfg:egh_planter"));
        assert!(!bamboo_plantable_on("tfc:rock/granite"));
    }

    #[test]
    fn shallow_saltwater_source_quirk_is_preserved() {
        assert!(!may_place_in_water("shallow_allow_saltwater"));
        assert!(may_place_in_water("shallow_water"));
        assert!(may_place_in_water("submerged_allow_saltwater"));
    }

    #[test]
    fn extracted_forest_litter_has_source_tree_provider_blocks() {
        let oak = forest_litter_entries().get("tfc:tree/oak_entry").unwrap();
        assert_eq!(
            oak.fallen_leaves.as_deref(),
            Some("tfc:wood/fallen_leaves/oak")
        );
        assert!(
            oak.groundcover
                .iter()
                .any(|state| state.block == "tfc:wood/twig/oak")
        );
        assert!(
            oak.groundcover
                .iter()
                .any(|state| state.block == "tfc:wood/fallen_leaves/oak")
        );
    }

    #[test]
    fn enabling_forest_litter_does_not_change_tree_projection() {
        let world = World::new(0, crate::model::Settings::default());
        let Some((cx, cz)) = (0..24)
            .flat_map(|z| (0..24).map(move |x| (x, z)))
            .find(|(cx, cz)| {
                world.region.forest_type(*cx, *cz) > 0
                    && feature_biome_allowed(&world, *cx, *cz, "tfg:earth/forest")
            })
        else {
            panic!("fixture scan should find at least one source forest chunk");
        };
        let index = forest_index().unwrap();
        let tree_only = generate_inner(&world, cx, cz, index, false, true, false, &|| false).0;
        let with_litter = generate_inner(&world, cx, cz, index, false, true, true, &|| false).0;
        assert!(
            !tree_only.is_empty(),
            "comparison fixture should contain tree blocks"
        );
        assert_eq!(tree_only, with_litter);
    }

    #[test]
    fn underwater_support_uses_only_the_source_specific_tag() {
        assert!(ground_supports("submerged", true, "tfc:sand/white"));
        assert!(!ground_supports("submerged", true, "tfc:grass/loam"));
        assert!(ground_supports("submerged", false, "tfc:grass/loam"));
        assert!(ground_supports("shallow_water", true, "tfc:grass/loam"));
    }
}
