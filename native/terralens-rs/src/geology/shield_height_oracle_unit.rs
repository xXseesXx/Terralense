use super::{Noise2D, build_height, tfg_shield};
use serde::Deserialize;
use std::rc::Rc;

#[derive(Deserialize)]
struct Fixture {
    format: String,
    samples: Vec<Sample>,
    constant_hotspot_branch_samples: Vec<BranchSample>,
}
#[derive(Deserialize)]
struct Sample {
    seed: i64,
    x: i32,
    z: i32,
    hotspot: f64,
    height: f64,
}
#[derive(Deserialize)]
struct BranchSample {
    seed: i64,
    x: i32,
    z: i32,
    hotspot: f64,
    threshold: f64,
    side: i8,
    height: f64,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/tfg-dormant-shield-java.json"
    ))
    .expect("valid direct installed-Java oracle fixture")
}

#[test]
fn dormant_shield_matches_installed_java_noise_for_multiple_seeds_and_signed_coordinates() {
    let fixture = fixture();
    assert_eq!(fixture.format, "terralens-shield-height-java-v1");
    assert_eq!(fixture.samples.len(), 32);
    for sample in fixture.samples {
        let field = build_height(sample.seed, "dormant_shield_volcano")
            .expect("height expression exists")
            .expect("dormant shield has a height expression");
        let actual = field(sample.x as f64, sample.z as f64);
        assert!(
            (actual - sample.height).abs() < 1e-9,
            "seed={} at ({}, {}), Java hotspot={:.15}: Rust {actual:.15}, Java {:.15}; delta={:.3e}",
            sample.seed,
            sample.x,
            sample.z,
            sample.hotspot,
            sample.height,
            (actual - sample.height).abs()
        );
    }
}

#[test]
fn dormant_shield_matches_java_at_every_piecewise_threshold() {
    let fixture = fixture();
    assert_eq!(fixture.constant_hotspot_branch_samples.len(), 96);
    for sample in fixture.constant_hotspot_branch_samples {
        let hot: Noise2D = Rc::new(move |_, _| sample.hotspot);
        let field = tfg_shield(sample.seed, hot, 1);
        let actual = field(sample.x as f64, sample.z as f64);
        assert!(
            (actual - sample.height).abs() < 1e-9,
            "seed={} at ({}, {}), threshold={:.17}, side={}: Rust {actual:.15}, Java {:.15}; delta={:.3e}",
            sample.seed,
            sample.x,
            sample.z,
            sample.threshold,
            sample.side,
            sample.height,
            (actual - sample.height).abs()
        );
    }
}
