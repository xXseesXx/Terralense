//! TFG soil-disc surface decorations (duff, alfisol, and podzol).
//!
//! This reproduces the five effective placed-feature configurations from TFG Core
//! 0.9.25. Patch application is deliberately deferred to the caller so overlays can
//! be clipped against whichever render tile is being built.
use super::SoilPatch;
use crate::{
    dimension::Dimension,
    ground::base_sample,
    vegetation::{FeatureRandom, feature_biome_allowed, feature_index_in_step, feature_seed},
    world::World,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

const SOIL_DATA: &str = include_str!("assets/soils.json");

#[derive(Clone)]
struct SoilFeature {
    id: String,
    index: u32,
    count: i32,
    min_forest: u8,
    max_forest: u8,
    min_temperature: f32,
    max_temperature: f32,
    min_rainfall: f32,
    max_rainfall: f32,
    fuzzy: bool,
    min_radius: i32,
    max_radius_exclusive: i32,
    height: i32,
    states: Arc<HashMap<String, Arc<str>>>,
}

fn features() -> &'static [SoilFeature] {
    static FEATURES: OnceLock<Vec<SoilFeature>> = OnceLock::new();
    FEATURES.get_or_init(|| {
        let root: Value = serde_json::from_str(SOIL_DATA)
            .expect("bundled TFG soil-decoration data must be valid JSON");
        root["features"]
            .as_array()
            .expect("soil features array")
            .iter()
            .map(|entry| {
                let states = entry["states"]
                    .as_object()
                    .expect("soil state map")
                    .iter()
                    .map(|(from, to)| {
                        (
                            from.clone(),
                            Arc::<str>::from(to.as_str().expect("block ID")),
                        )
                    })
                    .collect();
                SoilFeature {
                    id: entry["id"].as_str().expect("feature ID").to_owned(),
                    index: entry["index"].as_u64().expect("feature order index") as u32,
                    count: entry["count"].as_i64().expect("placement count") as i32,
                    min_forest: forest_level(entry["min_forest"].as_str().unwrap_or("none")),
                    max_forest: forest_level(entry["max_forest"].as_str().unwrap_or("old_growth")),
                    min_temperature: entry["min_temperature"].as_f64().unwrap_or(-f64::MAX) as f32,
                    max_temperature: entry["max_temperature"].as_f64().unwrap_or(f64::MAX) as f32,
                    min_rainfall: entry["min_rainfall"].as_f64().unwrap_or(-f64::MAX) as f32,
                    max_rainfall: entry["max_rainfall"].as_f64().unwrap_or(f64::MAX) as f32,
                    fuzzy: entry["fuzzy"].as_bool().unwrap_or(false),
                    min_radius: entry["min_radius"].as_i64().expect("minimum radius") as i32,
                    max_radius_exclusive: entry["max_radius_exclusive"]
                        .as_i64()
                        .expect("exclusive maximum radius")
                        as i32,
                    height: entry["height"].as_i64().expect("disc height") as i32,
                    states: Arc::new(states),
                }
            })
            .collect()
    })
}

fn forest_level(name: &str) -> u8 {
    match name {
        "sparse" => 1,
        "edge" => 2,
        "normal" => 3,
        "old_growth" => 4,
        _ => 0,
    }
}

/// Generate effective TFG soil discs whose placement chunks are `(cx, cz)`.
///
/// Feature positions use the WORLD_SURFACE_WG first-air level from the base terrain
/// sampler. This matches terrain without earlier stage-8 tree canopies or saved blocks;
/// the final block replacement remains conditional on the caller's current surface
/// state being one of the exact configured `replace` states.
pub(super) fn generate(
    world: &World,
    cx: i32,
    cz: i32,
    cancel: &impl Fn() -> bool,
) -> Option<Vec<SoilPatch>> {
    let mut patches = Vec::new();
    if world.dimension != Dimension::Overworld {
        return Some(patches);
    }
    let chunk_x = cx.wrapping_mul(16);
    let chunk_z = cz.wrapping_mul(16);

    for feature in features() {
        if cancel() {
            return None;
        }
        if feature_index_in_step(&feature.id, 8) != Some(feature.index)
            || !feature_biome_allowed(world, cx, cz, &feature.id)
        {
            continue;
        }
        let mut random = FeatureRandom::new(feature_seed(cx, cz, -64, 8, feature.index));
        if !climate_allowed(world, cx, cz, feature, &mut random) {
            continue;
        }

        // CountPlacement precedes InSquarePlacement and HeightmapPlacement, so each
        // origin's two offsets and disc's feature RNG are consumed in sequence.
        for _ in 0..feature.count {
            if cancel() {
                return None;
            }
            let x = chunk_x.wrapping_add(random.next_int_bound(16));
            let z = chunk_z.wrapping_add(random.next_int_bound(16));
            let Some(base) = base_sample(world, x, z) else {
                continue;
            };
            // WORLD_SURFACE_WG is first air, one above the highest solid block.
            let y = base.y.saturating_add(1);
            let radius = feature.min_radius
                + random.next_int_bound(feature.max_radius_exclusive - feature.min_radius);

            // SoilDiscFeature calls nextFloat for every in-circle position at every
            // y, even though all current configs have integrity=1. These draws are
            // important because the placement RNG continues into the next origin.
            let radius_sq = radius * radius;
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    if dx * dx + dz * dz > radius_sq {
                        continue;
                    }
                    for _dy in -feature.height..=feature.height {
                        if cancel() {
                            return None;
                        }
                        let _ = random.next_float();
                    }
                }
            }
            patches.push(SoilPatch {
                index: feature.index,
                x,
                y,
                z,
                radius,
                height: feature.height,
                states: Arc::clone(&feature.states),
            });
        }
    }
    Some(patches)
}

