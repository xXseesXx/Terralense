//! Compare forest leaf-column forecasts with independent saved-chunk fixtures.
//!
//! Usage: `cargo run --release --features diagnostics --example tree_fixture_accuracy -- --fixtures FILE`

use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::PathBuf,
};
use terralens::{
    model::Settings,
    vegetation::{self, SurfacePlantKind},
    world::World,
};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(
        args.first().map(String::as_str),
        Some("--fixtures"),
        "expected --fixtures PATH"
    );
    let path = PathBuf::from(args.get(1).expect("fixture path required"));
    let fixture: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read fixture"))
            .expect("parse fixture JSON");
    let sites = fixture["sites"].as_array().expect("fixture sites");
    let world = World::new(0, Settings::default());
    let mut reports = Vec::new();

    for index in [25_u32, 3] {
        for full_height_support_proxy in [true, false] {
            for first_available_origin in [true, false] {
                let mut site_reports = Vec::new();
                let mut total = Counts::default();
                for site in sites {
                    let rows = site["columns"].as_array().expect("site columns");
                    let actual: HashSet<_> = rows
                        .iter()
                        .filter_map(|row| {
                            let block = row[3].as_str()?;
                            if is_leaf(block) {
                                Some((row[0].as_i64()? as i32, row[1].as_i64()? as i32))
                            } else {
                                None
                            }
                        })
                        .collect();
                    let target_positions: HashSet<_> = rows
                        .iter()
                        .filter_map(|row| Some((row[0].as_i64()? as i32, row[1].as_i64()? as i32)))
                        .collect();
                    let mut source_chunks = BTreeSet::new();
                    for &(x, z) in &target_positions {
                        let cx = x.div_euclid(16);
                        let cz = z.div_euclid(16);
                        for dz in -2..=2 {
                            for dx in -2..=2 {
                                source_chunks.insert((cx + dx, cz + dz));
                            }
                        }
                    }
                    let mut predicted = HashSet::new();
                    for (cx, cz) in source_chunks {
                        for plant in vegetation::forest_source_chunk_with_options(
                            &world,
                            cx,
                            cz,
                            index,
                            full_height_support_proxy,
                            first_available_origin,
                            &|| false,
                        ) {
                            if plant.kind == SurfacePlantKind::Tree
                                && target_positions.contains(&(plant.x, plant.z))
                            {
                                predicted.insert((plant.x, plant.z));
                            }
                        }
                    }
                    let counts = Counts {
                        tp: predicted.intersection(&actual).count(),
                        fp: predicted.difference(&actual).count(),
                        fn_: actual.difference(&predicted).count(),
                    };
                    total.add(counts);
                    site_reports.push(serde_json::json!({
                        "center": site["center"],
                        "columns": target_positions.len(),
                        "saved_foliage_columns": actual.len(),
                        "tp": counts.tp,
                        "fp": counts.fp,
                        "fn": counts.fn_,
                        "precision": ratio(counts.tp, counts.tp + counts.fp),
                        "recall": ratio(counts.tp, counts.tp + counts.fn_),
                        "f1": f1(counts),
                    }));
                }
                reports.push(serde_json::json!({
                    "feature_index": index,
                    "full_height_support_proxy": full_height_support_proxy,
                    "first_available_origin": first_available_origin,
                    "totals": {
                        "tp": total.tp, "fp": total.fp, "fn": total.fn_,
                        "precision": ratio(total.tp, total.tp + total.fp),
                        "recall": ratio(total.tp, total.tp + total.fn_),
                        "f1": f1(total),
                    },
                    "sites": site_reports,
                }));
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "seed": 0,
            "fixture": path,
        "scope": "exact fixture columns; forest feature only; lava/magma/water checks always active; saved foliage includes bushes, palms and other foliage",
            "reports": reports,
        })
    );
}

#[derive(Clone, Copy, Default)]
struct Counts {
    tp: usize,
    fp: usize,
    fn_: usize,
}

impl Counts {
    fn add(&mut self, other: Self) {
        self.tp += other.tp;
        self.fp += other.fp;
        self.fn_ += other.fn_;
    }
}

fn is_leaf(block: &str) -> bool {
    block.contains("/leaves/") || block.ends_with("_leaves") || block.ends_with("_leaf")
}

fn ratio(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 }
}
fn f1(c: Counts) -> f64 {
    let p = ratio(c.tp, c.tp + c.fp);
    let r = ratio(c.tp, c.tp + c.fn_);
    if p + r == 0.0 {
        0.0
    } else {
        2.0 * p * r / (p + r)
    }
}
