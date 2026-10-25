//! Immutable World 0 reference compared with each seed-only generation stage.
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
use terralens::{caves, ground, model::Settings, world::World};
fn fixture(bytes: &[u8]) -> Value {
    let mut s = String::new();
    GzDecoder::new(bytes).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}
fn fluid(block: &str) -> Option<&'static str> {
    if block.contains("magma") {
        Some("magma")
    } else if block.contains("lava") {
        Some("lava")
    } else if block.contains("water") {
        Some("water")
    } else {
        None
    }
}
fn frozen(block: &str) -> bool {
    matches!(
        block.rsplit(':').next(),
        Some("snow" | "snow_block" | "ice" | "sea_ice" | "packed_ice" | "blue_ice")
    )
}
fn main() {
    let world = World::new(0, Settings::default());
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 2 {
        let x = args[0].parse().unwrap();
        let z = args[1].parse().unwrap();
        println!(
            "{}",
            serde_json::to_string_pretty(&ground::diagnostic_column(&world, x, z)).unwrap()
        );
        return;
    }
    let f = fixture(include_bytes!(
        "../tests/fixtures/generator-accuracy/world0-spread-48.json.gz"
    ));
    let mut rows = Vec::new();
    let mut pairs = BTreeMap::<String, usize>::new();
    let mut stages = BTreeMap::<String, usize>::new();
    let mut total = 0;
    let mut exact = 0;
    for chunk in f["chunks"].as_array().unwrap() {
        for row in chunk["columns"].as_array().unwrap() {
            let actual = row["ground_block"].as_str().unwrap();
            if frozen(actual) {
                continue;
            }
            total += 1;
            let (x, z) = (
                row["x"].as_i64().unwrap() as i32,
                row["z"].as_i64().unwrap() as i32,
            );
            let sample = ground::sample(&world, x, z).unwrap();
            let surface = world.volcanic_surface(x, z);
            let pred_fluid = surface
                .map(|s| match s {
                    terralens::volcanic_lava::Surface::Water(_) => "water",
                    terralens::volcanic_lava::Surface::Lava(_) => "lava",
                    terralens::volcanic_lava::Surface::Magma(_) => "magma",
                })
                .or_else(|| world.is_surface_water(x, z).then_some("water"))
                .or_else(|| fluid(&sample.block));
            let predicted = pred_fluid.unwrap_or(&sample.block);
            let a = fluid(actual).unwrap_or(actual);
            if a == predicted {
                exact += 1;
                continue;
            }
            *pairs.entry(format!("{a} -> {predicted}")).or_default() += 1;
            let trace = ground::diagnostic_column(&world, x, z);
            let stage = if pred_fluid.is_some() && fluid(actual).is_none() {
                if trace["physical"]["fluid"][1]
                    .as_i64()
                    .is_some_and(|top| i64::from(sample.y) >= top)
                    && fluid(&sample.block).is_none()
                {
                    "fluid_mask_above_solid_decoration"
                } else {
                    "fluid_or_terrain_coverage"
                }
            } else if trace["before_forest"]["block"] != trace["final_ground"]["block"] {
                "later_decoration"
            } else if trace["base"] != trace["before_forest"] {
                "early_decoration"
            } else if row["ground_y"].as_i64() > Some(sample.y as i64) {
                "missing_raised_surface"
            } else {
                "surface_builder_or_substrate"
            };
            *stages.entry(stage.to_string()).or_default() += 1;
            rows.push(json!({"actual":row,"predicted_block_or_fluid":predicted,"modeled_divergence_stage":stage,"trace":trace}));
        }
    }
    let profiles = fixture(include_bytes!(
        "../tests/fixtures/generator-accuracy/world0-surface-profiles-48.json.gz"
    ));
    let mut voxel_rows = Vec::new();
    let mut voxel_stages = BTreeMap::<String, usize>::new();
    for chunk in profiles["chunks"].as_array().unwrap() {
        let (cx, cz) = (
            chunk["chunk"][0].as_i64().unwrap() as i32,
            chunk["chunk"][1].as_i64().unwrap() as i32,
        );
        for col in chunk["vertical_lattice_4x4"].as_array().unwrap() {
            let x = cx * 16 + col["local"][0].as_i64().unwrap() as i32;
            let z = cz * 16 + col["local"][1].as_i64().unwrap() as i32;
            for run in col["rle"].as_array().unwrap() {
                let actual = run[2].as_str().unwrap();
                if matches!(actual, "unknown" | "excluded_snow_ice") {
                    continue;
                }
                let y0 = run[0].as_i64().unwrap() as i32;
                let len = run[1].as_i64().unwrap() as i32;
                for y in y0..y0 + len {
                    let p = caves::state_at(&world, x, y, z).unwrap();
                    let predicted = format!("{p:?}").to_ascii_lowercase();
                    if actual == predicted {
                        continue;
                    }
                    let trace = caves::diagnostic_voxel(&world, x, y, z).unwrap();
                    let stage = if trace["before_air_carvers"] == trace["final"] {
                        "density_or_later_decoration"
                    } else {
                        "air_carver"
                    };
                    *voxel_stages.entry(stage.into()).or_default() += 1;
                    voxel_rows.push(json!({"chunk":[cx,cz],"point":[x,y,z],"actual":actual,"predicted":predicted,"modeled_divergence_stage":stage,"trace":trace}));
                }
            }
        }
    }
    println!("{}",serde_json::to_string_pretty(&json!({"seed":0,"scope":"Independent saved reference; seed-only World with no save source. Stages are diagnostic evidence, not automatic proof of a source cause.","reference":f["provenance"],"surface_columns":total,"surface_matches":exact,"surface_mismatches":rows.len(),"surface_pairs":pairs,"surface_stages":stages,"surface_rows":rows,"voxel_stages":voxel_stages,"voxel_rows":voxel_rows})).unwrap());
}
