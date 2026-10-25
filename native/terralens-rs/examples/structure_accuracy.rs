//! Read-only comparison of raw structure predictions with completed saved chunks.
//!
//! Run with `cargo run --release --example structure_accuracy -- WORLD_OR_LEVEL_DAT`.
//! The JSON report is written to stdout; progress and failures go to stderr.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;
use terralens::{dimension::Dimension, features, model::Bounds, prism, world::World};

const ERROR_TARGET: f64 = 0.05;

type Start = (String, i32, i32);
type Stamp = (u64, String);

fn stamps(dir: &Path) -> Result<BTreeMap<String, Stamp>, Box<dyn Error>> {
    let mut out = BTreeMap::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("mca") {
            continue;
        }
        let metadata = path.metadata()?;
        let modified = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
            .to_string();
        let name = path
            .file_name()
            .ok_or("region file has no name")?
            .to_string_lossy();
        out.insert(name.into_owned(), (metadata.len(), modified));
    }
    for rel in ["../level.dat", "../data/tfg_worldgen_versions.dat"] {
        let path = dir.join(rel);
        if path.exists() {
            let metadata = path.metadata()?;
            let modified = metadata
                .modified()?
                .duration_since(UNIX_EPOCH)?
                .as_nanos()
                .to_string();
            out.insert(rel.into(), (metadata.len(), modified));
        }
    }
    Ok(out)
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn sha256(path: &Path) -> Option<String> {
    let output = Command::new("sha256sum").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .split_whitespace()
        .next()
        .map(str::to_owned)
}

fn fingerprints(root: &Path) -> Result<BTreeMap<String, Option<String>>, Box<dyn Error>> {
    let mut paths = Vec::new();
    rust_files(&root.join("src"), &mut paths)?;
    paths.extend(
        [
            "assets/data/tfg/structure-applicability.json",
            "assets/data/tfg/structure-previews.json.gz",
            "assets/data/tfg/biome-heights.json",
            "assets/data/tfg/biome-rivers.json",
            "assets/data/tfg/biome-shores.json",
            "assets/data/tfg/biome-blend-types.json",
        ]
        .map(|path| root.join(path)),
    );
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path.strip_prefix(root)?.to_string_lossy().into_owned();
            Ok((name, sha256(&path)))
        })
        .collect()
}

fn region_count(stamps: &BTreeMap<String, Stamp>) -> usize {
    stamps.keys().filter(|name| name.ends_with(".mca")).count()
}

fn prediction_key(id: &str) -> Option<Start> {
    let (label, pos) = id.rsplit_once('@')?;
    let (x, z) = pos.split_once(',')?;
    Some((label.to_owned(), x.parse().ok()?, z.parse().ok()?))
}

fn marker_chunk(marker: &terralens::model::Marker) -> (i32, i32) {
    (
        (marker.x.floor() as i32).div_euclid(16),
        (marker.z.floor() as i32).div_euclid(16),
    )
}

fn family(id: &str) -> String {
    for name in ["arabic", "malay", "norse", "yurts"] {
        if id.starts_with(&format!("tfg:illagers/{name}_village")) {
            return format!("{name}_village");
        }
    }
    "other".into()
}

fn rate(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then(|| numerator as f64 / denominator as f64)
}

