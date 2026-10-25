//! Deterministic placements for TFC wild-crop patch features.
//!
//! Crop placement settings come from the installed TFG 0.9.25 data pack and its
//! bundled TFC 3.2.25 resources. The outer placement modifiers and vanilla
//! `random_patch` random-call order are reproduced here. Surface block predicates
//! are evaluated against TerraLens' predicted height model, so these are seed
//! predictions rather than confirmed block placements.

use super::{FeatureRandom, SurfacePlant, SurfacePlantKind, templates::TemplateBlockKind};
use crate::{biome, world::World};
use serde_json::Value;
use std::{collections::HashSet, sync::OnceLock};

const CROP_FEATURES: &str = include_str!("assets/crop_features.json");

#[derive(Clone, Copy)]
struct ClimateRule {
    min_temperature: f64,
    max_temperature: f64,
    min_rainfall: f64,
    max_rainfall: f64,
    min_forest: u8,
    max_forest: u8,
}

struct CropFeature {
    id: String,
    block: String,
    chance: u32,
    climate: ClimateRule,
    tries: u32,
    xz_spread: u32,
    y_spread: u32,
    spreading_crop: bool,
    tree_placement_width: u8,
    tree_placement_height: u8,
    tree_ground_type: String,
}

fn forest_level(value: &Value, default: u8) -> u8 {
    match value.as_str().unwrap_or("") {
        "none" => 0,
        "sparse" => 1,
        "edge" => 2,
        "normal" => 3,
        "old_growth" => 4,
        _ => default,
    }
}

fn block_id(record: &Value) -> Option<String> {
    fn find(value: &Value) -> Option<String> {
        if let Some(id) = value
            .get("Name")
            .and_then(Value::as_str)
            .filter(|id| id.contains(':'))
        {
            return Some(id.to_owned());
        }
        if let Some(id) = value.get("block").and_then(Value::as_str) {
            return Some(id.to_owned());
        }
        if let Some(id) = value
            .get("config")
            .and_then(|config| config.get("block"))
            .and_then(Value::as_str)
        {
            return Some(id.to_owned());
        }
        value.as_object()?.values().find_map(find)
    }
    record.get("inner").and_then(find).or_else(|| {
        record
            .get("configured")
            .and_then(|configured| configured.get("config"))
            .and_then(|config| config.get("feature"))
            .and_then(find)
    })
}

fn parse_feature(record: &Value) -> Option<CropFeature> {
    let id = record.get("id")?.as_str()?.to_owned();
    // Palm fruit patches are dynamic_random_patch features; their child entries
    // are parsed here so the same deterministic stream can emit tree markers.
    let configured = record.get("configured")?;
    let feature_type = configured.get("type")?.as_str()?;
    let config = configured.get("config")?;
    let dynamic_palm = feature_type == "tfc:dynamic_random_patch"
        && (id.ends_with("/coconut_patch") || id.ends_with("/date_patch"));
    if feature_type != "minecraft:random_patch" && !dynamic_palm {
        return None;
    }
    let tries = config.get("tries")?.as_u64()? as u32;
    // Minecraft's RandomPatchConfiguration defaults omitted spreads to 7 and 3.
    let xz_spread = config
        .get("xz_spread")
        .and_then(Value::as_u64)
        .unwrap_or(if dynamic_palm { 7 } else { 0 }) as u32;
    let y_spread = config
        .get("y_spread")
        .and_then(Value::as_u64)
        .unwrap_or(if dynamic_palm { 3 } else { 0 }) as u32;
    let tree_placement = record
        .get("inner")
        .and_then(|inner| inner.get("config"))
        .and_then(|inner| inner.get("placement"));
    let placement = record.get("placed")?.get("placement")?.as_array()?;
    let chance = placement
        .iter()
        .find(|modifier| {
            modifier.get("type").and_then(Value::as_str) == Some("minecraft:rarity_filter")
        })
        .and_then(|modifier| modifier.get("chance"))
        .and_then(Value::as_u64)
        .unwrap_or(1) as u32;
    let climate = placement
        .iter()
        .find(|modifier| modifier.get("type").and_then(Value::as_str) == Some("tfc:climate"))?;
    let block = block_id(record)
        .or_else(|| dynamic_palm.then(|| id.strip_suffix("_patch").unwrap_or(&id).to_owned()))?;
    Some(CropFeature {
        id,
        block,
        chance: chance.max(1),
        climate: ClimateRule {
            min_temperature: climate
                .get("min_temperature")
                .and_then(Value::as_f64)
                .unwrap_or(f64::NEG_INFINITY),
            max_temperature: climate
                .get("max_temperature")
                .and_then(Value::as_f64)
                .unwrap_or(f64::INFINITY),
            min_rainfall: climate
                .get("min_rainfall")
                .and_then(Value::as_f64)
                .unwrap_or(f64::NEG_INFINITY),
            max_rainfall: climate
                .get("max_rainfall")
                .and_then(Value::as_f64)
                .unwrap_or(f64::INFINITY),
            min_forest: forest_level(climate.get("min_forest").unwrap_or(&Value::Null), 0),
            max_forest: forest_level(climate.get("max_forest").unwrap_or(&Value::Null), 4),
        },
        tries,
        xz_spread,
        y_spread,
        tree_placement_width: tree_placement
            .and_then(|placement| placement.get("width"))
            .and_then(Value::as_u64)
            .unwrap_or(1) as u8,
        tree_placement_height: tree_placement
            .and_then(|placement| placement.get("height"))
            .and_then(Value::as_u64)
            .unwrap_or(1) as u8,
        tree_ground_type: tree_placement
            .and_then(|placement| placement.get("ground_type"))
            .and_then(Value::as_str)
            .unwrap_or("normal")
            .to_owned(),
        spreading_crop: record
            .get("inner")
            .and_then(|inner| inner.get("type"))
            .and_then(Value::as_str)
            == Some("tfc:spreading_crop"),
    })
}

