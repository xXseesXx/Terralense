use serde_json::Value;
use terralens::{ground, model::Settings, world::World};

#[test]
fn rocky_plateau_exposes_the_saved_raw_rhyolite_surface() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/ground/rocky-plateau-rock-seed0-1550-704.json"
    ))
    .unwrap();
    assert_eq!(fixture["provenance"]["seed"], 0);
    assert_eq!(
        fixture["provenance"]["chunk"],
        serde_json::json!([1550, 704])
    );
    assert_eq!(
        fixture["biome_palette"],
        serde_json::json!(["tfg:earth/rocky_plateau"])
    );
    let columns = fixture["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 256);
    let world = World::new(0, Settings::default());
    for column in columns {
        let x = 1550 * 16 + column["x"].as_i64().unwrap() as i32;
        let z = 704 * 16 + column["z"].as_i64().unwrap() as i32;
        let actual_y = column["ground_y"].as_i64().unwrap() as i32;
        let actual = column["ground_block"].as_str().unwrap();
        let predicted = ground::sample(&world, x, z).unwrap();
        assert_eq!(actual, "tfc:rock/raw/rhyolite", "reference {x},{z}");
        assert_eq!(
            (predicted.y, predicted.block.as_ref()),
            (actual_y, actual),
            "predicted surface at {x},{z}"
        );
    }
}

#[test]
fn rocky_plateau_preserves_its_soil_and_mixed_rock_reference() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/ground/rocky-plateau-seed0-1536-704.json"
    ))
    .unwrap();
    assert_eq!(fixture["provenance"]["seed"], 0);
    assert_eq!(
        fixture["provenance"]["chunk"],
        serde_json::json!([1536, 704])
    );
    assert_eq!(
        fixture["biome_palette"],
        serde_json::json!(["tfg:earth/rocky_plateau"])
    );
    let columns = fixture["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 256);
    let world = World::new(0, Settings::default());
    for column in columns {
        let x = 1536 * 16 + column["x"].as_i64().unwrap() as i32;
        let z = 704 * 16 + column["z"].as_i64().unwrap() as i32;
        let actual_y = column["ground_y"].as_i64().unwrap() as i32;
        let actual = column["ground_block"].as_str().unwrap();
        let predicted = ground::sample(&world, x, z).unwrap();
        assert_eq!(
            (predicted.y, predicted.block.as_ref()),
            (actual_y, actual),
            "predicted surface at {x},{z}"
        );
    }
}
