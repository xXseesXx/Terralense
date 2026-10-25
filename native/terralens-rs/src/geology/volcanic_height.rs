//! Centered cinder-cone, tuff-ring, and tuya height adjustment from TFG Core Modern 0.9.25.
//!
//! `TFGChunkHeightFiller` applies these samplers after biome and shore blending, and before
//! river carving. Each centered sampler receives the same pre-volcano height; its results are
//! combined with `max`, exactly as the Java filler does.

use crate::{
    biome::ids as biome_ids,
    noise::{OpenSimplex2D, TfgCellular2D},
    region::Generator,
};
use lru::LruCache;
use std::{cell::RefCell, num::NonZeroUsize};

const SEA_LEVEL: f64 = 63.0;
const NOT_PRESENT: f64 = i32::MIN as f64;

#[derive(Clone, Copy)]
struct FeatureBiome {
    kind: Kind,
    rarity: i32,
    base: f64,
    scale: f64,
    icy: bool,
    active_shield: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Cinder,
    Tuff,
    Tuya,
}

fn feature_biome(id: u16) -> Option<FeatureBiome> {
    use biome_ids::id as b;
    let (kind, rarity, base, scale, icy, active_shield) = match id {
        b::CANYONS | b::DOLINE_CANYONS => (Kind::Cinder, 6, 14., 30., false, false),
        b::VOLCANIC_MOUNTAINS | b::VOLCANIC_MOUNTAIN_LAKE => {
            (Kind::Cinder, 4, 25., 50., false, false)
        }
        b::VOLCANIC_OCEANIC_MOUNTAINS | b::VOLCANIC_OCEANIC_MOUNTAIN_LAKE => {
            (Kind::Cinder, 2, -12., 50., false, false)
        }
        b::ACTIVE_SHIELD_VOLCANO => (Kind::Cinder, 4, 15., 25., false, true),
        b::DORMANT_SHIELD_VOLCANO => (Kind::Tuff, 2, 0., 36., false, false),
        b::EXTINCT_SHIELD_VOLCANO => (Kind::Tuff, 2, 0., 26., false, false),
        b::ANCIENT_SHIELD_VOLCANO => (Kind::Tuff, 3, -16., 30., false, false),
        b::SUNKEN_SHIELD_VOLCANO => (Kind::Tuff, 2, -8., 24., false, false),
        b::OLD_SHIELD_VOLCANO_SHORE => (Kind::Tuff, 3, -8., 26., false, false),
        b::ICE_SHEET_TUYAS | b::ICE_SHEET_TUYAS_EDGE => (Kind::Tuya, 3, 0., 35., true, false),
        b::TUYAS => (Kind::Tuya, 2, 0., 35., false, false),
        _ => return None,
    };
    Some(FeatureBiome {
        kind,
        rarity,
        base,
        scale,
        icy,
        active_shield,
    })
}

pub(super) struct Samplers {
    cinder_cells: TfgCellular2D,
    cinder_jitter: OpenSimplex2D,
    tuff_cells: TfgCellular2D,
    tuff_jitter: OpenSimplex2D,
    tuff_cliff: OpenSimplex2D,
    tuff_everywhere: OpenSimplex2D,
    tuya_cells: TfgCellular2D,
    tuya_jitter: OpenSimplex2D,
    tuya_added: OpenSimplex2D,
    center_biomes: RefCell<LruCache<(i32, i32), u16>>,
}

