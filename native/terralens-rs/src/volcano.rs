//! TFG centered volcanic features, exposed as stable map landmarks.
//!
//! Positions use the same seeded cellular centers as TFG's cinder-cone placement.
//! Regional shield volcano terrain and hotspot ages are generated elsewhere; this
//! module only indexes the discrete cinder cones, tuff rings, and tuyas placed there.

use crate::{
    dimension::Dimension,
    features::{FeatureBatch, FeatureCoverage},
    model::{Bounds, Marker, MarkerKind},
    noise::TfgCellular2D,
    search,
    world::World,
};
use std::collections::HashSet;
const CINDER_FREQUENCY: f64 = 0.009_f32 as f64;
const TUFF_FREQUENCY: f64 = 0.003_f32 as f64;
const TUYA_FREQUENCY: f64 = 0.0033_f32 as f64;
const MAX_CELL_CHECKS: usize = 100_000;
const WORLD_BORDER: f64 = 29_999_984.0;

const CATALOG: &[(&str, &str)] = &[
    ("terralens:volcano/active_shield", "Active shield vents"),
    ("terralens:volcano/cinder_cone", "Cinder cones"),
    (
        "terralens:volcano/dormant_shield",
        "Dormant volcanic craters",
    ),
    (
        "terralens:volcano/extinct_shield",
        "Extinct volcanic craters",
    ),
    (
        "terralens:volcano/ancient_shield",
        "Ancient volcanic craters",
    ),
    ("terralens:volcano/sunken_shield", "Sunken volcanic craters"),
    ("terralens:volcano/shore_shield", "Eroded volcanic craters"),
    ("terralens:volcano/tuya", "Tuyas"),
    ("terralens:volcano/glacial_tuya", "Glacial tuyas"),
];

#[derive(Clone, Copy)]
struct FeatureType {
    id: &'static str,
    label: &'static str,
    detail: &'static str,
    color: [u8; 3],
    max_blocks_per_point: f64,
}

const ACTIVE_SHIELD: FeatureType = FeatureType {
    id: "terralens:volcano/active_shield",
    label: "Active volcano vent",
    detail: "active shield volcano · volcanic vent · caldera",
    color: [231, 91, 48],
    max_blocks_per_point: 128.0,
};
const CINDER_CONE: FeatureType = FeatureType {
    id: "terralens:volcano/cinder_cone",
    label: "Cinder cone",
    detail: "cinder cone · volcanic vent · crater",
    color: [192, 93, 48],
    max_blocks_per_point: 64.0,
};
const DORMANT_SHIELD: FeatureType = FeatureType {
    id: "terralens:volcano/dormant_shield",
    label: "Dormant volcanic crater",
    detail: "tuff ring · dormant shield · crater",
    color: [151, 135, 113],
    max_blocks_per_point: 128.0,
};
const EXTINCT_SHIELD: FeatureType = FeatureType {
    id: "terralens:volcano/extinct_shield",
    label: "Extinct volcanic crater",
    detail: "tuff ring · extinct shield · crater",
    color: [119, 119, 111],
    max_blocks_per_point: 128.0,
};
const ANCIENT_SHIELD: FeatureType = FeatureType {
    id: "terralens:volcano/ancient_shield",
    label: "Ancient volcanic crater",
    detail: "tuff ring · ancient shield · crater",
    color: [185, 145, 94],
    max_blocks_per_point: 256.0,
};
const SUNKEN_SHIELD: FeatureType = FeatureType {
    id: "terralens:volcano/sunken_shield",
    label: "Sunken volcanic crater",
    detail: "tuff ring · sunken shield · crater",
    color: [91, 126, 138],
    max_blocks_per_point: 128.0,
};
const SHIELD_SHORE: FeatureType = FeatureType {
    id: "terralens:volcano/shore_shield",
    label: "Eroded volcanic crater",
    detail: "tuff ring · old shield volcanic shore · crater",
    color: [105, 124, 121],
    max_blocks_per_point: 256.0,
};
const TUYA: FeatureType = FeatureType {
    id: "terralens:volcano/tuya",
    label: "Tuya",
    detail: "tuya · subglacial volcanic landform · volcanic vent",
    color: [151, 167, 157],
    max_blocks_per_point: 128.0,
};
const GLACIAL_TUYA: FeatureType = FeatureType {
    id: "terralens:volcano/glacial_tuya",
    label: "Glacial tuya",
    detail: "tuya · ice-sheet feature · volcanic vent",
    color: [173, 207, 223],
    max_blocks_per_point: 256.0,
};

