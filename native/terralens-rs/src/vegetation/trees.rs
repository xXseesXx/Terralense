//! TFC ForestFeature seed-driven tree proposals and template silhouettes.
//!
//! Selection, counts, biome-climate filtering, weirdness and configured-tree
//! variant draws follow the installed TFG/TFC data and source. The terrain
//! survival test still uses TerraLens' predicted height/climate surface.

use super::{
    FeatureRandom, SurfacePlant, SurfacePlantKind,
    climate::{ForestNoise, adjusted_average_temp_by_elevation},
    feature_biome_allowed, is_water_biome_id,
    templates::{self, TreeVariant},
};
use crate::{biome, world::World};
use lru::LruCache;
use serde_json::Value;
use std::{
    cell::RefCell,
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};

const FOREST_CONFIG: &str = include_str!("assets/forest_feature.json");
const FEATURE_ORDER: &str = include_str!("assets/feature-order.json");
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
    if !feature_biome_allowed(world, chunk_x, chunk_z, "tfg:earth/forest") {
        return Vec::new();
    }
    let forest = world.region.forest_type(chunk_x, chunk_z).min(4) as usize;
    if forest == 0 {
        return Vec::new();
    }
    let config = configs()[forest];
    let seed = super::feature_seed(chunk_x, chunk_z, ORIGIN_Y, 8, index);
    let mut random = FeatureRandom::new(seed);
    if random.next_float() > config.chance {
        return Vec::new();
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

    for _ in 0..tree_count.max(0) {
        if cancel() {
            break;
        }
        let x = origin_x.wrapping_add(random.next_int_bound(16));
        let z = origin_z.wrapping_add(random.next_int_bound(16));
        let biome_id = world.region.biome(x, z);
        let biome_name = biome::biome_name(biome_id);
        if biome::is_ocean(biome_id) || is_water_biome_id(biome_name) {
            continue;
        }
        let Some(height) = world.height(x, z) else {
            continue;
        };
        let y = height.round() as i32;
        let Some(entry_id) = choose_entry(world, x, z, y, weirdness, use_weirdness(), &mut random)
        else {
            continue;
        };

        let entry = templates::forest_entry(&entry_id);
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
            }
            continue;
        };
        // RandomTreeFeature.place draws root offsets before trunk height. The
        // exact root success/weighted-state draws require saved substrate; draw
        // the position samples here and use the predicted substrate only for the
        // rendered silhouette, with provenance exposing that terrain limit.
        if let Some(roots) = template.root_system.as_ref() {
            if roots.special_placer.is_none() {
                let width = i32::from(roots.width).max(1);
                let height = i32::from(roots.height).max(1);
                // Installed StackedTreeFeature calls roots once in its placement
                // predicate and again before the trunk. Preserve both streams.
                let passes = if template.feature_type == templates::TreeFeatureType::StackedTree {
                    2
                } else {
                    1
                };
                for _ in 0..u32::from(roots.tries) * passes {
                    let _dx = random.next_int_bound(width) - random.next_int_bound(width);
                    let _dz = random.next_int_bound(width) - random.next_int_bound(width);
                    let _dy = random.next_int_bound(height);
                }
            }
        }
        template.finish_trunk(&mut random);
        template.finish_palette(&mut random);
        let species = feature_id.strip_prefix("tfc:tree/").unwrap_or(feature_id);
        for cell in template.cells {
            plants.push(SurfacePlant {
                x: x.saturating_add(cell.x as i32),
                y: y.saturating_add(cell.y as i32),
                z: z.saturating_add(cell.z as i32),
                kind: SurfacePlantKind::Tree,
                variant: species.to_owned(),
            });
        }
    }
    let _ = bush_count; // Bush count is sampled at the exact point in the stream; surface crowns are trees only.
    plants
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
}
