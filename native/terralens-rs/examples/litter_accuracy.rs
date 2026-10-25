//! Seed-only litter projection versus the existing independent saved top columns.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use terralens::{model::Settings, surface_litter, world::World};

fn main() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ground/seed0-ground-samples.json"
    ))
    .unwrap();
    let world = World::new(0, Settings::default());
    let mut results = Vec::new();
    for site in fixture["sites"].as_array().unwrap() {
        let cx = site["chunk"][0].as_i64().unwrap() as i32;
        let cz = site["chunk"][1].as_i64().unwrap() as i32;
        let projected = surface_litter::predict_chunk(&world, cx, cz, &|| false).unwrap();
        let mut actual = BTreeMap::<String, usize>::new();
        let mut predicted = BTreeMap::<String, usize>::new();
        let mut exact = 0;
        let mut xy = 0;
        let mut hidden = 0;
        for row in site["columns"].as_array().unwrap() {
            let x = row["x"].as_u64().unwrap() as usize;
            let z = row["z"].as_u64().unwrap() as usize;
            let block = row["top_block"].as_str().unwrap();
            if surface_litter::classify(block).is_some() {
                *actual.entry(block.into()).or_default() += 1;
            }
            let Some(p) = projected.get(x, z) else {
                continue;
            };
            *predicted.entry(p.block.clone()).or_default() += 1;
            xy += usize::from(surface_litter::classify(block).is_some());
            exact += usize::from(p.block == block && Some(i64::from(p.y)) == row["top_y"].as_i64());
            hidden += usize::from(row["top_y"].as_i64().unwrap() > i64::from(p.y));
        }
        results.push(
            json!({"site":site["name"],"chunk":[cx,cz],"saved_top_litter":actual,
            "projected_litter":predicted,"exact_block_y_matches":exact,"litter_xy_matches":xy,
            "projected_columns_below_saved_cover":hidden}),
        );
    }
    println!("{}",serde_json::to_string_pretty(&json!({"seed":0,
        "scope":"Independent saved top columns compared with seed-only litter. Earlier vegetation, structures, post-generation changes and occupied-state differences remain; hidden ground litter is not directly observable in top columns. This is not saved-reader or full chunk generation parity.",
        "reference":fixture["provenance"],"sites":results})).unwrap());
}