pub fn catalog() -> &'static [(&'static str, &'static str)] {
    CATALOG
}

/// Stable type check that also accepts occurrence IDs with a `@x,z` suffix.
pub fn is_volcano_id(id: &str) -> bool {
    id.split('@')
        .next()
        .is_some_and(|id| id.starts_with("terralens:volcano/"))
}

pub fn max_blocks_per_point(id: &str) -> f64 {
    feature_type(id).map_or(0.0, |feature| feature.max_blocks_per_point)
}

fn feature_type(id: &str) -> Option<&'static FeatureType> {
    let id = id.split('@').next().unwrap_or(id);
    [
        &ACTIVE_SHIELD,
        &CINDER_CONE,
        &DORMANT_SHIELD,
        &EXTINCT_SHIELD,
        &ANCIENT_SHIELD,
        &SUNKEN_SHIELD,
        &SHIELD_SHORE,
        &TUYA,
        &GLACIAL_TUYA,
    ]
    .into_iter()
    .find(|feature| feature.id == id)
}

#[derive(Clone, Copy)]
enum Family {
    Cinder,
    Tuff,
    Tuya,
}

fn type_for_center_biome(biome: u16, family: Family) -> Option<(&'static FeatureType, u8)> {
    use crate::biome::ids::id as i;
    match (family, biome) {
        (Family::Cinder, i::ACTIVE_SHIELD_VOLCANO) => Some((&ACTIVE_SHIELD, 4)),
        (Family::Cinder, i::CANYONS | i::DOLINE_CANYONS) => Some((&CINDER_CONE, 6)),
        (Family::Cinder, i::VOLCANIC_MOUNTAINS | i::VOLCANIC_MOUNTAIN_LAKE) => {
            Some((&CINDER_CONE, 4))
        }
        (Family::Cinder, i::VOLCANIC_OCEANIC_MOUNTAINS | i::VOLCANIC_OCEANIC_MOUNTAIN_LAKE) => {
            Some((&CINDER_CONE, 2))
        }
        (Family::Tuff, i::DORMANT_SHIELD_VOLCANO) => Some((&DORMANT_SHIELD, 2)),
        (Family::Tuff, i::EXTINCT_SHIELD_VOLCANO) => Some((&EXTINCT_SHIELD, 2)),
        (Family::Tuff, i::ANCIENT_SHIELD_VOLCANO) => Some((&ANCIENT_SHIELD, 3)),
        (Family::Tuff, i::SUNKEN_SHIELD_VOLCANO) => Some((&SUNKEN_SHIELD, 2)),
        (Family::Tuff, i::OLD_SHIELD_VOLCANO_SHORE) => Some((&SHIELD_SHORE, 3)),
        (Family::Tuya, i::TUYAS) => Some((&TUYA, 2)),
        (Family::Tuya, i::ICE_SHEET_TUYAS | i::ICE_SHEET_TUYAS_EDGE) => Some((&GLACIAL_TUYA, 3)),
        _ => None,
    }
}

fn valid_bounds(bounds: Bounds) -> bool {
    bounds.min_x.is_finite()
        && bounds.min_z.is_finite()
        && bounds.max_x.is_finite()
        && bounds.max_z.is_finite()
        && bounds.min_x <= bounds.max_x
        && bounds.min_z <= bounds.max_z
        && bounds.min_x >= -WORLD_BORDER
        && bounds.min_z >= -WORLD_BORDER
        && bounds.max_x <= WORLD_BORDER
        && bounds.max_z <= WORLD_BORDER
}

