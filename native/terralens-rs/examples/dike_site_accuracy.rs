//! Compare seed-only visible surfaces with the read-only World 0 spring-site capture.
use serde_json::{Value, json};
use terralens::{ground, model::Settings, world::World};

fn rock(block: &str) -> bool {
    ["tfc:rock/raw/", "tfc:rock/gravel/", "tfc:rock/hardened/"]
        .iter()
        .any(|prefix| block.starts_with(prefix))
}

fn main() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ground/dike-user-site-seed0.json"
    ))
    .unwrap();
    let world = World::new(0, Settings::default());
    let mut total = 0;
    let mut exact = 0;
    let mut rock_total = 0;
    let mut rock_exact = 0;
    let mut height_exact = 0;
    let mut block_height_exact = 0;
    let mut water_total = 0;
    let mut water_true_positive = 0;
    let mut water_false_positive = 0;
    let mut pairs = std::collections::BTreeMap::<String, usize>::new();
    let mut divergences = Vec::new();
    let mut controls = Vec::new();
    let control_points = [
        (-319_262, 16_107),
        (-319_245, 16_110),
        (-319_246, 16_110),
        (-319_244, 16_110),
    ];
    for chunk in fixture["chunks"].as_array().unwrap() {
        for row in chunk["columns"].as_array().unwrap() {
            let x = row["world_x"].as_i64().unwrap() as i32;
            let z = row["world_z"].as_i64().unwrap() as i32;
            let actual = row["ground_block"].as_str().unwrap();
            let mut predicted = ground::sample(&world, x, z).unwrap();
            if let Some(surface) = world.volcanic_surface(x, z) {
                use terralens::volcanic_lava::Surface;
                let (block, y) = match surface {
                    Surface::Water(y) => ("minecraft:water", y),
                    Surface::Lava(y) => ("minecraft:lava", y),
                    Surface::Magma(y) => ("minecraft:magma_block", y),
                };
                predicted.block = block.into();
                predicted.y = y;
            }
            let actual_y = row["ground_y"].as_i64().unwrap() as i32;
            total += 1;
            exact += usize::from(predicted.block.as_ref() == actual);
            height_exact += usize::from(predicted.y == actual_y);
            block_height_exact +=
                usize::from(predicted.y == actual_y && predicted.block.as_ref() == actual);
            if actual == "minecraft:water" {
                water_total += 1;
                water_true_positive += usize::from(predicted.block.as_ref() == actual);
            } else if predicted.block.as_ref() == "minecraft:water" {
                water_false_positive += 1;
            }
            if rock(actual) {
                rock_total += 1;
                rock_exact += usize::from(predicted.block.as_ref() == actual);
            }
            if predicted.block.as_ref() != actual && divergences.len() < 100 {
                divergences.push(json!({
                    "x": x,
                    "z": z,
                    "saved_y": row["ground_y"],
                    "saved": actual,
                    "predicted_y": predicted.y,
                    "predicted": predicted.block.as_ref(),
                    "host_rock": world.rock_at(x, predicted.y, z),
                }));
            }
            if predicted.block.as_ref() != actual {
                *pairs
                    .entry(format!("{actual} -> {}", predicted.block))
                    .or_default() += 1;
            }
            if control_points.contains(&(x, z)) {
                let actual_surface = row["surface_height_forgecaps"].as_i64().unwrap() as i32;
                let (point, layer) = world.region.rock_layer_at(
                    x,
                    row["ground_y"].as_i64().unwrap() as i32,
                    z,
                    actual_surface,
                );
                controls.push(json!({
                    "x": x,
                    "z": z,
                    "saved_y": row["ground_y"],
                    "saved": actual,
                    "saved_surface_height": row["surface_height_forgecaps"],
                    "predicted_y": predicted.y,
                    "predicted": predicted.block.as_ref(),
                    "height": world.height(x,z),
                    "host_at_saved_y": world.rock_at(x,row["ground_y"].as_i64().unwrap() as i32,z),
                    "host_at_predicted_y": world.rock_at(x,predicted.y,z),
                    "host_at_y0": world.rock_at(x,0,z),
                    "host_at_saved_forgecaps_y": terralens::geology::sample_rock_layer(point, layer),
                }));
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "seed": 0,
            "reference": fixture["provenance"],
            "columns": total,
            "exact_surface_materials": exact,
            "exact_surface_heights": height_exact,
            "exact_surface_block_and_height": block_height_exact,
            "water_columns": water_total,
            "water_true_positive": water_true_positive,
            "water_false_positive": water_false_positive,
            "surface_accuracy": exact as f64 / total as f64,
            "exposed_rock_columns": rock_total,
            "exact_exposed_rock_materials": rock_exact,
            "exposed_rock_accuracy": if rock_total == 0 { 1.0 } else { rock_exact as f64 / rock_total as f64 },
            "controls": controls,
            "mismatch_pairs": pairs,
            "divergences": divergences,
        }))
        .unwrap()
    );
}
