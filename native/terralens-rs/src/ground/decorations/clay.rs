//! TFC's step-2 clay soil discs.
//!
//! The two configured features and their replacement maps come from the pinned
//! TFC 3.2.25 data pack. Placement ordering and RNG follow the installed
//! TFG FeatureSorter profile: clay discs run before erosion, boulders, and the
//! later soil discs.
use super::ClayPatch;
use crate::{
    dimension::Dimension,
    vegetation::{FeatureRandom, feature_biome_allowed, feature_index_in_step, feature_seed},
    world::World,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

const CLAY_DATA: [&str; 2] = [
    include_str!("assets/clay-disc.json"),
    include_str!("assets/water-clay-disc.json"),
];
const CLAY_FEATURE: &str = "tfc:clay_disc_with_indicator";
const WATER_CLAY_FEATURE: &str = "tfc:water_clay_disc_with_indicator";
const DECORATION_STEP: i32 = 2;
const ORIGIN_Y: i32 = -64;

#[derive(Clone)]
struct Config {
    id: &'static str,
    rarity: i32,
    min_radius: i32,
    max_radius_exclusive: i32,
    height: i32,
    near_water: bool,
    states: Arc<HashMap<String, Arc<str>>>,
}

fn configurations() -> &'static [Config] {
    static CONFIGS: OnceLock<Vec<Config>> = OnceLock::new();
    CONFIGS.get_or_init(|| {
        let definitions = [(CLAY_FEATURE, 20, false), (WATER_CLAY_FEATURE, 10, true)];
        definitions
            .into_iter()
            .zip(CLAY_DATA)
            .map(|((id, rarity, near_water), raw)| {
                let root: Value = serde_json::from_str(raw)
                    .expect("bundled TFC clay-disc data must be valid JSON");
                let config = &root["config"];
                let mut states = HashMap::new();
                for state in config["states"].as_array().expect("clay state pairs") {
                    // SoilDiscConfig maps the source block named by `replace`
                    // to the destination block named by `with`.
                    states.insert(
                        state["replace"]
                            .as_str()
                            .expect("replacement block")
                            .to_owned(),
                        Arc::<str>::from(state["with"].as_str().expect("clay block")),
                    );
                }
                let min_radius = config["min_radius"].as_i64().expect("minimum radius") as i32;
                let max_radius = config["max_radius"].as_i64().expect("maximum radius") as i32;
                Config {
                    id,
                    rarity,
                    min_radius,
                    max_radius_exclusive: if max_radius > min_radius {
                        max_radius
                    } else {
                        max_radius.saturating_add(1)
                    },
                    height: config["height"].as_i64().expect("disc height") as i32,
                    near_water,
                    states: Arc::new(states),
                }
            })
            .collect()
    })
}

/// Return the placed clay discs originating in one worldgen chunk.
pub(super) fn generate(
    world: &World,
    cx: i32,
    cz: i32,
    cancel: &impl Fn() -> bool,
) -> Option<Vec<ClayPatch>> {
    let mut patches = Vec::new();
    if world.dimension != Dimension::Overworld {
        return Some(patches);
    }
    let chunk_x = cx.wrapping_mul(16);
    let chunk_z = cz.wrapping_mul(16);
    for config in configurations() {
        if cancel() {
            return None;
        }
        let Some(index) = feature_index_in_step(config.id, DECORATION_STEP) else {
            continue;
        };
        if !feature_biome_allowed(world, cx, cz, config.id) {
            continue;
        }
        let mut random = FeatureRandom::new(feature_seed(cx, cz, ORIGIN_Y, DECORATION_STEP, index));
        if random.next_int_bound(config.rarity) != 0 {
            continue;
        }

        // Minecraft's InSquare placement follows the rarity filter.
        let x = chunk_x.wrapping_add(random.next_int_bound(16));
        let z = chunk_z.wrapping_add(random.next_int_bound(16));
        let Some(y) = worldgen_surface_first_air(world, x, z) else {
            continue;
        };

        // The normal feature's climate modifier runs last, after rarity,
        // in-square, and heightmap placement. The source config has only the
        // minimum rainfall condition.
        if config.near_water {
            if !near_water(world, x, y, z) {
                continue;
            }
        } else if (world.climate(x, z).rainfall as f32) < 175.0_f32 {
            continue;
        }

        let radius = config.min_radius
            + random.next_int_bound(config.max_radius_exclusive - config.min_radius);
        // SoilDiscFeature consumes one nextFloat per in-circle voxel, even with
        // integrity=1. Keep the source's x -> z -> y order for deterministic
        // placement RNG accounting and future feature-modifier parity.
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx * dx + dz * dz > radius * radius {
                    continue;
                }
                for _dy in -config.height..=config.height {
                    if cancel() {
                        return None;
                    }
                    let _ = random.next_float();
                }
            }
        }
        patches.push(ClayPatch {
            index,
            x,
            y,
            z,
            radius,
            height: config.height,
        });
    }
    patches.sort_by_key(|patch| patch.index);
    Some(patches)
}

