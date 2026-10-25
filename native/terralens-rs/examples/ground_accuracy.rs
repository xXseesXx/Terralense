//! Compare seed-only surface materials with the independent World 0 fixtures.
use serde_json::{Value, json};
use terralens::{ground, model::Settings, volcanic_lava::Surface, world::World};

fn main() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ground/seed0-ground-samples.json"
    ))
    .unwrap();
    let world = World::new(0, Settings::default());
    let mut sites = Vec::new();
    let (mut total, mut exact) = (0, 0);
    for site in fixture["sites"].as_array().unwrap() {
        let cx = site["chunk"][0].as_i64().unwrap() as i32;
        let cz = site["chunk"][1].as_i64().unwrap() as i32;
        let (mut matched, mut n) = (0, 0);
        let mut misses = std::collections::BTreeMap::<String, usize>::new();
        let mut mismatch_examples = Vec::new();
        for row in site["columns"].as_array().unwrap() {
            let x = cx * 16 + row["x"].as_i64().unwrap() as i32;
            let z = cz * 16 + row["z"].as_i64().unwrap() as i32;
            let actual = row["ground_block"].as_str().unwrap();
            let expected_fluid =
                Surface::from_block(actual, row["ground_y"].as_i64().unwrap() as i32);
            let physical = world
                .volcanic_surface(x, z)
                .or_else(|| world.is_surface_water(x, z).then_some(Surface::Water(62)));
            let computed = ground::sample(&world, x, z);
            let matches = match (expected_fluid, physical) {
                (Some(Surface::Water(_)), Some(Surface::Water(_)))
                | (Some(Surface::Lava(_)), Some(Surface::Lava(_)))
                | (Some(Surface::Magma(_)), Some(Surface::Magma(_))) => true,
                (None, None) => computed.as_ref().is_some_and(|g| &*g.block == actual),
                _ => false,
            };
            n += 1;
            matched += usize::from(matches);
            if !matches {
                let estimated_height = world.height(x, z);
                let computed = physical
                    .map(|s| format!("{s:?}"))
                    .unwrap_or_else(|| computed.map_or("none".into(), |g| g.block.to_string()));
                *misses.entry(format!("{actual} -> {computed}")).or_default() += 1;
                if mismatch_examples.len() < 24 {
                    mismatch_examples.push(json!({
                        "x": x, "z": z,
                        "actual_ground_y": row["ground_y"],
                        "actual_ground_block": actual,
                        "estimated_noise_height": estimated_height,
                        "estimated_surface": computed,
                    }));
                }
            }
        }
        total += n;
        exact += matched;
        sites.push(
            json!({"site": site["name"], "columns": n, "matched_material_or_fluid": matched,
            "accuracy": matched as f64 / n as f64, "mismatched_actual_materials": misses,
            "mismatch_examples": mismatch_examples}),
        );
    }
    println!("{}", serde_json::to_string_pretty(&json!({
        "seed": 0, "mode": "seed_only_without_saved_ground", "columns": total,
        "matched_material_or_fluid": exact, "accuracy": exact as f64 / total as f64,
        "notes": ["Fixed sites are biased toward volcanic and covered terrain; this is not a global accuracy estimate.",
            "Only fluid class is scored on water, lava and magma columns; their fill heights are evaluated separately.",
            "Saved-chunk extraction is checked independently in prism::surface tests and is not included in this seed-only score."],
        "sites": sites,
    })).unwrap());
}