fn crop_features() -> &'static [CropFeature] {
    static FEATURES: OnceLock<Vec<CropFeature>> = OnceLock::new();
    FEATURES.get_or_init(|| {
        let data: Value = serde_json::from_str(CROP_FEATURES)
            .expect("embedded TFG crop worldgen data must be valid JSON");
        data.get("features")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(parse_feature)
            .collect()
    })
}

fn climate_matches(world: &World, x: i32, z: i32, rule: ClimateRule) -> bool {
    let biome_id = world.region.biome(x, z);
    if biome::is_ocean(biome_id) || super::is_water_biome_id(biome::biome_name(biome_id)) {
        return false;
    }
    let climate = world.climate(x, z);
    let forest = world.region.forest_type(x >> 4, z >> 4);
    climate.temperature >= rule.min_temperature
        && climate.temperature <= rule.max_temperature
        && climate.rainfall >= rule.min_rainfall
        && climate.rainfall <= rule.max_rainfall
        && forest >= rule.min_forest
        && forest <= rule.max_forest
}

fn crop_patch_origin(random: &mut FeatureRandom, chance: u32) -> Option<(i32, i32)> {
    if random.next_float() >= 1.0 / chance.max(1) as f32 {
        return None;
    }
    Some((random.next_int_bound(16), random.next_int_bound(16)))
}

#[cfg(test)]
fn random_patch_offsets(
    random: &mut FeatureRandom,
    tries: u32,
    xz_spread: u32,
    y_spread: u32,
) -> Vec<(i32, i32)> {
    let x_bound = xz_spread.saturating_add(1).min(i32::MAX as u32) as i32;
    let y_bound = y_spread.saturating_add(1).min(i32::MAX as u32) as i32;
    (0..tries)
        .map(|_| {
            // Installed Minecraft 1.20.1 RandomPatchFeature bytecode: x−x, y−y,
            // z−z. Each bound is the configured spread plus one.
            let dx = random.next_int_bound(x_bound) - random.next_int_bound(x_bound);
            let _dy = random.next_int_bound(y_bound) - random.next_int_bound(y_bound);
            let dz = random.next_int_bound(x_bound) - random.next_int_bound(x_bound);
            (dx, dz)
        })
        .collect()
}

