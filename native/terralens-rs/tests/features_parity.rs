//! Ore placement and structure witnesses are compared with the checked-in OutCrop TFG
//! generator snapshot under controlled terrain inputs.
//! Structure witnesses retain exact placement and selected-child checks, while current
//! populations also enforce current TFG Core 0.9.25 Lithostitched spawn conditions.
//! Counts from earlier native versions that omitted those rules are not accuracy witnesses.
//! This conversion snapshot predates the independent TFG surface-height port. Ore placement
//! uses permissive height and unknown host-rock inputs; structure witnesses use a constant
//! eligible height. This isolates placement/RNG/biome parity from terrain-gate accuracy.
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

fn current_structure_id(witness: &Value) -> String {
    let old_id = witness["id"].as_str().expect("fixture structure id");
    let chunk = old_id.rsplit_once('@').expect("fixture chunk identity").1;
    let selected_child = witness["type"].as_str().expect("fixture selected child");
    format!("{selected_child}@{chunk}")
}
#[test]
fn tfg_structure_and_ore_witnesses_match_reference_windows() {
    let data = reference();
    let vein_definitions: Value =
        serde_json::from_str(include_str!("../assets/data/tfg/veins.json")).unwrap();
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
            |_, _| Some(320),
            |_, _, _| None,
        );
        let expected = entry["features"]["deposits"].as_array().unwrap();
        deposit_total += expected.len();
        let actual: std::collections::HashMap<_, _> =
            ores.markers.iter().map(|m| (m.id.as_str(), m)).collect();
        assert!(
            !ores.coverage.truncated,
            "captured ore witnesses require a complete scan, seed={seed}"
        );
        // This is the immutable snapshot's witness set, not a claim that the changed
        // terrain/host-rock model yields identical runtime candidate populations.
        for want in expected {
            let id = want["id"].as_str().unwrap();
            let got = actual
                .get(id)
                .unwrap_or_else(|| panic!("missing {id} seed={seed}"));
            assert_eq!(got.x, want["x"].as_f64().unwrap(), "{id}");
            assert_eq!(got.z, want["z"].as_f64().unwrap(), "{id}");
            if let (Some(top), Some(bottom)) = (want["topY"].as_f64(), want["bottomY"].as_f64()) {
                let vein_id = id.split('@').next().unwrap();
                if vein_definitions["veins"][vein_id]["project"].as_bool() == Some(true) {
                    // OutCrop's projected center used its old height model. Keep the
                    // captured vein extent and let the controlled terrain set the center.
                    assert_eq!(
                        got.max_y.unwrap() - got.min_y.unwrap(),
                        top - bottom,
                        "extent {id}"
                    );
                } else {
                    assert_eq!(got.max_y, Some(top), "top {id}");
                    assert_eq!(got.min_y, Some(bottom), "bottom {id}");
                }
            }
        }
        let eligible_height = |_: i32, _: i32| Some(80);
        let structures = features::structure_candidates_with_world(
            seed,
            bounds,
            |x, z| world.biome(x, z),
            |x, z| Some(world.climate(x, z)),
            |cx, cz| world.region.forest_type(cx, cz),
            eligible_height,
        );
        let expected = entry["features"]["structures"].as_array().unwrap();
        structure_total += expected.len();
        let actual: std::collections::HashMap<_, _> = structures
            .markers
            .iter()
            .map(|m| (m.id.as_str(), m))
            .collect();
        assert!(
            actual.len() >= expected.len(),
            "current candidate count unexpectedly fell below legacy witnesses seed={seed} truncated={}",
            structures.coverage.truncated
        );
        for want in expected {
            let id = current_structure_id(want);
            assert!(
                terralens::structure_support::enabled(
                    terralens::dimension::Dimension::Overworld,
                    want["type"].as_str().unwrap()
                ),
                "fixture witness is no longer source eligible: {id}"
            );
            let got = actual
                .get(id.as_str())
                .unwrap_or_else(|| panic!("missing structure witness {id} seed={seed}"));
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
        let eligible_height = |_: i32, _: i32| Some(80);
        let got = features::structure_candidates_with_world(
            seed,
            bounds,
            |x, z| world.biome(x, z),
            |x, z| Some(world.climate(x, z)),
            |cx, cz| world.region.forest_type(cx, cz),
            eligible_height,
        );
        let expected = entry["features"]["structures"].as_array().unwrap();
        let actual: std::collections::HashMap<_, _> =
            got.markers.iter().map(|m| (m.id.as_str(), m)).collect();
        assert!(
            actual.len() >= expected.len(),
            "seed={seed} truncated={}",
            got.coverage.truncated
        );
        for want in expected {
            let id = current_structure_id(want);
            assert!(
                terralens::structure_support::enabled(
                    terralens::dimension::Dimension::Overworld,
                    want["type"].as_str().unwrap()
                ),
                "fixture witness is no longer source eligible: {id}"
            );
            let marker = actual
                .get(id.as_str())
                .unwrap_or_else(|| panic!("missing structure witness {id} seed={seed}"));
            assert_eq!(marker.label, want["type"].as_str().unwrap(), "{id}");
            assert_eq!(marker.x, want["x"].as_f64().unwrap(), "{id}");
            assert_eq!(marker.z, want["z"].as_f64().unwrap(), "{id}");
        }
    }
}
