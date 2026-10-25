//! Score the seed-only sampler against a deterministic spread of saved World 0 chunks.
//!
//! This example deliberately opens no save: it consumes a compact, independently
//! decoded fixture and instantiates the seed sampler without a saved-world source.
//! Run with `cargo run --release --features diagnostics --example accuracy_survey`.

use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Read,
};
use terralens::{
    ground,
    model::Settings,
    vegetation::{self, SurfacePlantKind},
    volcanic_lava::Surface,
    world::World,
};

const FIXTURE: &[u8] =
    include_bytes!("../tests/fixtures/generator-accuracy/world0-spread-48.json.gz");
const FOREST_FEATURE_INDEX: u32 = 25;

fn main() {
    let mut decoder = GzDecoder::new(FIXTURE);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes).expect("read fixture gzip");
    let fixture: Value = serde_json::from_slice(&bytes).expect("parse fixture JSON");
    let world = World::new(0, Settings::default());

    let mut biome = Counts::default();
    let mut forest_type = Counts::default();
    let mut height_errors = Vec::new();
    let mut height_integer_exact = 0usize;
    let mut height_missing = 0usize;
    let mut temperature_errors = Vec::new();
    let mut rainfall_errors = Vec::new();
    let mut climate_tolerance = Counts::default();
    let mut ground = Counts::default();
    let mut snow_ice_excluded = 0usize;
    let mut visible_snow_ice_tops = 0usize;
    let mut sandstone = Counts::default();
    let mut material_confusions = BTreeMap::<String, usize>::new();
    let mut material_examples_by_pair = BTreeMap::<String, Vec<Value>>::new();
    let mut material_mismatch_examples = Vec::new();
    let mut fluid = Confusion::default();
    let mut fluid_mismatch_examples = Vec::new();
    let mut actual_leaves = HashSet::<(i32, i32)>::new();
    let mut target_columns = HashSet::<(i32, i32)>::new();
    let mut source_chunks = BTreeSet::<(i32, i32)>::new();
    let mut per_chunk = Vec::new();
    let chunks = fixture["chunks"].as_array().expect("fixture chunks");

    for chunk in chunks {
        let pair = chunk["chunk"].as_array().expect("chunk coordinates");
        let cx = pair[0].as_i64().unwrap() as i32;
        let cz = pair[1].as_i64().unwrap() as i32;
        let observed_forest = chunk["forest_type"].as_i64().unwrap() as u8;
        let predicted_forest = world.region.forest_type(cx, cz);
        forest_type.add(observed_forest == predicted_forest);
        let corners_t = read_corners(&chunk["temperature_corners"]);
        let corners_r = read_corners(&chunk["rainfall_corners"]);
        let mut site_biome = Counts::default();
        let mut site_climate = Counts::default();
        let mut site_ground = Counts::default();
        let mut site_fluid = Confusion::default();
        let mut site_material_confusions = BTreeMap::<String, usize>::new();
        let mut site_height_errors = Vec::new();
        let mut site_height_integer_exact = 0usize;
        let mut site_height_missing = 0usize;
        let mut site_height_mismatches = Vec::new();

        for row in chunk["columns"].as_array().expect("chunk columns") {
            let x = row["x"].as_i64().unwrap() as i32;
            let z = row["z"].as_i64().unwrap() as i32;
            target_columns.insert((x, z));

            let saved_biome = row["surface_biome"].as_str().unwrap_or("");
            let predicted_biome = world.biome(x, z);
            biome.add(saved_biome == predicted_biome);
            site_biome.add(saved_biome == predicted_biome);

            let saved_height = row["terrain_noise_height"].as_i64().unwrap() as i32;
            if let Some(predicted_height) = world.height(x, z) {
                height_errors.push((predicted_height - f64::from(saved_height)).abs());
                let error = (predicted_height - f64::from(saved_height)).abs();
                let exact = predicted_height.trunc() as i32 == saved_height;
                height_integer_exact += usize::from(exact);
                site_height_errors.push(error);
                site_height_integer_exact += usize::from(exact);
                if !exact {
                    site_height_mismatches.push(json!({
                        "x": x, "z": z,
                        "saved_noise_height": saved_height,
                        "predicted_raw_height": predicted_height,
                        "predicted_integer_height": predicted_height.trunc() as i32,
                        "surface_biome": saved_biome,
                    }));
                }
            } else {
                height_missing += 1;
                site_height_missing += 1;
            }

            let climate = world.climate(x, z);
            let dx = f64::from(((x & 15) as f32) / 16.0_f32);
            let dz = f64::from(((z & 15) as f32) / 16.0_f32);
            let expected_t = java_lerp4(corners_t, dx, dz) as f32;
            let expected_r = java_lerp4(corners_r, dx, dz) as f32;
            let t_error = (climate.temperature as f32 - expected_t).abs() as f64;
            let r_error = (climate.rainfall as f32 - expected_r).abs() as f64;
            temperature_errors.push(t_error);
            rainfall_errors.push(r_error);
            climate_tolerance.add(t_error <= 0.01 && r_error <= 0.1);
            site_climate.add(t_error <= 0.01 && r_error <= 0.1);

            let actual_block = row["ground_block"].as_str().unwrap_or("");
            let actual_top = row["surface_block"].as_str().unwrap_or("");
            let actual_fluid = fluid_kind(actual_block);
            let predicted_surface = world
                .volcanic_surface(x, z)
                .or_else(|| world.is_surface_water(x, z).then_some(Surface::Water(62)));
            let (predicted_block, predicted_fluid) = match predicted_surface {
                Some(surface) => (None, Some(surface_kind(surface).to_owned())),
                None => {
                    let sample = ground::sample(&world, x, z);
                    let block = sample.as_ref().map(|sample| sample.block.to_string());
                    let predicted_fluid = block.as_deref().and_then(fluid_kind).map(str::to_owned);
                    (block, predicted_fluid)
                }
            };
            if !is_snow_or_ice(actual_block) {
                fluid.add(
                    actual_fluid.unwrap_or("solid"),
                    predicted_fluid.as_deref().unwrap_or("solid"),
                );
                site_fluid.add(
                    actual_fluid.unwrap_or("solid"),
                    predicted_fluid.as_deref().unwrap_or("solid"),
                );
                if actual_fluid.unwrap_or("solid") != predicted_fluid.as_deref().unwrap_or("solid")
                    && fluid_mismatch_examples.len() < 48
                {
                    fluid_mismatch_examples.push(json!({
                        "x": x, "z": z,
                        "actual_ground_y": row["ground_y"],
                        "actual_block": actual_block,
                        "actual_class": actual_fluid.unwrap_or("solid"),
                        "predicted_class": predicted_fluid.as_deref().unwrap_or("solid"),
                        "predicted_surface_y": predicted_surface.map(Surface::y),
                        "surface_biome": saved_biome,
                    }));
                }
            }
            let material_match = if is_snow_or_ice(actual_block) {
                false
            } else {
                match (actual_fluid, predicted_fluid) {
                    (Some(a), Some(p)) => a == p,
                    (None, None) => {
                        let predicted = predicted_block.as_deref().unwrap_or("");
                        *material_confusions
                            .entry(format!("{actual_block} -> {predicted}"))
                            .or_default() += usize::from(actual_block != predicted);
                        *site_material_confusions
                            .entry(format!("{actual_block} -> {predicted}"))
                            .or_default() += usize::from(actual_block != predicted);
                        if actual_block != predicted {
                            record_material_example(
                                &mut material_examples_by_pair,
                                actual_block,
                                predicted,
                                x,
                                z,
                                row["ground_y"].as_i64().unwrap() as i32,
                                world.height(x, z),
                                saved_biome,
                            );
                        }
                        if actual_block != predicted && material_mismatch_examples.len() < 64 {
                            material_mismatch_examples.push(json!({
                                "x": x, "z": z,
                                "actual_ground_y": row["ground_y"],
                                "actual_block": actual_block,
                                "predicted_block": predicted,
                                "estimated_raw_height": world.height(x, z),
                                "surface_biome": saved_biome,
                            }));
                        }
                        actual_block == predicted
                    }
                    (_actual, predicted) => {
                        let predicted_name = predicted_block
                            .as_deref()
                            .unwrap_or_else(|| predicted.as_deref().unwrap_or("none"));
                        *material_confusions
                            .entry(format!("{actual_block} -> {predicted_name}"))
                            .or_default() += 1;
                        *site_material_confusions
                            .entry(format!("{actual_block} -> {predicted_name}"))
                            .or_default() += 1;
                        record_material_example(
                            &mut material_examples_by_pair,
                            actual_block,
                            predicted_name,
                            x,
                            z,
                            row["ground_y"].as_i64().unwrap() as i32,
                            world.height(x, z),
                            saved_biome,
                        );
                        if material_mismatch_examples.len() < 64 {
                            material_mismatch_examples.push(json!({
                                "x": x, "z": z,
                                "actual_ground_y": row["ground_y"],
                                "actual_block": actual_block,
                                "predicted_block": predicted_name,
                                "estimated_raw_height": world.height(x, z),
                                "surface_biome": saved_biome,
                            }));
                        }
                        false
                    }
                }
            };
            // The saved ground substrate remains scorable beneath a thin snow
            // cover. Only a frozen substrate is excluded, never counted as a pass.
            if is_snow_or_ice(actual_block) {
                snow_ice_excluded += 1;
            } else {
                ground.add(material_match);
                site_ground.add(material_match);
                if actual_block.to_ascii_lowercase().contains("sandstone") {
                    sandstone.add(actual_block == predicted_block.as_deref().unwrap_or(""));
                }
            }
            visible_snow_ice_tops += usize::from(is_snow_or_ice(actual_top));

            if row["has_leaf_block"].as_bool().unwrap_or(false) {
                actual_leaves.insert((x, z));
            }
            let section = (cx, cz);
            for dz in -2..=2 {
                for dx in -2..=2 {
                    source_chunks.insert((section.0 + dx, section.1 + dz));
                }
            }
        }
        per_chunk.push(json!({
            "chunk": [cx, cz],
            "surface_biome_quart": count_json(site_biome),
            "tfc_climate_tolerance": count_json(site_climate),
            "ground_material_or_fluid": count_json(site_ground),
            "material_mismatches": top_pairs(&site_material_confusions),
            "fluid_class": site_fluid.json(),
            "height": {
                "samples": site_height_errors.len() + site_height_missing,
                "compared": site_height_errors.len(),
                "missing_predictions": site_height_missing,
                "integer_exact": site_height_integer_exact,
                "integer_accuracy": ratio(site_height_integer_exact, site_height_errors.len() + site_height_missing),
                "compared_integer_accuracy": ratio(site_height_integer_exact, site_height_errors.len()),
                "mae_blocks": mean(&site_height_errors),
                "max_abs_error": site_height_errors.iter().copied().fold(0.0_f64, f64::max),
                "mismatches": site_height_mismatches,
            },
        }));
    }

    let mut predicted_leaves = HashSet::new();
    for (cx, cz) in source_chunks.iter().copied() {
        for plant in vegetation::tree_source_chunk_with_feature_index(
            &world,
            cx,
            cz,
            FOREST_FEATURE_INDEX,
            &|| false,
        ) {
            if plant.kind == SurfacePlantKind::Tree && target_columns.contains(&(plant.x, plant.z))
            {
                predicted_leaves.insert((plant.x, plant.z));
            }
        }
    }
    let tp = predicted_leaves.intersection(&actual_leaves).count();
    let fp = predicted_leaves.difference(&actual_leaves).count();
    let fn_ = actual_leaves.difference(&predicted_leaves).count();
    let precision = ratio(tp, tp + fp);
    let recall = ratio(tp, tp + fn_);
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    let material_top = top_pairs(&material_confusions);
    let common_pair_examples = material_top
        .iter()
        .filter_map(|row| {
            let pair = row["actual_to_predicted"].as_str()?;
            Some(json!({"pair": pair, "examples": material_examples_by_pair.get(pair)}))
        })
        .collect::<Vec<_>>();
    let region_hashes = fixture["provenance"]["region_sha256"].clone();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "seed": 0,
            "fixture": "world0-spread-48.json.gz",
            "read_only_reference_regions": region_hashes,
            "reference_provenance": fixture["provenance"],
            "chunks": chunks.len(),
            "columns": target_columns.len(),
            "per_chunk": per_chunk,
            "systems": {
                "surface_biome_quart": count_json(biome),
                "forest_type_chunk": count_json(forest_type),
                "tfc_surface_height_noise": {
                    "columns": target_columns.len(),
                    "compared": height_errors.len(),
                    "missing_predictions": height_missing,
                    "integer_exact": height_integer_exact,
                    "integer_accuracy": ratio(height_integer_exact, target_columns.len()),
                    "compared_integer_accuracy": ratio(height_integer_exact, height_errors.len()),
                    "mae_blocks": mean(&height_errors),
                    "median_absolute_error": percentile(&height_errors, 0.50),
                    "p95_absolute_error": percentile(&height_errors, 0.95),
                    "max_absolute_error": height_errors.iter().copied().fold(0.0_f64, f64::max),
                },
                "tfc_climate": {
                    "columns": temperature_errors.len(),
                    "within_0_01_c_and_0_1_mm": count_json(climate_tolerance),
                    "temperature_mae_c": mean(&temperature_errors),
                    "temperature_p95_abs_error_c": percentile(&temperature_errors, 0.95),
                    "rainfall_mae_mm": mean(&rainfall_errors),
                    "rainfall_p95_abs_error_mm": percentile(&rainfall_errors, 0.95),
                },
                "surface_material_exact_or_fluid_class": {
                    "columns": ground.total,
                    "matches": ground.correct,
                    "accuracy": ratio(ground.correct, ground.total),
                    "snow_ice_excluded_columns": snow_ice_excluded,
                    "visible_snow_ice_top_columns_informational": visible_snow_ice_tops,
                    "most_common_mismatches": material_top,
                    "common_pair_examples": common_pair_examples,
                    "mismatch_examples": material_mismatch_examples,
                },
                "sandstone_variant_exact": count_json(sandstone),
                "surface_fluid_class": {
                    "summary": fluid.json(),
                    "mismatch_examples": fluid_mismatch_examples,
                },
                "leaf_canopy_columns": {
                    "metric": "2D leaf-column coverage; not per-tree origins/species/height",
                    "saved_leaf_columns": actual_leaves.len(),
                    "predicted_leaf_columns": predicted_leaves.len(),
                    "tp": tp, "fp": fp, "fn": fn_,
                    "precision": precision, "recall": recall, "f1": f1,
                    "feature_index": FOREST_FEATURE_INDEX,
                    "source_chunks_evaluated": source_chunks.len(),
                },
            },
            "limitations": [
                "The sample ranks already allocated save chunks, so it measures observed generated terrain only and inherits where the player explored.",
                "Biome is scored at the saved highest-block quart sample; season-dependent climate tint is not part of this climate metric.",
                "Ground requires exact block IDs for solids and exact class for fluids; overlapping player edits are not distinguished from generation.",
                "Leaf coverage includes foliage written by every mod and is not the exact placed-tree set.",
                "Scores are diagnostics by system and denominator; they must not be pooled into one world-wide error rate.",
            ],
        }))
        .unwrap()
    );
}

