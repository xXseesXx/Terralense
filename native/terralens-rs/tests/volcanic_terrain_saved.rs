use serde::Deserialize;
use terralens::{model::Settings, world::World};

#[derive(Deserialize)]
struct Fixture {
    sites: Vec<Site>,
}

#[derive(Deserialize)]
struct Site {
    candidate: String,
    status: String,
    center: Center,
    columns: Vec<Column>,
    cross_samples: Vec<Column>,
    no_overlay_reference_mae: f64,
}

#[derive(Deserialize)]
struct Center {
    x: i32,
    z: i32,
}

#[derive(Deserialize)]
struct Column {
    x: i32,
    z: i32,
    terrain_noise_height: i32,
    world_surface_top_y: i32,
    top_block: Option<String>,
    ground_below_surface_fluid_block: Option<String>,
    saved_surface_biome: Option<String>,
}

fn is_fluid(block: Option<&str>) -> bool {
    matches!(
        block,
        Some(
            "minecraft:water"
                | "minecraft:lava"
                | "minecraft:flowing_water"
                | "minecraft:flowing_lava"
        )
    ) || block.is_some_and(|id| id.starts_with("tfc:fluid/"))
}

#[test]
fn saved_volcanic_noise_heights_match_generated_columns() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "fixtures/volcanic-terrain/seed0-generated-columns.json"
    ))
    .unwrap();
    let world = World::new(0, Settings::default());
    assert_eq!(
        fixture.sites.len(),
        5,
        "fixture must cover five volcanic families"
    );

    for site in fixture.sites {
        assert_eq!(
            site.status, "minecraft:full",
            "saved sample chunk must be complete"
        );
        assert_eq!(
            site.columns.len(),
            256,
            "center chunk must cover all 256 columns"
        );
        assert!(
            !site.cross_samples.is_empty(),
            "site needs saved rim/control columns"
        );

        let mut noise_errors = Vec::new();
        let mut surface_vs_noise_errors = Vec::new();
        let mut fluid_top_columns = 0;
        for column in site.columns.iter().chain(&site.cross_samples) {
            assert!(
                column.saved_surface_biome.is_some(),
                "saved biome should be present at each sampled surface column"
            );
            if is_fluid(column.top_block.as_deref()) {
                fluid_top_columns += 1;
                assert!(
                    !is_fluid(column.ground_below_surface_fluid_block.as_deref()),
                    "fluid tops must be skipped when recording ground below the surface fluid"
                );
            }
            let height = world
                .height(column.x, column.z)
                .expect("terrain height is available");
            assert!(
                height.is_finite(),
                "non-finite terrain at {},{}",
                column.x,
                column.z
            );
            let predicted = height as i32;
            assert_eq!(
                predicted, column.terrain_noise_height,
                "{}: generated terrain-noise height at {},{}",
                site.candidate, column.x, column.z,
            );
            noise_errors.push(f64::from((predicted - column.terrain_noise_height).abs()));
            surface_vs_noise_errors.push(f64::from(
                (column.world_surface_top_y - column.terrain_noise_height).abs(),
            ));
        }
        noise_errors.sort_by(f64::total_cmp);
        surface_vs_noise_errors.sort_by(f64::total_cmp);
        let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
        let p95 = |values: &[f64]| values[((values.len() - 1) as f64 * 0.95).floor() as usize];
        eprintln!(
            "{} ({},{}) n={} fluid_top_columns={} predicted-vs-noise MAE={:.3} P95={:.3} max={:.3}; saved-top-vs-noise MAE={:.3} P95={:.3} max={:.3}",
            site.candidate,
            site.center.x,
            site.center.z,
            noise_errors.len(),
            fluid_top_columns,
            mean(&noise_errors),
            p95(&noise_errors),
            noise_errors.last().unwrap(),
            mean(&surface_vs_noise_errors),
            p95(&surface_vs_noise_errors),
            surface_vs_noise_errors.last().unwrap(),
        );
        assert!(
            mean(&noise_errors) < site.no_overlay_reference_mae,
            "{} overlay should improve mean height error against its recorded pre-overlay reference",
            site.candidate,
        );
    }
}