/// Recreates TFC/TFG crop patch candidates for one generation chunk.
///
/// `seed_for_feature` supplies Minecraft's step-9 feature seed after the caller
/// resolves the installed biome `FeatureSorter` index for each placed feature.
/// Returning `None` skips that feature (for example, it is absent from the
/// active biome feature list). The block-level `replaceable` and `would_survive`
/// predicates cannot be exact until the predicted terrain is backed by saved
/// chunk blocks; climate, rarity, in-square placement, and patch offsets are
/// evaluated in their source order.
/// Generate ordinary crops without palm-tree child features. The independent
/// canopy source already computes those tree templates and shares their cache.
pub(super) fn generate_crop_only(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    seed_for_feature: impl Fn(&str) -> Option<i64>,
    cancel: &impl Fn() -> bool,
) -> Vec<SurfacePlant> {
    let origin_x = chunk_x.saturating_mul(16);
    let origin_z = chunk_z.saturating_mul(16);
    let mut plants = Vec::new();
    let mut seen = HashSet::new();

    for feature in crop_features()
        .iter()
        .filter(|feature| !is_palm_patch(&feature.id))
    {
        if cancel() {
            break;
        }
        let Some(seed) = seed_for_feature(&feature.id) else {
            continue;
        };
        let mut random = FeatureRandom::new(seed);

        // RarityFilter consumes nextFloat even when chance is one. InSquarePlacement
        // then consumes x followed by z, each in [0, 15].
        let Some((square_x, square_z)) = crop_patch_origin(&mut random, feature.chance) else {
            continue;
        };
        let patch_x = origin_x.saturating_add(square_x);
        let patch_z = origin_z.saturating_add(square_z);
        if !climate_matches(world, patch_x, patch_z, feature.climate) {
            continue;
        }

        let x_bound = feature.xz_spread.saturating_add(1).min(i32::MAX as u32) as i32;
        let y_bound = feature.y_spread.saturating_add(1).min(i32::MAX as u32) as i32;
        for _ in 0..feature.tries {
            if cancel() {
                return plants;
            }
            // Vanilla RandomPatchFeature draws one attempt, invokes its child,
            // then draws the next attempt. Keep this interleaving because some
            // TFC child features (spreading crops) consume RNG themselves.
            let dx = random.next_int_bound(x_bound) - random.next_int_bound(x_bound);
            let _dy = random.next_int_bound(y_bound) - random.next_int_bound(y_bound);
            let dz = random.next_int_bound(x_bound) - random.next_int_bound(x_bound);
            let x = patch_x.saturating_add(dx);
            let z = patch_z.saturating_add(dz);
            let biome_id = world.region.biome(x, z);
            let biome_name = biome::biome_name(biome_id);
            if biome::is_ocean(biome_id) || super::is_water_biome_id(biome_name) {
                continue;
            }
            let Some(surface_y) = world.height(x, z) else {
                continue;
            };
            let y = surface_y.floor() as i32 + 1;
            // SpreadingCropFeature chooses an adjacent horizontal direction
            // before checking plantable ground and consumes it on every call.
            // The placement filters are known only approximately from predicted
            // terrain, so valid land/surface candidates are treated as calls.
            if feature.spreading_crop {
                let _direction = random.next_int_bound(4);
            }
            if !seen.insert((x, y, z, feature.block.as_str())) {
                continue;
            }
            plants.push(SurfacePlant {
                x,
                y,
                z,
                kind: SurfacePlantKind::Crop,
                variant: feature.block.clone(),
            });
        }
    }
    plants
}

