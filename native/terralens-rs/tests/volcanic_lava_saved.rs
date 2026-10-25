use serde::Deserialize;
use terralens::{model::Settings, world::World};

#[derive(Deserialize)]
struct Fixture {
    sites: Vec<Site>,
}
#[derive(Deserialize)]
struct Site {
    family: String,
    columns: Vec<(i32, i32, i32, String, bool, String)>,
}

#[test]
fn tuya_caldera_lava_matches_independent_saved_surface_observations() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "fixtures/volcanic-terrain/seed0-surface-fluids.json"
    ))
    .unwrap();
    let world = World::new(0, Settings::default());
    assert_eq!(fixture.sites.len(), 5);
    let sites: Vec<_> = fixture
        .sites
        .into_iter()
        .filter(|site| matches!(site.family.as_str(), "tuya" | "glacial_tuya"))
        .collect();
    assert_eq!(sites.len(), 2);
    for site in sites {
        assert_eq!(site.columns.len(), 2304);
        let (mut tp, mut fp, mut fn_, mut wrong_y) = (0, 0, 0, 0);
        let mut levels = std::collections::BTreeMap::new();
        for (x, z, actual_y, _, lava, _) in site.columns {
            let modeled = world.surface_lava_y(x, z);
            if let Some(y) = modeled {
                *levels.entry(y).or_insert(0) += 1;
            }
            match (modeled, lava) {
                (Some(y), true) => {
                    tp += 1;
                    wrong_y += usize::from(y != actual_y);
                }
                (Some(_), false) => fp += 1,
                (None, true) => fn_ += 1,
                (None, false) => {}
            }
        }
        eprintln!(
            "{}: TP={tp} FP={fp} FN={fn_} wrong_y={wrong_y} levels={levels:?}",
            site.family
        );
        if matches!(site.family.as_str(), "tuya" | "glacial_tuya") {
            assert!(tp > 0, "{} must expose its saved caldera", site.family);
            let expected_lava = if site.family == "tuya" { 173 } else { 422 };
            assert_eq!(
                tp + fn_,
                expected_lava,
                "saved lava denominator must stay intact"
            );
            assert_eq!(wrong_y, 0, "exposed caldera elevations must match the save");
            assert!(
                tp * 100 >= (tp + fn_) * 99,
                "{} lava recall fell below 99%",
                site.family
            );
            assert!(
                tp * 100 >= (tp + fp) * 99,
                "{} lava precision fell below 99%",
                site.family
            );
        } else {
            assert_eq!(
                tp + fp,
                0,
                "{} has no lava in this saved sample",
                site.family
            );
        }
    }
}