/// WORLD_SURFACE_WG is measured before the later AIR-carver step. The cave
/// sampler retains that builder cap alongside its carved physical surface.
fn worldgen_surface_first_air(world: &World, x: i32, z: i32) -> Option<i32> {
    crate::caves::column(world, x, z)
        .map(|column| {
            let top = column
                .fluid
                .map_or(column.surface_builder_y, |(_, fluid_y)| {
                    column.surface_builder_y.max(fluid_y)
                });
            top.saturating_add(1)
        })
        .or_else(|| {
            crate::surface_water::ordinary_solid_top(world.height(x, z)?)
                .map(|surface_y| surface_y.saturating_add(1))
        })
}

fn near_water(world: &World, x: i32, y: i32, z: i32) -> bool {
    for dx in -4..=4 {
        for dz in -4..=4 {
            let Some(water_y) = crate::caves::column(world, x + dx, z + dz)
                .and_then(|column| column.fluid)
                .and_then(|(cell, water_y)| (cell == crate::caves::Cell::Water).then_some(water_y))
            else {
                continue;
            };
            if (y - 4..=y).contains(&water_y) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Settings, vegetation::feature_index_in_step};

    #[test]
    fn configured_clay_discs_match_the_pinned_tfc_datapack() {
        let config = configurations();
        assert_eq!(config.len(), 2);
        assert_eq!(
            (
                config[0].rarity,
                config[0].min_radius,
                config[0].max_radius_exclusive,
                config[0].height
            ),
            (20, 3, 5, 3)
        );
        assert_eq!(
            (
                config[1].rarity,
                config[1].min_radius,
                config[1].max_radius_exclusive,
                config[1].height
            ),
            (10, 2, 3, 2)
        );
        assert!(!config[0].near_water);
        assert!(config[1].near_water);
        assert_eq!(
            config[0].states["tfc:grass/sandy_loam"].as_ref(),
            "tfc:clay_grass/sandy_loam"
        );
        assert_eq!(
            config[1].states["tfg:grass/oxisol"].as_ref(),
            "tfg:clay_grass/oxisol"
        );
        assert_eq!(
            config[1].states["tfc:grass/loam"].as_ref(),
            "tfc:clay_grass/loam"
        );
    }

    #[test]
    fn clay_features_use_the_stage_two_feature_sorter_indices() {
        assert_eq!(feature_index_in_step(CLAY_FEATURE, 2), Some(8));
        assert_eq!(feature_index_in_step(WATER_CLAY_FEATURE, 2), Some(9));
    }

    #[test]
    fn known_world0_sandy_loam_clay_origin_is_recreated_from_the_feature_seed() {
        let world = World::new(0, Settings::default());
        let source_chunk = (-7032, -972);
        let patches = generate(&world, source_chunk.0, source_chunk.1, &|| false).unwrap();
        let patch = patches
            .iter()
            .find(|patch| patch.x == -112505 && patch.z == -15552 && patch.index == 8)
            .expect("the source-feature trace places clay at the audit coordinate");
        assert_eq!(patch.y, 103);
        assert_eq!((patch.radius, patch.height), (3, 3));
    }
}