/// Generate only the coconut/date tree crowns used by the standalone Trees
/// overlay. Keeps the tree view from paying for or duplicating crop patches.
pub(super) fn generate_tree_patches(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Vec<SurfacePlant> {
    let seed_for_feature = |id: &str| {
        if !super::feature_biome_allowed(world, chunk_x, chunk_z, id) {
            return None;
        }
        // Palm fruit patches are attached to the large-features step (8); all
        // ordinary crops use vegetal-decoration step (9).
        let step = if is_palm_patch(id) { 8 } else { 9 };
        let index = super::feature_index_in_step(id, step)?;
        Some(super::feature_seed(
            chunk_x,
            chunk_z,
            super::OVERWORLD_MIN_Y,
            step,
            index,
        ))
    };
    generate_palm_tree_patches(world, chunk_x, chunk_z, &seed_for_feature, cancel)
}

/// TFG coconut/date patches are dynamic_random_patch features whose inner feature
/// chooses a random structure-tree template. Model their exact feature seed,
/// outer rarity/climate/offset stream, dynamic attempt count and tree-template
/// choice. Block checks (sand, salt-water radius, flatness, replaceability) are
/// approximated against the predicted surface rather than generated chunk blocks.
fn generate_palm_tree_patches(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    seed_for_feature: &impl Fn(&str) -> Option<i64>,
    cancel: &impl Fn() -> bool,
) -> Vec<SurfacePlant> {
    let mut plants = Vec::new();
    let origin_x = chunk_x.saturating_mul(16);
    let origin_z = chunk_z.saturating_mul(16);
    let mut seen = HashSet::new();

    for feature in crop_features().iter().filter(|f| is_palm_patch(&f.id)) {
        if cancel() {
            break;
        }
        let Some(seed) = seed_for_feature(&feature.id) else {
            continue;
        };
        let mut random = FeatureRandom::new(seed);
        if random.next_float() >= 1.0 / feature.chance.max(1) as f32 {
            continue;
        }

        // The climate placed modifier precedes in_square in the installed JSON;
        // it is tested at the placed-feature input position (the chunk origin).
        if !climate_matches(world, origin_x, origin_z, feature.climate) {
            continue;
        }
        let patch_x = origin_x.saturating_add(random.next_int_bound(16));
        let patch_z = origin_z.saturating_add(random.next_int_bound(16));
        let Some(patch_height) = world.height(patch_x, patch_z) else {
            continue;
        };
        let patch_top = crate::ground::pre_forest_sample(world, patch_x, patch_z)
            .map_or(patch_height as i32 + 1, |ground| ground.y.saturating_add(1));
        // WORLD_SURFACE_WG sees water above the sea floor. Its below-origin
        // block cannot pass the bamboo planting predicate in a flooded column.
        if world.is_surface_water(patch_x, patch_z) || world.is_surface_fluid(patch_x, patch_z) {
            continue;
        }
        if !near_saltwater(
            world,
            patch_x,
            patch_top,
            patch_z,
            if feature.id.ends_with("/coconut_patch") {
                20
            } else {
                35
            },
        ) {
            continue;
        }
        // The outer patch has its own support predicate before FlatEnough:
        // bamboo's merged vanilla+TFG tag, distinct from the child tree's sand rule.
        if !crate::ground::pre_forest_sample(world, patch_x, patch_z)
            .is_some_and(|ground| super::trees::bamboo_plantable_on(&ground.block))
        {
            continue;
        }
        // `flat_enough` tests the block occupancy of a 5x5 surface patch and
        // lowers its input position by at most three blocks. Height-only terrain
        // can approximate this occupancy but cannot identify exact sand/fluid.
        let Some(patch_y) = flat_enough_position(world, patch_x, patch_z, patch_top, 0.2) else {
            continue;
        };

        // DynamicDensityRandomPatchFeature caps its base tries by forest type,
        // applies altitude reduction above sea level + 25, then multiplies by
        // TFG grass density * 2 and truncates to int. For base=3, forest caps
        // do not reduce the count. Sea level is 63 in the installed profile.
        let mut tries = 3.0f32;
        if patch_y > 88 {
            let altitude_factor = 1.0 - (((patch_y - 88) as f32 / 75.0).clamp(0.0, 1.0) * 0.8);
            tries *= altitude_factor;
        }
        tries = (tries * world.region.settings.grass_density as f32 * 2.0).max(0.0);
        let tries = tries as u32;
        let species = if feature.id.ends_with("/coconut_patch") {
            "coconut"
        } else {
            "date"
        };
        let child_id = format!("tfg:palm_tree/{species}");

        for _ in 0..tries {
            if cancel() {
                return plants;
            }
            // Omitted xz/y spreads use vanilla RandomPatchConfiguration's 7/3 defaults.
            let patch_attempt_x = patch_x.saturating_add(
                random.next_int_bound(feature.xz_spread as i32 + 1)
                    - random.next_int_bound(feature.xz_spread as i32 + 1),
            );
            let _patch_attempt_y = patch_y.saturating_add(
                random.next_int_bound(feature.y_spread as i32 + 1)
                    - random.next_int_bound(feature.y_spread as i32 + 1),
            );
            let patch_attempt_z = patch_z.saturating_add(
                random.next_int_bound(feature.xz_spread as i32 + 1)
                    - random.next_int_bound(feature.xz_spread as i32 + 1),
            );
            // Nested placed feature applies random_offset(-7..7, 0, -7..7),
            // followed by WORLD_SURFACE_WG, before its RandomTreeFeature.
            let x = patch_attempt_x.saturating_add(random.next_int_bound(15) - 7);
            let _child_dy = random.next_int_bound(1);
            let z = patch_attempt_z.saturating_add(random.next_int_bound(15) - 7);
            let Some(height) = world.height(x, z) else {
                continue;
            };
            let y = crate::ground::pre_forest_sample(world, x, z)
                .map_or(height as i32 + 1, |ground| ground.y.saturating_add(1));

            // RandomTreeFeature consumes placement rotation, mirror, then chooses
            // one of four structure variants before validating ground.
            let rotation = random.next_int_bound(4) as u8;
            let mirror = random.next_int_bound(3) as u8;
            let variant = random.next_int_bound(4);

            let structure_id = format!("{child_id}_{variant}");
            let Some(mut template) =
                super::templates::from_structure(&structure_id, rotation, mirror)
            else {
                continue;
            };
            if !super::trees::valid_tree_placement(
                world,
                x,
                z,
                y,
                feature.tree_placement_width,
                feature.tree_placement_height,
                &feature.tree_ground_type,
                &template,
                false,
            ) {
                continue;
            }
            template.finish_palette(&mut random);
            for cell in template.cells {
                let column_x = x.saturating_add(cell.x as i32);
                let column_z = z.saturating_add(cell.z as i32);
                let top = y.saturating_add(cell.y as i32);
                if world
                    .height(column_x, column_z)
                    .is_some_and(|surface| top <= surface as i32)
                {
                    continue;
                }
                if !seen.insert((column_x, top, column_z, species)) {
                    continue;
                }
                plants.push(SurfacePlant {
                    x: column_x,
                    y: top,
                    z: column_z,
                    kind: if cell.kind == TemplateBlockKind::Wood {
                        SurfacePlantKind::TreeWood
                    } else {
                        SurfacePlantKind::Tree
                    },
                    variant: structure_id.clone(),
                });
            }
        }
    }
    plants
}

fn is_palm_patch(id: &str) -> bool {
    id.ends_with("/coconut_patch") || id.ends_with("/date_patch")
}

fn near_saltwater(world: &World, x: i32, y: i32, z: i32, radius: i32) -> bool {
    let sea_top = crate::surface_water::SEA_SURFACE_BLOCK_Y;
    if y.saturating_sub(radius) > sea_top {
        return false;
    }
    let wet = |dx: i32, dz: i32| {
        let (px, pz) = (x.saturating_add(dx), z.saturating_add(dz));
        if !biome::is_ocean(world.region.biome(px, pz)) {
            return false;
        }
        world
            .height(px, pz)
            .is_some_and(|height| saltwater_intersects(y, radius, height as i32, sea_top))
    };
    // Find broad shorelines quickly, then check all remaining block columns so
    // narrow inlets and square corners still follow NearFluidPlacement's box.
    if (-radius..=radius)
        .step_by(8)
        .any(|dx| (-radius..=radius).step_by(8).any(|dz| wet(dx, dz)))
    {
        return true;
    }
    (-radius..=radius).any(|dx| (-radius..=radius).any(|dz| wet(dx, dz)))
}

fn saltwater_intersects(origin_y: i32, radius: i32, solid_top: i32, water_top: i32) -> bool {
    origin_y
        .saturating_sub(radius)
        .max(solid_top.saturating_add(1))
        <= origin_y.min(water_top)
}

fn flat_enough_position(world: &World, x: i32, z: i32, top_y: i32, flatness: f32) -> Option<i32> {
    for depth in 0..4 {
        let surface_level = top_y - depth;
        let mut occupied = 0;
        for dz in -2..=2 {
            for dx in -2..=2 {
                if crate::ground::pre_forest_sample(world, x + dx, z + dz)
                    .is_some_and(|ground| ground.y >= surface_level)
                {
                    occupied += 1;
                }
            }
        }
        if occupied as f32 / 25.0 > flatness {
            return Some(top_y - depth);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palm_near_fluid_requires_water_in_the_vertical_search_box() {
        assert!(saltwater_intersects(65, 20, 50, 62));
        assert!(saltwater_intersects(82, 20, 50, 62));
        assert!(
            !saltwater_intersects(83, 20, 50, 62),
            "ocean below a cliff is out of reach"
        );
        assert!(
            !saltwater_intersects(65, 20, 70, 62),
            "dry ocean-biome columns contain no saltwater"
        );
        assert!(
            !saltwater_intersects(50, 20, 50, 62),
            "no fluid below the sea floor"
        );
    }

    #[test]
    fn spreading_crop_child_random_draws_interleave_between_attempts() {
        // Captured from the installed MC 1.20.1 WorldgenRandom fixture using
        // TFC feature seed -3483402354714499433, wheat's successful rarity and
        // in-square draws, then RandomPatchFeature + SpreadingCropFeature calls.
        let seed = crate::vegetation::feature_seed(-100, 26, -64, 9, 45);
        let mut random = FeatureRandom::new(seed);
        assert_eq!(crop_patch_origin(&mut random, 80), Some((0, 2)));
        let mut result = Vec::new();
        for _ in 0..6 {
            let dx = random.next_int_bound(6) - random.next_int_bound(6);
            let dy = random.next_int_bound(2) - random.next_int_bound(2);
            let dz = random.next_int_bound(6) - random.next_int_bound(6);
            let direction = random.next_int_bound(4);
            result.push((dx, dy, dz, direction));
        }
        assert_eq!(
            result,
            [
                (-5, -1, 2, 0),
                (-3, 0, 2, 1),
                (1, 0, -1, 3),
                (-1, 0, -2, 0),
                (3, 1, -2, 2),
                (0, 1, 4, 3),
            ]
        );
    }

    #[test]
    fn every_active_direct_crop_patch_has_a_decoded_child_block() {
        let features = crop_features();
        let simple: Vec<_> = features
            .iter()
            .filter(|feature| {
                !feature.id.ends_with("_patch") || !feature.id.contains("/palm_tree/")
            })
            .collect();
        let palms: Vec<_> = features
            .iter()
            .filter(|feature| feature.id.contains("/palm_tree/") && feature.id.ends_with("_patch"))
            .collect();
        assert_eq!(simple.len(), 37);
        assert_eq!(palms.len(), 2);
        assert_eq!(
            simple.iter().filter(|f| f.id.starts_with("tfg:")).count(),
            14
        );
        for feature in features {
            assert!(
                feature.block.contains(':'),
                "{} has no decoded child block",
                feature.id
            );
            assert!(feature.tries > 0, "{} has no attempts", feature.id);
        }
    }

    #[test]
    fn coconut_and_date_patches_are_loaded_from_installed_feature_data() {
        let features = crop_features();
        for species in ["coconut", "date"] {
            let feature = features
                .iter()
                .find(|feature| feature.id.ends_with(&format!("/{species}_patch")))
                .expect("installed palm fruit patch should be parsed");
            assert_eq!(feature.chance, 160);
            assert_eq!(feature.tries, 3);
            assert_eq!(feature.block, format!("tfg:earth/crop/palm_tree/{species}"));
        }
    }

    #[test]
    fn palm_tree_patch_features_use_large_features_step_eight() {
        for (id, expected_index) in [
            ("tfg:earth/crop/palm_tree/coconut_patch", 39),
            ("tfg:earth/crop/palm_tree/date_patch", 40),
        ] {
            assert_eq!(
                super::super::feature_index_in_step(id, 8),
                Some(expected_index)
            );
            assert_eq!(super::super::feature_index_in_step(id, 9), None);
        }
    }

    #[test]
    fn wheat_patch_positions_match_minecraft_1201_java_fixture() {
        // Captured with the installed 1.20.1 SRG WorldgenRandom and
        // XoroshiroRandomSource classes, using TFC Helpers.seedLargeFeatures:
        // chunk (-100, 26), origin Y -64, step 9, feature index 45.
        let seed = crate::vegetation::feature_seed(-100, 26, -64, 9, 45);
        assert_eq!(seed, -3_483_402_354_714_499_433);
        let mut random = FeatureRandom::new(seed);
        let origin = crop_patch_origin(&mut random, 80);
        assert_eq!(origin, Some((0, 2)));
        let positions = random_patch_offsets(&mut random, 6, 5, 1);
        let expected = [(-5, 2), (1, 1), (-1, 2), (-1, 1), (4, 3), (1, -4)];
        assert_eq!(positions.as_slice(), &expected);
    }
}