/// Enumerate exact lattice feature points used by TFG's cinder, tuff-ring and tuya samplers.
/// Each family has its own spread and jitter. Shield volcano age comes from center-biome
/// gating; points are seed-derived and stable across pans, zooms and adjacent tiles.
pub fn candidates(
    world: &World,
    bounds: Bounds,
    query: &str,
    visible_bpp: Option<f64>,
    cancel: &impl Fn() -> bool,
) -> FeatureBatch {
    if world.dimension != Dimension::Overworld || !valid_bounds(bounds) {
        return FeatureBatch::default();
    }
    if cancel() {
        return cancelled_batch();
    }
    let max_bpp = CATALOG
        .iter()
        .map(|(id, _)| max_blocks_per_point(id))
        .fold(0.0, f64::max);
    if visible_bpp.is_some_and(|bpp| !bpp.is_finite() || bpp > max_bpp) {
        return FeatureBatch::default();
    }

    let mut markers = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut scanned = 0usize;
    let mut truncated = false;
    let mut requested_total = 0u64;
    for (family, frequency, sampler) in [
        (
            Family::Cinder,
            CINDER_FREQUENCY,
            TfgCellular2D::new(world.seed),
        ),
        (
            Family::Tuff,
            TUFF_FREQUENCY,
            TfgCellular2D::configured(world.seed, 0.2, 1),
        ),
        (
            Family::Tuya,
            TUYA_FREQUENCY,
            TfgCellular2D::configured(world.seed, 0.21, 1),
        ),
    ] {
        let jitter = match family {
            Family::Cinder => 0.43701595,
            Family::Tuff => 0.2_f32,
            Family::Tuya => 0.21_f32,
        } as f64;
        let margin = jitter / frequency + 2.0;
        let min_i = (((bounds.min_x - margin) * frequency).floor() as i32).saturating_sub(1);
        let max_i = (((bounds.max_x + margin) * frequency).ceil() as i32).saturating_add(1);
        let min_j = (((bounds.min_z - margin) * frequency).floor() as i32).saturating_sub(1);
        let max_j = (((bounds.max_z + margin) * frequency).ceil() as i32).saturating_add(1);
        let count_x = (i64::from(max_i) - i64::from(min_i) + 1).max(0) as u64;
        let count_z = (i64::from(max_j) - i64::from(min_j) + 1).max(0) as u64;
        requested_total = requested_total.saturating_add(count_x.saturating_mul(count_z));
        let mut sampler = sampler;
        sampler.spread(frequency);
        'cells: for lattice_x in min_i..=max_i {
            for lattice_z in min_j..=max_j {
                if scanned.is_multiple_of(64) && cancel() {
                    truncated = true;
                    break 'cells;
                }
                if scanned >= MAX_CELL_CHECKS {
                    truncated = true;
                    break 'cells;
                }
                scanned += 1;
                let cell = sampler.feature_point(lattice_x, lattice_z);
                let (center_x, center_z) = (cell.x as i32, cell.y as i32);
                if !bounds.contains(center_x as f64, center_z as f64) {
                    continue;
                }
                let center_biome = world.region.biome_at_quart(center_x >> 2, center_z >> 2);
                let Some((feature, rarity)) = type_for_center_biome(center_biome, family) else {
                    continue;
                };
                if cell.noise.abs() > 1.0 / rarity as f64
                    || visible_bpp.is_some_and(|bpp| bpp > feature.max_blocks_per_point)
                {
                    continue;
                }

                let biome = crate::biome::biome_name(center_biome);
                let mut marker = Marker {
                    id: format!("{}@{},{}", feature.id, center_x, center_z),
                    label: feature.label.to_owned(),
                    namespace: "tfg".into(),
                    x: f64::from(center_x),
                    z: f64::from(center_z),
                    y: None,
                    kind: MarkerKind::Volcano,
                    confirmed: false,
                    min_y: None,
                    max_y: None,
                    detail: format!(
                        "TFG {} · center biome {biome} · volcanic feature rarity 1/{rarity}",
                        feature.detail
                    ),
                    color: feature.color,
                    bounds: None,
                };
                if seen_ids.insert(marker.id.clone()) && search::matches_query(&marker, query) {
                    marker.y = world.height(center_x, center_z);
                    markers.push(marker);
                }
            }
        }
        if truncated {
            break;
        }
    }
    markers.sort_by(|a, b| {
        a.z.total_cmp(&b.z)
            .then_with(|| a.x.total_cmp(&b.x))
            .then_with(|| a.id.cmp(&b.id))
    });
    FeatureBatch {
        coverage: FeatureCoverage {
            truncated: truncated || requested_total > MAX_CELL_CHECKS as u64,
            scanned_cells: scanned,
            marker_count: markers.len(),
        },
        markers,
    }
}

