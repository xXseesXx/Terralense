//! World-feature markers are compared with the checked-in OutCrop TFG generator snapshot.
//! Coordinates and the counted population are fixed so missing feature classes cannot pass quietly.
use serde_json::Value;
use terralens::{
    features,
    model::{Bounds, Settings},
    world::World,
};

fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/tfg-reference.json"))
        .expect("feature parity fixture")
}
#[test]
fn tfg_structure_and_ore_candidates_match_reference_windows() {
    let data = reference();
    let worlds = data["worlds"].as_array().unwrap();
    assert_eq!(worlds.len(), 3);
    let mut deposit_total = 0usize;
    let mut structure_total = 0usize;
    for entry in worlds {
        let seed = entry["seed"].as_str().unwrap().parse::<i64>().unwrap();
        let b = &entry["featureBounds"];
        let bounds = Bounds {
            min_x: b["minX"].as_f64().unwrap(),
            min_z: b["minZ"].as_f64().unwrap(),
            max_x: b["maxX"].as_f64().unwrap(),
            max_z: b["maxZ"].as_f64().unwrap(),
        };
        let world = World::new(seed, Settings::default());
        let ores = features::ore_candidates_with_climate(
            seed,
            bounds,
            |x, z| world.biome(x, z),
            |x, z| Some(world.climate(x, z)),
            |x, z| world.height(x, z).map(|h| h.floor() as i32),
            |x, y, z| world.rock_at(x, y, z),
        );
        let expected = entry["features"]["deposits"].as_array().unwrap();
        deposit_total += expected.len();
        let actual: std::collections::HashMap<_, _> =
            ores.markers.iter().map(|m| (m.id.as_str(), m)).collect();
        if actual.len() != expected.len() {
            let missing: Vec<_> = expected
                .iter()
                .filter_map(|x| {
                    let id = x["id"].as_str()?;
                    (!actual.contains_key(id)).then_some(id)
                })
                .collect();
            let extras: Vec<_> = ores
                .markers
                .iter()
                .filter(|m| {
                    !expected
                        .iter()
                        .any(|e| e["id"].as_str() == Some(m.id.as_str()))
                })
                .map(|m| m.id.as_str())
                .collect();
            eprintln!(
                "seed={seed} deposits actual={} expected={} missing={} extras={}",
                actual.len(),
                expected.len(),
                missing.join(","),
                extras.join(",")
            );
            let common = expected
                .iter()
                .filter_map(|w| {
                    let id = w["id"].as_str()?;
                    let g = actual.get(id)?;
                    Some(format!(
                        "{id} ts=({}, {}) rs=({}, {})",
                        w["x"], w["z"], g.x, g.z
                    ))
                })
                .take(12)
                .collect::<Vec<_>>();
            eprintln!("common examples: {}", common.join("; "));
            for marker in &ores.markers {
                if expected
                    .iter()
                    .all(|e| e["id"].as_str() != Some(marker.id.as_str()))
                {
                    let y = ((marker.min_y.unwrap_or_default() + marker.max_y.unwrap_or_default())
                        / 2.)
                        .floor() as i32;
                    eprintln!(
                        "EXTRA {} xyz=({}, {}, {}) rocks={:?}",
                        marker.id,
                        marker.x,
                        y,
                        marker.z,
                        world.rock_at(marker.x as i32, y, marker.z as i32)
                    );
                }
            }
        }
        assert_eq!(
            actual.len(),
            expected.len(),
            "deposit count seed={seed} truncated={}",
            ores.coverage.truncated
        );
        for want in expected {
            let id = want["id"].as_str().unwrap();
            let got = actual
                .get(id)
                .unwrap_or_else(|| panic!("missing {id} seed={seed}"));
            assert_eq!(got.x, want["x"].as_f64().unwrap(), "{id}");
            assert_eq!(got.z, want["z"].as_f64().unwrap(), "{id}");
            if let Some(y) = want["topY"].as_f64() {
                assert_eq!(got.max_y, Some(y), "top {id}");
            }
            if let Some(y) = want["bottomY"].as_f64() {
                assert_eq!(got.min_y, Some(y), "bottom {id}");
            }
        }
        let structures = features::structure_candidates_with_world(
            seed,
            bounds,
            |x, z| world.biome(x, z),
            |x, z| Some(world.climate(x, z)),
            |cx, cz| world.region.forest_type(cx, cz),
            |x, z| world.height(x, z).map(|h| h.floor() as i32),
        );
        let expected = entry["features"]["structures"].as_array().unwrap();
        structure_total += expected.len();
        let actual: std::collections::HashMap<_, _> = structures
            .markers
            .iter()
            .map(|m| (m.id.as_str(), m))
            .collect();
        assert_eq!(
            actual.len(),
            expected.len(),
            "structure count seed={seed} truncated={}",
            structures.coverage.truncated
        );
        for want in expected {
            let id = want["id"].as_str().unwrap();
            let got = actual
                .get(id)
                .unwrap_or_else(|| panic!("missing structure {id} seed={seed}"));
            assert_eq!(got.label, want["type"].as_str().unwrap(), "{id}");
            assert_eq!(got.x, want["x"].as_f64().unwrap(), "{id}");
            assert_eq!(got.z, want["z"].as_f64().unwrap(), "{id}");
        }
    }
    assert_eq!(deposit_total, 715);
    assert_eq!(structure_total, 12);
}

#[test]
fn tfg_structure_candidates_match_reference_independently() {
    let data = reference();
    for entry in data["worlds"].as_array().unwrap() {
        let seed = entry["seed"].as_str().unwrap().parse::<i64>().unwrap();
        let b = &entry["featureBounds"];
        let bounds = Bounds {
            min_x: b["minX"].as_f64().unwrap(),
            min_z: b["minZ"].as_f64().unwrap(),
            max_x: b["maxX"].as_f64().unwrap(),
            max_z: b["maxZ"].as_f64().unwrap(),
        };
        let world = World::new(seed, Settings::default());
        let got = features::structure_candidates_with_world(
            seed,
            bounds,
            |x, z| world.biome(x, z),
            |x, z| Some(world.climate(x, z)),
            |cx, cz| world.region.forest_type(cx, cz),
            |x, z| world.height(x, z).map(|h| h.floor() as i32),
        );
        let expected = entry["features"]["structures"].as_array().unwrap();
        let actual: std::collections::HashMap<_, _> =
            got.markers.iter().map(|m| (m.id.as_str(), m)).collect();
        if actual.len() != expected.len() {
            eprintln!(
                "structure mismatch seed={seed}: got={:?}; expected={:?}",
                actual.keys().collect::<Vec<_>>(),
                expected
                    .iter()
                    .filter_map(|e| e["id"].as_str())
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(
            actual.len(),
            expected.len(),
            "seed={seed} truncated={}",
            got.coverage.truncated
        );
        for want in expected {
            let id = want["id"].as_str().unwrap();
            let marker = actual
                .get(id)
                .unwrap_or_else(|| panic!("missing {id} seed={seed}"));
            assert_eq!(marker.label, want["type"].as_str().unwrap(), "{id}");
            assert_eq!(marker.x, want["x"].as_f64().unwrap(), "{id}");
            assert_eq!(marker.z, want["z"].as_f64().unwrap(), "{id}");
        }
    }
}