fn climate_allowed(
    world: &World,
    cx: i32,
    cz: i32,
    feature: &SoilFeature,
    random: &mut FeatureRandom,
) -> bool {
    let forest = world.region.forest_type(cx, cz);
    if forest < feature.min_forest || forest > feature.max_forest {
        return false;
    }
    let pos_x = cx.wrapping_mul(16);
    let pos_z = cz.wrapping_mul(16);
    let climate = world.climate(pos_x, pos_z);
    // ClimatePlacement receives the chunk's initial origin before in-square and
    // heightmap modifiers, at y=0. TFC leaves average temperature unchanged below
    // sea level. Keep comparisons in f32 like the Java implementation.
    let temperature = climate.temperature as f32;
    let rainfall = climate.rainfall as f32;
    if !(feature.min_temperature <= temperature
        && temperature <= feature.max_temperature
        && feature.min_rainfall <= rainfall
        && rainfall <= feature.max_rainfall)
    {
        return false;
    }
    if feature.fuzzy {
        let target_temperature = (feature.min_temperature + feature.max_temperature) / 2.0;
        let target_rainfall = (feature.min_rainfall + feature.max_rainfall) / 2.0;
        let norm_temp = (temperature - target_temperature).abs()
            / (feature.max_temperature - feature.min_temperature);
        let norm_rain =
            (rainfall - target_rainfall).abs() / (feature.max_rainfall - feature.min_rainfall);
        return random.next_float() * random.next_float() > norm_temp.max(norm_rain);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{features, trace_placement_rng};
    use crate::vegetation::feature_seed;

    #[test]
    fn source_profile_keeps_effective_old_growth_link_and_block_replacements() {
        let loaded = features();
        assert_eq!(loaded.len(), 5);
        let old_growth = loaded
            .iter()
            .find(|f| f.id.ends_with("duff_disc_old_growth"))
            .unwrap();
        assert_eq!(old_growth.index, 35);
        assert_eq!(old_growth.count, 7);
        assert_eq!(
            (old_growth.min_radius, old_growth.max_radius_exclusive),
            (3, 5)
        );
        assert_eq!(old_growth.height, 7);
        assert_eq!(old_growth.min_forest, 4);
        assert_eq!(
            old_growth.states["tfc:grass/loam"].as_ref(),
            "tfg:duff/loam"
        );

        let alfisol = loaded
            .iter()
            .find(|f| f.id.ends_with("alfisol_disc"))
            .unwrap();
        assert_eq!(alfisol.states.len(), 32);
        assert_eq!(
            alfisol.states["tfc:grass/loam"].as_ref(),
            "tfg:grass/alfisol"
        );
    }

    #[test]
    fn stage_eight_soil_rng_matches_java_traces() {
        // Standalone Java 17 trace using Minecraft 1.20.1 XoroshiroRandomSource
        // upgrade/next, WorldgenRandom's legacy bounded-int methods, TFG's
        // Helpers.seedLargeFeatures formula, and SoilDiscFeature's x/z/y loops.
        assert_eq!(
            feature_seed(-70, -100, -64, 8, 34),
            7_425_749_133_626_424_155
        );
        assert_eq!(
            feature_seed(-70, -100, -64, 8, 36),
            3_312_910_429_259_700_933
        );
        assert_eq!(
            feature_seed(-70, -100, -64, 8, 37),
            1_102_273_561_799_052_552
        );
        let duff = trace_placement_rng(7_425_749_133_626_424_155, -70, -100, 5, 3, 5, false);
        assert_eq!(
            duff.0,
            vec![
                (-1120, -1597, 3, 435),
                (-1115, -1588, 3, 435),
                (-1105, -1593, 3, 435),
                (-1116, -1600, 4, 735),
                (-1120, -1593, 3, 435)
            ]
        );
        assert_eq!(duff.1, 6_061_549);

        // Center-of-range alfisol/podzol climates give fuzzy threshold zero, so
        // each pair of Java nextFloat draws must accept and precede placement.
        let alfisol = trace_placement_rng(3_312_910_429_259_700_933, -70, -100, 3, 7, 14, true);
        assert_eq!(
            alfisol.0,
            vec![
                (-1119, -1593, 12, 6615),
                (-1106, -1589, 9, 3795),
                (-1117, -1592, 13, 7935)
            ]
        );
        assert_eq!(alfisol.1, 11_404_575);
        let podzol = trace_placement_rng(1_102_273_561_799_052_552, -70, -100, 3, 7, 14, true);
        assert_eq!(
            podzol.0,
            vec![
                (-1120, -1588, 7, 2235),
                (-1106, -1591, 9, 3795),
                (-1116, -1591, 9, 3795)
            ]
        );
        assert_eq!(podzol.1, 4_525_358);
    }
}

#[cfg(test)]
fn trace_placement_rng(
    seed: i64,
    cx: i32,
    cz: i32,
    count: i32,
    min_radius: i32,
    max_radius: i32,
    fuzzy_center: bool,
) -> (Vec<(i32, i32, i32, i32)>, i32) {
    let mut random = crate::vegetation::FeatureRandom::new(seed);
    if fuzzy_center {
        let climate_probability = random.next_float() * random.next_float();
        assert!(climate_probability > 0.0);
    }
    let mut origins = Vec::new();
    for _ in 0..count {
        let x = cx * 16 + random.next_int_bound(16);
        let z = cz * 16 + random.next_int_bound(16);
        let radius = min_radius + random.next_int_bound(max_radius - min_radius);
        let mut draws = 0;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx * dx + dz * dz <= radius * radius {
                    for _ in -7..=7 {
                        let _ = random.next_float();
                        draws += 1;
                    }
                }
            }
        }
        origins.push((x, z, radius, draws));
    }
    (origins, random.next(24))
}