impl Samplers {
    pub(super) fn new(seed: i64) -> Self {
        let mut cinder_cells = TfgCellular2D::new(seed);
        cinder_cells.spread(0.009f32 as f64);
        let cinder_jitter = simplex(
            seed.wrapping_add(8_179_234_123),
            2,
            0.128f32 as f64,
            -0.0016f32 as f64,
            0.0016f32 as f64,
        );
        // CenteredFeatureBlendType.ALL is CINDER_CONE, TUYA, TUFF_RING. Only the
        // tuff sampler consumes Seed.next(), so this is the first nextLong from Seed.of.
        let mut tuya_cells = TfgCellular2D::configured(seed, 0.21, 1);
        tuya_cells.spread(0.0033f32 as f64);
        let tuya_jitter = simplex(
            seed.wrapping_add(1_234_123),
            2,
            0.128f32 as f64,
            -0.016f32 as f64,
            0.016f32 as f64,
        );
        let tuya_added = simplex(seed, 2, 0.1, -2., 3.);

        let mut tuff_cells = TfgCellular2D::configured(seed, 0.2, 1);
        tuff_cells.spread(0.003f32 as f64);
        let tuff_jitter = simplex(
            seed.wrapping_add(1_234_123),
            2,
            0.064f32 as f64,
            -0.032f32 as f64,
            0.032f32 as f64,
        );
        let tuff_cliff = simplex(seed, 2, 0.1, -2., 10.);
        let mut sequential_seed = crate::random::Xoroshiro::new(seed);
        let tuff_everywhere = simplex(sequential_seed.next_long(), 3, 0.03, -10., 10.);
        Self {
            cinder_cells,
            cinder_jitter,
            tuff_cells,
            tuff_jitter,
            tuff_cliff,
            tuff_everywhere,
            tuya_cells,
            tuya_jitter,
            tuya_added,
            center_biomes: RefCell::new(LruCache::new(NonZeroUsize::new(4096).unwrap())),
        }
    }

    /// Apply the TFG centered-feature contribution before the independent river-height pass.
    pub(super) fn apply(&self, region: &Generator, x: i32, z: i32, height: f64) -> f64 {
        combine(
            height,
            [
                self.cinder(region, x, z, height),
                self.tuya(region, x, z, height),
                self.tuff(region, x, z, height),
            ],
        )
    }

    fn center_biome(&self, region: &Generator, x: f64, z: f64) -> u16 {
        // Java casts the feature point to int (toward zero), then QuartPos.fromBlock (signed >> 2).
        let bx = x as i32;
        let bz = z as i32;
        let key = (bx >> 2, bz >> 2);
        if let Some(id) = self.center_biomes.borrow_mut().get(&key).copied() {
            return id;
        }
        let id = region.biome_at_quart(key.0, key.1);
        self.center_biomes.borrow_mut().put(key, id);
        id
    }

    fn cinder(&self, region: &Generator, x: i32, z: i32, height: f64) -> f64 {
        let c = self.cinder_cells.cell(x as f64, z as f64);
        let Some(cfg) =
            feature_biome(self.center_biome(region, c.x, c.y)).filter(|c| c.kind == Kind::Cinder)
        else {
            return NOT_PRESENT;
        };
        self.cinder_sample(c, x, z, height, cfg)
            .unwrap_or(NOT_PRESENT)
    }

    fn cinder_sample(
        &self,
        c: crate::noise::Cell,
        x: i32,
        z: i32,
        height: f64,
        cfg: FeatureBiome,
    ) -> Option<f64> {
        if cfg.kind != Kind::Cinder || cfg.rarity == 0 || c.noise.abs() > 1.0 / cfg.rarity as f64 {
            return None;
        }
        let f1 = c.f1;
        let easing = (map_easing(f1 as f32) as f64 + self.cinder_jitter.sample(x as f64, z as f64))
            .clamp(0., 1.);
        let shape = cinder_shape(1. - easing);
        let additional = shape * cfg.scale;
        let volcano_height = SEA_LEVEL + cfg.base + additional;
        // The active shield overload omits the Java `(float)` narrowing on f2.
        let weight_delta = if cfg.active_shield {
            c.f2 - f1
        } else {
            c.f2 as f32 as f64 - f1
        };
        let weight = 10.0f32 as f64 * weight_delta.clamp(0.0f32 as f64, 0.1f32 as f64);
        let target =
            0.2 * volcano_height + 0.8 * volcano_height.max(height + 0.6f32 as f64 * additional);
        Some(lerp(easing * weight, height, target))
    }

    fn tuff(&self, region: &Generator, x: i32, z: i32, height: f64) -> f64 {
        let c = self.tuff_cells.cell(x as f64, z as f64);
        let Some(cfg) =
            feature_biome(self.center_biome(region, c.x, c.y)).filter(|c| c.kind == Kind::Tuff)
        else {
            return NOT_PRESENT;
        };
        self.tuff_sample(c, x, z, height, cfg)
            .unwrap_or(NOT_PRESENT)
    }