fn cancelled_batch() -> FeatureBatch {
    FeatureBatch {
        coverage: FeatureCoverage {
            truncated: true,
            ..FeatureCoverage::default()
        },
        ..FeatureBatch::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Settings;

    #[test]
    fn seed_zero_landmarks_are_stable_across_tile_edges_and_queries() {
        let world = World::new(0, Settings::default());
        let bounds = Bounds {
            min_x: -10_000.0,
            min_z: -10_000.0,
            max_x: 10_000.0,
            max_z: 10_000.0,
        };
        let all = candidates(&world, bounds, "", None, &|| false);
        assert!(!all.coverage.truncated);
        assert!(
            !all.markers.is_empty(),
            "seed zero should contain TFG volcanic centers in the 20 km square"
        );
        assert!(
            all.markers
                .iter()
                .any(|m| m.id == "terralens:volcano/active_shield@-2660,3951")
        );

        let mut tiled = Vec::new();
        for (min_x, min_z, max_x, max_z) in [
            (-10_000.0, -10_000.0, 0.0, 0.0),
            (0.0, -10_000.0, 10_000.0, 0.0),
            (-10_000.0, 0.0, 0.0, 10_000.0),
            (0.0, 0.0, 10_000.0, 10_000.0),
        ] {
            tiled.extend(
                candidates(
                    &world,
                    Bounds {
                        min_x,
                        min_z,
                        max_x,
                        max_z,
                    },
                    "volcanic",
                    None,
                    &|| false,
                )
                .markers
                .into_iter()
                .map(|m| m.id),
            );
        }
        tiled.sort();
        tiled.dedup();
        let mut expected: Vec<_> = all.markers.iter().map(|m| m.id.clone()).collect();
        expected.sort();
        assert_eq!(
            tiled, expected,
            "tile unions should equal the same full-area result"
        );
    }

    #[test]
    fn cellular_lattice_samples_return_the_exact_feature_point_for_negative_cells() {
        // JDK/Core Modern TFGCellular2D fixture also checked by noise.rs.
        let noise = TfgCellular2D::new(-6696614430994881185);
        let cell = noise.feature_point(-3, -2);
        assert_eq!(cell.x, -2.565192856882202);
        assert_eq!((cell.cx, cell.cy), (-3, -2));
        assert_eq!(cell.noise, 0.3067944347858429);
    }

    #[test]
    fn cancellation_and_non_overworld_do_not_return_partial_fake_markers() {
        let world = World::new(0, Settings::default());
        let bounds = Bounds {
            min_x: -1000.0,
            min_z: -1000.0,
            max_x: 1000.0,
            max_z: 1000.0,
        };
        assert!(
            candidates(&world, bounds, "", None, &|| true)
                .coverage
                .truncated
        );
        let moon = World::for_dimension(0, Settings::default(), Dimension::Moon, 96);
        assert!(
            candidates(&moon, bounds, "", None, &|| false)
                .markers
                .is_empty()
        );
    }
}