#[derive(Clone, Copy, Default)]
struct Counts {
    correct: usize,
    total: usize,
}

impl Counts {
    fn add(&mut self, matches: bool) {
        self.total += 1;
        self.correct += usize::from(matches);
    }
}

#[derive(Default)]
struct Confusion {
    counts: BTreeMap<(String, String), usize>,
}

impl Confusion {
    fn add(&mut self, actual: &str, predicted: &str) {
        *self
            .counts
            .entry((actual.to_owned(), predicted.to_owned()))
            .or_default() += 1;
    }
    fn json(&self) -> Value {
        let total: usize = self.counts.values().sum();
        let correct: usize = self
            .counts
            .iter()
            .filter(|((actual, predicted), _)| actual == predicted)
            .map(|(_, count)| count)
            .sum();
        let confusion = self
            .counts
            .iter()
            .map(|((actual, predicted), count)| {
                json!({"actual": actual, "predicted": predicted, "columns": count})
            })
            .collect::<Vec<_>>();
        json!({"columns": total, "matches": correct, "accuracy": ratio(correct, total), "confusion": confusion})
    }
}

fn read_corners(value: &Value) -> [f64; 4] {
    ["00", "01", "10", "11"].map(|key| value[key].as_f64().unwrap())
}

/// Java Helpers.lerp4(float,float,float,float,float,float) after the persisted
/// four-corner fields have been promoted to doubles by TFC's LerpFloatLayer.
fn java_lerp4(corners: [f64; 4], dx: f64, dz: f64) -> f64 {
    let z0 = corners[0] + dz * (corners[1] - corners[0]);
    let z1 = corners[2] + dz * (corners[3] - corners[2]);
    z0 + dx * (z1 - z0)
}

