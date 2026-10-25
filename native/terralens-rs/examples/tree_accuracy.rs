//! Compare seed-only tree top-column predictions with exact saved World 0 foliage.
//!
//! Usage: `cargo run --release --features diagnostics --example tree_accuracy -- WORLD_DIR CX CZ RADIUS [FULL_HEIGHT_SUPPORT_PROXY] [FIRST_AVAILABLE_ORIGIN] [INDEXES]`
//! The saved chunks are read only. CX/CZ and RADIUS are chunk coordinates; radius
//! is capped at 16. This is a diagnostic for the dynamic TFG feature-sort index.

use std::{collections::HashSet, path::PathBuf};
use terralens::{
    model::Settings,
    prism::read_surface_tile,
    vegetation::{self, SurfacePlantKind},
    world::World,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let world_path = PathBuf::from(args.next().expect("WORLD_DIR required"));
    let center_x = args.next().and_then(|v| v.parse().ok()).unwrap_or(20_i32);
    let center_z = args.next().and_then(|v| v.parse().ok()).unwrap_or(20_i32);
    let radius = args
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4_i32)
        .clamp(1, 16);
    let full_height_support_proxy = args.next().map(|value| value == "on").unwrap_or(false);
    let first_available_origin = args.next().map(|value| value != "off").unwrap_or(true);
    let feature_indices = args
        .next()
        .map(|value| {
            value
                .split(',')
                .map(|part| part.parse::<u32>().expect("valid feature index"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| (0..129).collect());
    let min_x = center_x - radius;
    let max_x = center_x + radius;
    let min_z = center_z - radius;
    let max_z = center_z + radius;
    let world = World::new(0, Settings::default());

    let mut actual = HashSet::new();
    let mut complete = 0;
    for cz in min_z..max_z {
        for cx in min_x..max_x {
            let Ok(Some(chunk)) = read_surface_tile(&world_path, cx, cz) else {
                continue;
            };
            complete += 1;
            for (index, column) in chunk.columns.iter().enumerate() {
                if column.tree.is_some() {
                    actual.insert((cx * 16 + index as i32 % 16, cz * 16 + index as i32 / 16));
                }
            }
        }
    }
    assert!(complete > 0, "no complete saved chunks in selected area");

    let mut rows = Vec::new();
    // KubeJS can shift this index beyond the 41-feature canonical profile.
    for feature_index in feature_indices.iter().copied() {
        let mut predicted = HashSet::new();
        // TFG palms include two random patch spreads before their templates.
        for cz in (min_z - 2)..=(max_z + 1) {
            for cx in (min_x - 2)..=(max_x + 1) {
                for plant in vegetation::forest_source_chunk_with_options(
                    &world,
                    cx,
                    cz,
                    feature_index,
                    full_height_support_proxy,
                    first_available_origin,
                    &|| false,
                ) {
                    if plant.kind == SurfacePlantKind::Tree
                        && plant.x >= min_x * 16
                        && plant.x < max_x * 16
                        && plant.z >= min_z * 16
                        && plant.z < max_z * 16
                    {
                        predicted.insert((plant.x, plant.z));
                    }
                }
            }
        }
        let true_positive = predicted.intersection(&actual).count();
        let false_positive = predicted.difference(&actual).count();
        let false_negative = actual.difference(&predicted).count();
        let precision = ratio(true_positive, true_positive + false_positive);
        let recall = ratio(true_positive, true_positive + false_negative);
        let f1 = if precision + recall == 0. {
            0.
        } else {
            2. * precision * recall / (precision + recall)
        };
        rows.push(serde_json::json!({
            "feature_index": feature_index,
            "predicted_leaf_top_columns": predicted.len(),
            "saved_leaf_top_columns": actual.len(),
            "true_positive": true_positive,
            "false_positive": false_positive,
            "false_negative": false_negative,
            "precision": precision,
            "recall": recall,
            "f1": f1,
        }));
    }
    rows.sort_by(|a, b| {
        b["f1"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&a["f1"].as_f64().unwrap_or_default())
    });
    println!(
        "{}",
        serde_json::json!({
            "seed": 0,
            "sample_region_chunks": [min_x, min_z, max_x - 1, max_z - 1],
            "complete_saved_chunks": complete,
            "full_height_support_proxy": full_height_support_proxy,
            "first_available_origin": first_available_origin,
            "saved_leaf_top_columns": actual.len(),
            "index25": rows.iter().find(|row| row["feature_index"] == 25),
            "best_indices": rows.iter().take(5).collect::<Vec<_>>(),
            "metric_note": "This sweep forecasts the forest feature only; physical fluid checks remain active, while the optional height-support gate is controlled separately. Saved leaf top columns may include palms, bushes and other foliage. It compares 2D foliage-column presence, not exact origins, species, or canopy height; these scores are not exact placement accuracy.",
            "tested_feature_indices": feature_indices
        })
    );
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.
    } else {
        numerator as f64 / denominator as f64
    }
}