    fn tuff_sample(
        &self,
        c: crate::noise::Cell,
        x: i32,
        z: i32,
        height: f64,
        cfg: FeatureBiome,
    ) -> Option<f64> {
        if cfg.kind != Kind::Tuff || cfg.rarity == 0 || c.noise.abs() > 1.0 / cfg.rarity as f64 {
            return None;
        }
        let f1 = c.f1;
        let easing = (map_easing(f1 as f32) as f64 + self.tuff_jitter.sample(x as f64, z as f64))
            .clamp(0., 1.);
        let shape = tuff_shape(1. - easing);
        let mut additional = shape * cfg.scale;
        if shape > 0.5 {
            additional += self.tuff_cliff.sample(x as f64, z as f64);
        }
        additional = (additional * gap_vertical_easing(c.noise, c.angle)).min(additional);
        let ring_height =
            SEA_LEVEL + cfg.base + additional + self.tuff_everywhere.sample(x as f64, z as f64);
        let delta = 25. * (c.f2 - f1).clamp(0., 0.04);
        Some(lerp(delta, height, ring_height.max(height)))
    }

    fn tuya(&self, region: &Generator, x: i32, z: i32, height: f64) -> f64 {
        let c = self.tuya_cells.cell(x as f64, z as f64);
        let Some(cfg) =
            feature_biome(self.center_biome(region, c.x, c.y)).filter(|c| c.kind == Kind::Tuya)
        else {
            return NOT_PRESENT;
        };
        self.tuya_sample(c, x, z, height, cfg)
            .unwrap_or(NOT_PRESENT)
    }

    fn tuya_sample(
        &self,
        c: crate::noise::Cell,
        x: i32,
        z: i32,
        height: f64,
        cfg: FeatureBiome,
    ) -> Option<f64> {
        if cfg.kind != Kind::Tuya || cfg.rarity == 0 || c.noise.abs() > 1.0 / cfg.rarity as f64 {
            return None;
        }
        let easing = (map_easing(c.f1 as f32) as f64 + self.tuya_jitter.sample(x as f64, z as f64))
            .clamp(0., 1.);
        let shape = if cfg.icy {
            icy_tuya_shape(1. - easing)
        } else {
            tuya_shape(1. - easing)
        };
        // Java explicitly narrows shape*scale + noise to float before adding sea/base height.
        let additional =
            (shape * cfg.scale + self.tuya_added.sample(x as f64, z as f64)) as f32 as f64;
        let tuya_height = SEA_LEVEL + cfg.base + additional;
        Some(lerp(
            easing,
            height,
            0.5f32 as f64 * (tuya_height + tuya_height.max(height + 0.4f32 as f64 * additional)),
        ))
    }
}

fn combine(height_in: f64, samples: [f64; 3]) -> f64 {
    let mut result = NOT_PRESENT;
    for sample in samples {
        result = result.max(sample);
    }
    if result == NOT_PRESENT {
        height_in
    } else {
        result
    }
}

fn simplex(seed: i64, octaves: u32, spread: f64, min: f64, max: f64) -> OpenSimplex2D {
    let mut n = OpenSimplex2D::new(seed);
    n.octaves(octaves).scaled(min, max).spread(spread);
    n
}