fn fluid_kind(block: &str) -> Option<&'static str> {
    let lower = block.to_ascii_lowercase();
    if lower.contains("lava") {
        Some("lava")
    } else if lower.contains("magma") {
        Some("magma")
    } else if lower.contains("water") {
        Some("water")
    } else {
        None
    }
}

fn is_snow_or_ice(block: &str) -> bool {
    let path = block.split_once(':').map_or(block, |(_, path)| path);
    path == "snow"
        || path == "snow_block"
        || path == "ice"
        || path == "packed_ice"
        || path == "blue_ice"
        || path == "sea_ice"
        || path.ends_with("/snow")
        || path.ends_with("/ice")
}

fn surface_kind(surface: Surface) -> &'static str {
    match surface {
        Surface::Water(_) => "water",
        Surface::Lava(_) => "lava",
        Surface::Magma(_) => "magma",
    }
}

fn count_json(counts: Counts) -> Value {
    json!({"matches": counts.correct, "samples": counts.total, "accuracy": ratio(counts.correct, counts.total)})
}

fn top_pairs(values: &BTreeMap<String, usize>) -> Vec<Value> {
    let mut pairs = values
        .iter()
        .filter(|(_, count)| **count > 0)
        .map(|(pair, count)| json!({"actual_to_predicted": pair, "columns": count}))
        .collect::<Vec<_>>();
    pairs.sort_by_key(|row| std::cmp::Reverse(row["columns"].as_u64().unwrap_or_default()));
    pairs.truncate(12);
    pairs
}

fn record_material_example(
    examples: &mut BTreeMap<String, Vec<Value>>,
    actual: &str,
    predicted: &str,
    x: i32,
    z: i32,
    ground_y: i32,
    estimated_height: Option<f64>,
    biome: &str,
) {
    let pair = format!("{actual} -> {predicted}");
    let rows = examples.entry(pair).or_default();
    if rows.len() < 4 {
        rows.push(json!({
            "x": x, "z": z,
            "actual_ground_y": ground_y,
            "estimated_raw_height": estimated_height,
            "surface_biome": biome,
        }));
    }
}

fn ratio(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() as f64 * p).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}
