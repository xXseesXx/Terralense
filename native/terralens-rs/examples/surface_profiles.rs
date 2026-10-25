//! Read-only independent World 0 voxel and surface-column scorecard.
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, time::Instant};
use terralens::{
    caves::{self, Cell},
    model::Settings,
    world::World,
};
fn name(c: Cell) -> &'static str {
    match c {
        Cell::Solid => "solid",
        Cell::Air => "air",
        Cell::Water => "water",
        Cell::Lava => "lava",
    }
}
fn main() {
    let mut s = String::new();
    GzDecoder::new(
        &include_bytes!("../tests/fixtures/generator-accuracy/world0-surface-profiles-48.json.gz")
            [..],
    )
    .read_to_string(&mut s)
    .unwrap();
    let f: Value = serde_json::from_str(&s).unwrap();
    let world = World::new(0, Settings::default());
    let start = Instant::now();
    let mut confusion = BTreeMap::<String, usize>::new();
    let mut sites = Vec::new();
    let mut n = 0;
    let mut excluded = 0;
    for chunk in f["chunks"].as_array().unwrap() {
        let (cx, cz) = (
            chunk["chunk"][0].as_i64().unwrap() as i32,
            chunk["chunk"][1].as_i64().unwrap() as i32,
        );
        let (mut matched, mut total) = (0, 0);
        let mut examples = Vec::new();
        for p in chunk["vertical_lattice_4x4"].as_array().unwrap() {
            let x = cx * 16 + p["local"][0].as_i64().unwrap() as i32;
            let z = cz * 16 + p["local"][1].as_i64().unwrap() as i32;
            for run in p["rle"].as_array().unwrap() {
                let actual = run[2].as_str().unwrap();
                let y0 = run[0].as_i64().unwrap() as i32;
                let count = run[1].as_i64().unwrap() as i32;
                if actual == "unknown" || actual == "excluded_snow_ice" {
                    excluded += count;
                    continue;
                }
                for y in y0..y0 + count {
                    let predicted = name(caves::state_at(&world, x, y, z).unwrap());
                    *confusion
                        .entry(format!("{actual} -> {predicted}"))
                        .or_default() += 1;
                    total += 1;
                    n += 1;
                    matched += usize::from(actual == predicted);
                    if actual != predicted && examples.len() < 8 {
                        examples.push(json!([x, y, z, actual, predicted]));
                    }
                }
            }
        }
        let (mut surface_exact, mut surface_n) = (0, 0);
        let mut surface_examples = Vec::new();
        for row in chunk["highest_natural_surface_16x16"].as_array().unwrap() {
            if row[4].as_str() == Some("excluded_snow_ice") {
                continue;
            }
            let x = cx * 16 + row[0].as_i64().unwrap() as i32;
            let z = cz * 16 + row[1].as_i64().unwrap() as i32;
            let y = row[2].as_i64().unwrap() as i32;
            let p = caves::column(&world, x, z).unwrap();
            let (k, py) = p.fluid.map_or(("solid", p.solid_y), |(c, y)| (name(c), y));
            let same = py == y && Some(k) == row[4].as_str();
            surface_exact += usize::from(same);
            surface_n += 1;
            if !same && surface_examples.len() < 6 {
                surface_examples.push(json!([x, z, y, row[3], row[4], py, k, p.solid_y]));
            }
        }
        sites.push(json!({"chunk":[cx,cz],"voxels":total,"matches":matched,"voxel_examples":examples,"surface_columns":surface_n,"surface_exact_y_and_class":surface_exact,"surface_examples":surface_examples}));
        eprintln!("chunk {cx},{cz} voxels {matched}/{total} surface {surface_exact}/{surface_n}");
    }
    let correct = confusion
        .iter()
        .filter(|(s, _)| {
            let (a, b) = s.split_once(" -> ").unwrap();
            a == b
        })
        .map(|(_, n)| n)
        .sum::<usize>();
    println!("{}",serde_json::to_string_pretty(&json!({"seed":0,"mode":"seed-only without saved blocks","reference":f["provenance"],"seconds":start.elapsed().as_secs_f64(),"voxels":n,"excluded_unknown_or_frozen_voxels":excluded,"correct_voxels":correct,"accuracy":correct as f64/n as f64,"confusion":confusion,"sites":sites})).unwrap());
}
