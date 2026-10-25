//! Independent World 0 blocks, never supplied to the seed-only World sampler.
use flate2::read::GzDecoder;
use serde_json::Value;
use std::{collections::BTreeMap, io::Read};
use terralens::{
    caves::{self, Cell},
    ground,
    model::Settings,
    world::World,
};

fn name(cell: Cell) -> &'static str {
    match cell {
        Cell::Solid => "solid",
        Cell::Air => "air",
        Cell::Water => "water",
        Cell::Lava => "lava",
    }
}

#[test]
fn independent_saved_voxels_retain_cave_and_aquifer_coverage() {
    let mut text = String::new();
    GzDecoder::new(
        &include_bytes!("fixtures/generator-accuracy/world0-surface-profiles-48.json.gz")[..],
    )
    .read_to_string(&mut text)
    .unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    let world = World::new(0, Settings::default());
    let mut confusion = BTreeMap::<(String, String), usize>::new();
    for chunk in fixture["chunks"].as_array().unwrap() {
        let cx = chunk["chunk"][0].as_i64().unwrap() as i32;
        let cz = chunk["chunk"][1].as_i64().unwrap() as i32;
        for column in chunk["vertical_lattice_4x4"].as_array().unwrap() {
            let x = cx * 16 + column["local"][0].as_i64().unwrap() as i32;
            let z = cz * 16 + column["local"][1].as_i64().unwrap() as i32;
            for run in column["rle"].as_array().unwrap() {
                let actual = run[2].as_str().unwrap();
                if actual == "unknown" || actual == "excluded_snow_ice" {
                    continue;
                }
                let start = run[0].as_i64().unwrap() as i32;
                let end = start + run[1].as_i64().unwrap() as i32;
                for y in start..end {
                    let got = name(caves::state_at(&world, x, y, z).unwrap());
                    *confusion.entry((actual.into(), got.into())).or_default() += 1;
                }
            }
        }
    }
    let total = confusion.values().sum::<usize>();
    let wrong = confusion
        .iter()
        .filter(|((a, b), _)| a != b)
        .map(|(_, n)| n)
        .sum::<usize>();
    assert_eq!(total, 76_770);
    assert!(wrong <= 520, "voxel mismatches {wrong}: {confusion:?}");
    // Keep rare classes visible: a solid-dominated aggregate cannot satisfy
    // these recall checks by filling all caves back in.
    for class in ["air", "water"] {
        let actual = confusion
            .iter()
            .filter(|((a, _), _)| a == class)
            .map(|(_, n)| n)
            .sum::<usize>();
        let matched = *confusion.get(&(class.into(), class.into())).unwrap_or(&0);
        assert!(
            matched as f64 / actual as f64 > 0.995,
            "{class} recall: {matched}/{actual}"
        );
    }
}

#[test]
fn river_undercut_exposes_the_saved_sandstone_floor() {
    let world = World::new(0, Settings::default());
    // Saved whorled-canyon chunk (-7720,191), independently decoded in fixture.
    for (x, z, raw, y, block) in [
        (-123506, 3059, 82, 74, "tfc:raw_sandstone/red"),
        (-123505, 3059, 82, 72, "tfc:raw_sandstone/yellow"),
        (-123510, 3060, 82, 76, "tfc:raw_sandstone/yellow"),
    ] {
        assert_eq!(world.height(x, z), Some(raw as f64));
        let column = caves::column(&world, x, z).unwrap();
        assert_eq!(column.solid_y, y);
        assert!(column.fluid.is_none());
        assert_eq!(caves::state_at(&world, x, y + 1, z), Some(Cell::Air));
        let ground = ground::sample(&world, x, z).unwrap();
        assert_eq!((ground.y, ground.block.as_ref()), (y, block));
    }
}

#[test]
fn physical_columns_stay_seed_and_settings_specific() {
    let a = World::new(0, Settings::default());
    let b = World::new(1, Settings::default());
    let first = caves::column(&a, -123505, 3059);
    let other = caves::column(&b, -123505, 3059);
    assert_ne!(first, other);
    assert_eq!(caves::column(&a, -123505, 3059), first);
}

#[test]
fn saved_emergent_boulder_is_not_hidden_by_the_earlier_water_column() {
    let world = World::new(0, Settings::default());
    let (x, z) = (8856, 25749);
    let terrain = caves::column(&world, x, z).unwrap();
    assert_eq!(terrain.fluid, Some((Cell::Water, 62)));
    let ground = ground::sample(&world, x, z).unwrap();
    assert_eq!(ground.y, 63);
    assert_eq!(ground.block.as_ref(), "tfc:rock/cobble/chalk");
    assert!(!world.is_surface_water(x, z));
    assert!(!world.is_surface_fluid(x, z));
    assert_eq!(world.surface_water_y(x, z), None);
}

#[test]
fn steep_river_banks_and_old_volcanic_shores_use_their_source_builders() {
    let world = World::new(0, Settings::default());
    for (x, z, y, block) in [
        (-130031, -22048, 63, "tfc:rock/gravel/basalt"),
        (15134, 14793, 63, "tfc:rock/gravel/rhyolite"),
        (-99478, -21584, 68, "tfc:rock/raw/basalt"),
        // These stored Y=64 banks have a fractional height above 64.0. TFG
        // retains their land builder instead of applying the river gravel rule.
        (-130032, -22048, 64, "tfc:sand/red"),
        (15135, 14792, 64, "tfc:grass/silty_loam"),
    ] {
        let ground = ground::sample(&world, x, z).unwrap();
        assert_eq!(
            (ground.y, ground.block.as_ref()),
            (y, block),
            "saved column {x},{z}"
        );
    }
}