fn map_easing(f1: f32) -> f32 {
    // Mth.map(f1, 0, 0.23f, 1, 0), with Java float arithmetic.
    ((f1 - 0.0f32) / (0.23f32 - 0.0f32)) * (0.0f32 - 1.0f32) + 1.0f32
}
fn cinder_shape(t: f64) -> f64 {
    if t > 0.025 {
        (5. / (9. * t + 1.) - 0.5) * 0.279173646008
    } else {
        let a = t * 9. + 0.05;
        (8. * a * a + 2.97663265306) * 0.279173646008
    }
}
fn tuff_shape(t: f64) -> f64 {
    if t < 0.03f32 as f64 {
        0.
    } else if t < 0.10f32 as f64 {
        t * 7f32 as f64 - 0.21f32 as f64
    } else if t < 0.15f32 as f64 {
        t * 6.6667f32 as f64
    } else if t < 0.20f32 as f64 {
        1. - (t - 0.15f32 as f64) * 6.6667f32 as f64
    } else {
        1.5f32 as f64 - 5. * t
    }
}
fn tuya_shape(t: f64) -> f64 {
    if t < 0.015f32 as f64 {
        map(t, 0., 0.015f32 as f64, 0.8f32 as f64, 1.)
    } else if t < 0.125f32 as f64 {
        map(t, 0.025f32 as f64, 0.125f32 as f64, 1., 0.75f32 as f64)
    } else {
        clamped_map(t, 0.125f32 as f64, 0.16f32 as f64, 0.75f32 as f64, 0.)
    }
}
fn icy_tuya_shape(t: f64) -> f64 {
    if t < 0.015f32 as f64 {
        map(t, 0., 0.015f32 as f64, 0.8f32 as f64, 1.)
    } else if t < 0.125f32 as f64 {
        map(t, 0.025f32 as f64, 0.125f32 as f64, 1., 0.75f32 as f64)
    } else if t < 0.16 {
        map(t, 0.125f32 as f64, 0.16f32 as f64, 0.75f32 as f64, 0.)
    } else {
        clamped_map(t, 0.16f32 as f64, 0.19f32 as f64, 0., 0.5f32 as f64)
    }
}
fn map(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    c + ((v - a) / (b - a)) * (d - c)
}
fn clamped_map(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    c + ((v - a) / (b - a)).clamp(0., 1.) * (d - c)
}
fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + (b - a) * t
}