fn counts_to_json(counts: &BTreeMap<String, (usize, usize, usize)>, key: &str) -> Vec<Value> {
    counts
        .iter()
        .map(|(name, (tp, fp, fn_))| {
            let fp_rate = rate(*fp, tp + fp);
            let fn_rate = rate(*fn_, tp + fn_);
            json!({
                key: name, "tp": tp, "fp": fp, "fn": fn_,
                "fp_rate": fp_rate, "fn_rate": fn_rate,
                "pass": fp_rate.is_some_and(|v| v < ERROR_TARGET)
                    && fn_rate.is_some_and(|v| v < ERROR_TARGET)
            })
        })
        .collect()
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(world_arg) = args.next() else {
        return Err(format!(
            "usage: {} WORLD_OR_LEVEL_DAT",
            Path::new(&program).display()
        )
        .into());
    };
    if args.next().is_some() {
        return Err(format!(
            "usage: {} WORLD_OR_LEVEL_DAT",
            Path::new(&program).display()
        )
        .into());
    }

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = PathBuf::from(world_arg);
    let imported = terralens::world_files::resolve_source(&source)?;
    if imported.worlds.len() != 1 {
        return Err(format!("expected one world, found {}", imported.worlds.len()).into());
    }
    let info = imported
        .worlds
        .into_iter()
        .next()
        .ok_or("world import returned no world")?;
    if !info.compatible {
        return Err("audit requires a supported TFG Overworld generation profile".into());
    }

    let region_dir = info.path.join("region");
    let before = stamps(&region_dir)?;
    let hashes_before = fingerprints(&repo)?;
    eprintln!("Scanning saved structure starts in {}", info.path.display());
    let scan =
        prism::scan_structures(&info.path).map_err(|e| format!("saved structure scan: {e}"))?;

    let mut coverage = BTreeMap::new();
    let mut completed_regions = 0usize;
    let mut full_chunks = 0usize;
    for chunk_map in &scan.completed {
        let count: usize = chunk_map
            .slots
            .iter()
            .map(|slot| slot.count_ones() as usize)
            .sum();
        if count > 0 {
            completed_regions += 1;
            full_chunks += count;
        }
        coverage.insert((chunk_map.region_x, chunk_map.region_z), chunk_map);
    }

    let world = World::for_dimension(
        info.seed,
        info.settings.clone(),
        Dimension::Overworld,
        Dimension::Overworld.default_y(),
    );
    let definitions = terralens::structure_support::source();
    let structures: HashMap<String, &Value> = definitions["structures"]
        .as_array()
        .ok_or("structure definitions missing structures array")?
        .iter()
        .filter_map(|structure| Some((structure["id"].as_str()?.to_owned(), structure)))
        .collect();

    let mut runtime_ids = BTreeSet::new();
    let mut unevaluable = BTreeSet::new();
    for set in definitions["sets"]
        .as_array()
        .ok_or("structure sets are missing")?
    {
        let placement = &set["placement"];
        if !matches!(
            placement["type"].as_str(),
            Some("minecraft:random_spread" | "tfc:climate")
        ) {
            continue;
        }
        let (Some(spacing), Some(separation)) = (
            placement["spacing"].as_i64(),
            placement["separation"].as_i64(),
        ) else {
            continue;
        };
        if spacing <= 0
            || separation < 0
            || separation >= spacing
            || placement["frequency"]
                .as_f64()
                .is_some_and(|value| value != 1.0)
        {
            continue;
        }
        for child in set["children"]
            .as_array()
            .ok_or("structure set children are missing")?
        {
            let Some(id) = child["id"].as_str() else {
                continue;
            };
            let Some(definition) = structures.get(id) else {
                continue;
            };
            if !terralens::structure_support::allowed_biomes(Dimension::Overworld, id)
                .is_some_and(|biomes| !biomes.is_empty())
            {
                continue;
            }
            let conditions = definition["definition"]["conditions"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if conditions.iter().any(|condition| {
                !matches!(condition["type"].as_str(), Some("avoid_biomes" | "height"))
            }) {
                unevaluable.insert(id.to_owned());
                continue;
            }
            runtime_ids.insert(id.to_owned());
        }
    }

    let mut predictions = BTreeSet::<Start>::new();
    let mut prediction_passes = 0usize;
    let mut truncated = Vec::new();
    let mut raw_candidates = 0usize;
    let mut candidates_outside_cohort = 0usize;
    for (&(region_x, region_z), chunk_map) in &coverage {
        if !chunk_map.slots.iter().any(|slot| *slot != 0) {
            continue;
        }
        prediction_passes += 1;
        let x0 = i64::from(region_x) * 512;
        let z0 = i64::from(region_z) * 512;
        let bounds = Bounds {
            min_x: x0 as f64,
            min_z: z0 as f64,
            max_x: (x0 + 511) as f64,
            max_z: (z0 + 511) as f64,
        };
        let batch = features::structure_candidates_with_world(
            info.seed,
            bounds,
            |x, z| world.biome(x, z),
            |x, z| Some(world.climate(x, z)),
            |chunk_x, chunk_z| world.region.forest_type(chunk_x, chunk_z),
            |x, z| world.height(x, z).map(|height| height as i32),
        );
        raw_candidates += batch.markers.len();
        if batch.coverage.truncated {
            truncated.push(json!({
                "region": [region_x, region_z],
                "scanned_cells": batch.coverage.scanned_cells,
                "marker_count": batch.coverage.marker_count
            }));
        }
        for marker in batch.markers {
            if let Some(key) = prediction_key(&marker.id) {
                if chunk_map.contains(key.1, key.2) {
                    predictions.insert(key);
                } else {
                    candidates_outside_cohort += 1;
                }
            }
        }
    }

    // Saved starts enter only after raw predictions have been generated.
    let mut actual_all = BTreeSet::<Start>::new();
    for marker in &scan.markers {
        let (chunk_x, chunk_z) = marker_chunk(marker);
        if coverage
            .get(&(chunk_x.div_euclid(32), chunk_z.div_euclid(32)))
            .is_some_and(|chunk_map| chunk_map.contains(chunk_x, chunk_z))
        {
            actual_all.insert((marker.id.clone(), chunk_x, chunk_z));
        }
    }
    let actual_supported: BTreeSet<_> = actual_all
        .iter()
        .filter(|(id, _, _)| runtime_ids.contains(id))
        .cloned()
        .collect();
    let actual_unscored: BTreeSet<_> = actual_all
        .iter()
        .filter(|(id, _, _)| !runtime_ids.contains(id))
        .cloned()
        .collect();
    let tp: BTreeSet<_> = predictions
        .intersection(&actual_supported)
        .cloned()
        .collect();
    let fp: BTreeSet<_> = predictions.difference(&actual_supported).cloned().collect();
    let fn_: BTreeSet<_> = actual_supported.difference(&predictions).cloned().collect();

    let mut by_type = BTreeMap::<String, (usize, usize, usize)>::new();
    for id in &runtime_ids {
        by_type.entry(id.clone()).or_default();
    }
    for (id, _, _) in &tp {
        by_type.entry(id.clone()).or_default().0 += 1;
    }
    for (id, _, _) in &fp {
        by_type.entry(id.clone()).or_default().1 += 1;
    }
    for (id, _, _) in &fn_ {
        by_type.entry(id.clone()).or_default().2 += 1;
    }

    let mut by_family = BTreeMap::<String, (usize, usize, usize)>::new();
    for name in [
        "arabic_village",
        "malay_village",
        "norse_village",
        "yurts_village",
        "other",
    ] {
        by_family.insert(name.into(), (0, 0, 0));
    }
    for (id, counts) in &by_type {
        let family_counts = by_family.entry(family(id)).or_default();
        family_counts.0 += counts.0;
        family_counts.1 += counts.1;
        family_counts.2 += counts.2;
    }

    let fp_same_chunk = fp
        .iter()
        .filter(|(_, x, z)| {
            actual_supported
                .iter()
                .any(|(_, ax, az)| ax == x && az == z)
        })
        .count();
    let fn_different_prediction = fn_
        .iter()
        .filter(|(_, x, z)| predictions.iter().any(|(_, px, pz)| px == x && pz == z))
        .count();
    let mut unscored_by_id = BTreeMap::<String, usize>::new();
    for (id, _, _) in &actual_unscored {
        *unscored_by_id.entry(id.clone()).or_default() += 1;
    }
    let mut errors_by_file = BTreeMap::<String, usize>::new();
    for error in &scan.errors {
        *errors_by_file
            .entry(error.split(':').next().unwrap_or(error).to_owned())
            .or_default() += 1;
    }

    let tp_count = tp.len();
    let fp_count = fp.len();
    let fn_count = fn_.len();
    let fp_rate = rate(fp_count, tp_count + fp_count);
    let fn_rate = rate(fn_count, tp_count + fn_count);
    let jaccard_error = rate(fp_count + fn_count, tp_count + fp_count + fn_count);
    let score_pass = fp_rate.is_some_and(|value| value < ERROR_TARGET)
        && fn_rate.is_some_and(|value| value < ERROR_TARGET)
        && jaccard_error.is_some_and(|value| value < ERROR_TARGET);

    let after = stamps(&region_dir)?;
    let hashes_after = fingerprints(&repo)?;
    let changed_stamps: Vec<_> = before.iter().filter_map(|(name, old)| {
        after.get(name).filter(|new| *new != old).map(|new| json!({"file":name,"before":{"bytes":old.0,"mtime_ns":old.1},"after":{"bytes":new.0,"mtime_ns":new.1}}))
    }).collect();
    let new_files: Vec<_> = after
        .keys()
        .filter(|name| !before.contains_key(*name))
        .cloned()
        .collect();
    let removed_files: Vec<_> = before
        .keys()
        .filter(|name| !after.contains_key(*name))
        .cloned()
        .collect();
    let changed_sources = hashes_before != hashes_after;
    let unavailable_hashes: Vec<_> = hashes_before
        .iter()
        .filter(|(_, hash)| hash.is_none())
        .map(|(name, _)| name.clone())
        .collect();
    let hashes_verified = unavailable_hashes.is_empty()
        && hashes_after.values().all(Option::is_some)
        && hashes_before == hashes_after;
    let save_unchanged =
        changed_stamps.is_empty() && new_files.is_empty() && removed_files.is_empty();
    let no_truncation = truncated.is_empty();
    let no_scan_errors = scan.errors.is_empty();
    let has_actual_support = !actual_supported.is_empty();
    let no_unscored_actuals = actual_unscored.is_empty();
    let valid = has_actual_support
        && no_unscored_actuals
        && no_scan_errors
        && no_truncation
        && save_unchanged
        && hashes_verified;
    let passed = valid && score_pass;

    let village_families = counts_to_json(&by_family, "family");
    let structure_types = counts_to_json(&by_type, "id");
    let report = json!({
        "schema": 2,
        "world": {"name": info.name, "seed": info.seed, "dimension": "minecraft:overworld", "settings": info.settings},
        "method": {
            "predictor": "features::structure_candidates_with_world, once per region; saved starts are never inputs to prediction",
            "matching": "exact canonical structure ID and start chunk; deduplicated by (id, chunk_x, chunk_z)",
            "cohort": "predictions and saved starts are compared only in completed full chunks",
            "error_target": ERROR_TARGET,
            "overall_pass_rule": "false-positive, false-negative and combined Jaccard error rates are all strictly below 5%; per-type results are reported but do not gate overall score"
        },
        "validity": {
            "valid": valid,
            "passed": passed,
            "has_scored_actual_starts": has_actual_support,
            "no_unscored_actual_starts": no_unscored_actuals,
            "no_scan_errors": no_scan_errors,
            "no_truncated_prediction_regions": no_truncation,
            "save_unchanged_during_scan": save_unchanged,
            "source_hashes_verified_and_unchanged": hashes_verified,
            "score_below_target": score_pass,
            "requirements": ["at least one scored actual start", "all actual starts represented in the score", "zero scan errors", "zero truncated prediction regions", "save fingerprints unchanged", "all source hashes available and unchanged", "FP, FN and combined Jaccard error rates each below 5%"]
        },
        "coverage": {
            "region_files_before": region_count(&before), "region_files_after": region_count(&after),
            "completed_regions": completed_regions, "full_chunks": full_chunks,
            "decoded_chunks": scan.chunks, "prediction_region_passes": prediction_passes,
            "truncated_region_passes": truncated, "raw_candidates": raw_candidates,
            "candidates_outside_completed_chunks": candidates_outside_cohort,
            "predictions_in_cohort": predictions.len(), "actual_starts_in_cohort": actual_all.len(),
            "actual_scored_starts": actual_supported.len(), "actual_unscored_starts": actual_unscored.len(),
            "scan_errors": scan.errors.len(), "scan_errors_by_file": errors_by_file
        },
        "score": {
            "tp": tp_count, "fp": fp_count, "fn": fn_count,
            "fp_rate": fp_rate, "fn_rate": fn_rate, "combined_jaccard_error_rate": jaccard_error,
            "fp_same_chunk_as_other_saved_start": fp_same_chunk,
            "fp_without_scored_start_in_chunk": fp_count - fp_same_chunk,
            "fn_with_other_prediction_in_chunk": fn_different_prediction,
            "fn_without_prediction_in_chunk": fn_count - fn_different_prediction,
            "per_village_family": village_families, "per_structure_type": structure_types,
            "village_completed_cohort": {
                "families": ["arabic_village", "malay_village", "norse_village", "yurts_village"],
                "saved_yurts_starts": actual_supported.iter().filter(|(id, _, _)| id.starts_with("tfg:illagers/yurts_village")).count(),
                "evidence_note": "zero saved Yurts starts in this completed-chunk cohort is no positive-generation evidence, not evidence the type cannot generate"
            }
        },
        "actual_types_not_evaluable": unevaluable,
        "false_positives": fp, "false_negatives": fn_,
        "unscored_actual": {"count": actual_unscored.len(), "by_id": unscored_by_id, "examples": actual_unscored.iter().take(500).collect::<Vec<_>>()},
        "freshness": {
            "save_stamp_changes": changed_stamps, "new_region_files": new_files, "removed_region_files": removed_files,
            "source_hashes_sha256_before": hashes_before, "source_hashes_sha256_after": hashes_after,
            "source_hashes_unavailable": unavailable_hashes, "source_files_changed": changed_sources,
            "hash_tool": "sha256sum", "fresh_process_scan_cache": "empty"
        }
    });

    eprintln!(
        "Audit: {full_chunks} complete chunks, TP={tp_count}, FP={fp_count}, FN={fn_count}; validity={}, result={}",
        if valid { "valid" } else { "invalid" },
        if passed { "PASS" } else { "FAIL" },
    );
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
    println!();
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("structure_accuracy: {error}");
        std::process::exit(2);
    }
}