fn gap_vertical_easing(noise: f64, angle: f64) -> f64 {
    // Java `%` keeps the sign of the dividend; negative hash noise means no gap.
    let gap_size = ((1. + noise) * 100.) % 1.;
    let mut easing = 1.;
    if gap_size > 0. {
        let gap_angle = 4. * ((noise.abs() * 10_000.) % 1.);
        let angle_to_gap = (angle - gap_angle).abs();
        if angle_to_gap < gap_size {
            let edge = (angle_to_gap + gap_size).min(angle_to_gap - gap_size).abs();
            easing = clamped_map(edge, 0., 0.3f64.max(0.6 * gap_size), 1., 0.);
        }
    }
    easing
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::Value;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct JavaFixture {
        samples: Vec<Value>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct JavaSample {
        family: String,
        seed: i64,
        x: i32,
        z: i32,
        pre_height: f64,
        raw_height: f64,
        present: bool,
        applied_height: f64,
        biome: JavaBiome,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct JavaBiome {
        has_cinder_cones: bool,
        has_tuff_rings: bool,
        has_tuyas: bool,
        rarity: i32,
        base_height: f64,
        scale_height: f64,
        icy: bool,
        is_active_shield_volcano: bool,
    }

    #[test]
    fn centered_sampler_matches_original_mapped_java_oracle() {
        let fixture: JavaFixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/tfg-centered-volcanic-java.json"
        ))
        .expect("mapped Java oracle fixture");
        let mut samplers = HashMap::<i64, Samplers>::new();
        let mut count = 0;
        for value in fixture.samples.iter().filter(|s| s["family"] != "shore") {
            let sample: JavaSample = serde_json::from_value(value.clone()).unwrap();
            let sampler = samplers
                .entry(sample.seed)
                .or_insert_with(|| Samplers::new(sample.seed));
            let cfg = FeatureBiome {
                kind: match sample.family.as_str() {
                    "cinder" => Kind::Cinder,
                    "tuff" => Kind::Tuff,
                    "tuya" => Kind::Tuya,
                    other => panic!("unknown oracle family {other}"),
                },
                rarity: sample.biome.rarity,
                base: sample.biome.base_height,
                scale: sample.biome.scale_height,
                icy: sample.biome.icy,
                active_shield: sample.biome.is_active_shield_volcano,
            };
            let enabled = match cfg.kind {
                Kind::Cinder => sample.biome.has_cinder_cones,
                Kind::Tuff => sample.biome.has_tuff_rings,
                Kind::Tuya => sample.biome.has_tuyas,
            };
            let result = if !enabled {
                None
            } else {
                let cell = match cfg.kind {
                    Kind::Cinder => sampler.cinder_cells.cell(sample.x as f64, sample.z as f64),
                    Kind::Tuff => sampler.tuff_cells.cell(sample.x as f64, sample.z as f64),
                    Kind::Tuya => sampler.tuya_cells.cell(sample.x as f64, sample.z as f64),
                };
                match cfg.kind {
                    Kind::Cinder => {
                        sampler.cinder_sample(cell, sample.x, sample.z, sample.pre_height, cfg)
                    }
                    Kind::Tuff => {
                        sampler.tuff_sample(cell, sample.x, sample.z, sample.pre_height, cfg)
                    }
                    Kind::Tuya => {
                        sampler.tuya_sample(cell, sample.x, sample.z, sample.pre_height, cfg)
                    }
                }
            };
            assert_eq!(
                result.is_some(),
                sample.present,
                "{} seed={} at ({}, {})",
                sample.family,
                sample.seed,
                sample.x,
                sample.z
            );
            // The Java filler max-combines the configured family samplers. It uses heightIn only
            // when every sampler returns NOT_PRESENT_RETURN.
            let applied = result.unwrap_or(sample.pre_height);
            assert!(
                (applied - sample.applied_height).abs() < 1e-9,
                "{} seed={} at ({}, {}): Rust {applied:.15}, Java {:.15}",
                sample.family,
                sample.seed,
                sample.x,
                sample.z,
                sample.applied_height
            );
            if let Some(raw) = result {
                assert!(
                    (raw - sample.raw_height).abs() < 1e-9,
                    "{} seed={} at ({}, {}): Rust raw {raw:.15}, Java {:.15}",
                    sample.family,
                    sample.seed,
                    sample.x,
                    sample.z,
                    sample.raw_height
                );
            }
            count += 1;
        }
        assert_eq!(count, 1320, "oracle must keep its 1,320 centered samples");
    }

    #[test]
    fn source_formula_shapes_match_tfg_boundaries() {
        // Samples are maintained beside the exact upstream tag and method names.
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/tfg-centered-volcanic-height-source.json"
        ))
        .unwrap();
        for sample in fixture["formula_samples"].as_array().unwrap() {
            let formula = sample["formula"].as_str().unwrap();
            let input = sample["input"].as_f64().unwrap();
            let expected = sample["expected"].as_f64().unwrap();
            let actual = match formula {
                "cinder" => cinder_shape(input),
                "tuff" => tuff_shape(input),
                "tuya" => tuya_shape(input),
                "tuya_icy" => icy_tuya_shape(input),
                _ => panic!("fixture formula must be recognized: {formula}"),
            };
            assert!(
                (actual - expected).abs() <= 1e-7,
                "{formula}({input}) = {actual}, expected {expected}"
            );
        }
        assert_eq!(gap_vertical_easing(0.0, 2.0), 1.0);
    }

    #[test]
    fn java_max_combine_preserves_lower_feature_height_and_falls_back_only_when_absent() {
        assert_eq!(
            combine(92.0, [91.61793311369063, NOT_PRESENT, NOT_PRESENT]),
            91.61793311369063
        );
        assert_eq!(combine(40.0, [30.0, 42.0, 38.0]), 42.0);
        assert_eq!(combine(-12.5, [NOT_PRESENT; 3]), -12.5);
    }

    #[test]
    fn centered_biome_flags_only_enable_source_declared_families() {
        assert_eq!(
            feature_biome(biome_ids::id::CANYONS).unwrap().kind,
            Kind::Cinder
        );
        assert_eq!(
            feature_biome(biome_ids::id::DORMANT_SHIELD_VOLCANO)
                .unwrap()
                .kind,
            Kind::Tuff
        );
        assert_eq!(
            feature_biome(biome_ids::id::ICE_SHEET_TUYAS).unwrap().kind,
            Kind::Tuya
        );
        assert!(feature_biome(biome_ids::id::PLAINS).is_none());
        assert!(feature_biome(biome_ids::id::SHIELD_VOLCANO_SHORE).is_none());
    }
}
