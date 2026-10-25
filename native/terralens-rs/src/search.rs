//! Unified atlas search. Saved starts are indexed separately; predicted ore and terrain searches
//! report their finite coverage. Terrain results are sample locations, never claimed block-exact.
use crate::{
    features, geology,
    model::{Bounds, Marker, MarkerKind, Point, Probe},
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    #[default]
    All,
    Structures,
    Volcanoes,
    Ores,
    Biomes,
    Geology,
    Forests,
    Trees,
}
impl Scope {
    pub const ALL: [Self; 8] = [
        Self::All,
        Self::Structures,
        Self::Volcanoes,
        Self::Ores,
        Self::Biomes,
        Self::Geology,
        Self::Forests,
        Self::Trees,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Everything",
            Self::Structures => "Structures",
            Self::Volcanoes => "Volcanoes",
            Self::Ores => "Ores & minerals",
            Self::Biomes => "Biomes",
            Self::Geology => "Rock types",
            Self::Forests => "Forest types",
            Self::Trees => "Tree habitats",
        }
    }
    pub fn accepts(self, marker: &Marker) -> bool {
        self == Self::All
            || matches!(
                (self, &marker.kind),
                (Self::Structures, MarkerKind::Structure)
                    | (Self::Volcanoes, MarkerKind::Volcano)
                    | (Self::Ores, MarkerKind::Ore)
                    | (Self::Biomes, MarkerKind::Biome)
                    | (Self::Geology, MarkerKind::Rock)
                    | (Self::Forests, MarkerKind::Forest)
                    | (Self::Trees, MarkerKind::Tree)
            )
    }
}
#[derive(Clone)]
pub struct Query {
    pub origin: Point,
    pub radius: f64,
    pub text: String,
    pub scope: Scope,
    pub structures_predicted: bool,
}
pub struct Outcome {
    pub markers: Vec<Marker>,
    pub limited: bool,
    pub note: String,
}
pub fn rank(
    origin: Point,
    radius: f64,
    markers: impl IntoIterator<Item = Marker>,
    limit: usize,
) -> Vec<Marker> {
    let mut values: Vec<_> = markers
        .into_iter()
        .filter(|m| {
            let point = m.map_point();
            point.x.is_finite() && point.z.is_finite() && origin.distance(point) <= radius
        })
        .collect();
    values.sort_by(|a, b| {
        origin
            .distance(a.map_point())
            .total_cmp(&origin.distance(b.map_point()))
            .then_with(|| b.confirmed.cmp(&a.confirmed))
            .then_with(|| a.key().cmp(&b.key()))
    });
    let mut seen = std::collections::HashSet::new();
    values.retain(|m| seen.insert(m.key()));
    values.truncate(limit);
    values
}
pub fn predict(w: &World, q: &Query) -> Outcome {
    predict_cancelable(w, q, || false).expect("uncancelled search")
}
pub fn predict_cancelable(w: &World, q: &Query, cancelled: impl Fn() -> bool) -> Option<Outcome> {
    if cancelled() {
        return None;
    }
    if w.dimension == crate::dimension::Dimension::Beneath {
        return predict_beneath(w, q, cancelled);
    }
    if w.dimension.is_planetary() {
        return predict_planetary(w, q, cancelled);
    }
    let mut markers = Vec::new();
    let mut limited = false;
    let mut notes = Vec::new();
    if q.scope == Scope::Volcanoes || q.scope == Scope::All && !q.text.trim().is_empty() {
        let batch =
            crate::volcano::candidates(w, bounds(q.origin, q.radius), &q.text, None, &cancelled);
        limited |= batch.coverage.truncated;
        markers.extend(batch.markers);
        notes.push(format!("volcano ≤{:.1} km", q.radius / 1000.));
    }
    if q.structures_predicted && matches!(q.scope, Scope::All | Scope::Structures) {
        let radius = q.radius.min(8192.);
        let b = bounds(q.origin, radius);
        let batch = features::structure_candidates_with_world(
            w.seed,
            b,
            |x, z| w.biome(x, z),
            |x, z| Some(w.climate(x, z)),
            |x, z| w.region.forest_type(x, z),
            |x, z| w.height(x, z).map(|y| y as i32),
        );
        markers.extend(
            batch
                .markers
                .into_iter()
                .filter(|m| matches_query(m, &q.text)),
        );
        limited |= batch.coverage.truncated || radius < q.radius;
        notes.push(format!("structure ≤{:.1} km", radius / 1000.));
    }
    if cancelled() {
        return None;
    }
    if q.scope == Scope::Ores || q.scope == Scope::All && !q.text.trim().is_empty() {
        let radius = q.radius.min(2048.);
        let batch = features::ore_candidates_matching(
            w.seed,
            bounds(q.origin, radius),
            &q.text,
            |x, z| w.biome(x, z),
            |x, z| Some(w.climate(x, z)),
            |x, z| w.height(x, z).map(|y| y as i32),
            |x, y, z| w.rock_at(x, y, z),
        );
        limited |= batch.coverage.truncated || radius < q.radius;
        markers.extend(batch.markers);
        notes.push(format!("ore ≤{:.1} km", radius / 1000.));
    }
    if cancelled() {
        return None;
    }
    if matches!(
        q.scope,
        Scope::Biomes | Scope::Geology | Scope::Forests | Scope::Trees
    ) || q.scope == Scope::All && !q.text.trim().is_empty()
    {
        let radius = q.radius.min(4096.);
        let step = 128.;
        let cells = (radius / step).ceil() as i32;
        let mut nearest = HashMap::<String, Marker>::new();
        for z in -cells..=cells {
            if cancelled() {
                return None;
            }
            for x in -cells..=cells {
                let point = Point {
                    x: q.origin.x + x as f64 * step,
                    z: q.origin.z + z as f64 * step,
                };
                if q.origin.distance(point) > radius
                    || point.x.abs() > 30_000_000.
                    || point.z.abs() > 30_000_000.
                {
                    continue;
                }
                let (bx, bz) = (point.x.floor() as i32, point.z.floor() as i32);
                let biome = w.biome(bx, bz);
                if matches!(q.scope, Scope::All | Scope::Biomes) {
                    location(
                        &mut nearest,
                        q,
                        &biome,
                        MarkerKind::Biome,
                        point,
                        "Biome sample · 128-block search grid",
                        [127, 192, 148],
                    );
                }
                if crate::biome::is_ocean(w.region.biome(bx, bz)) {
                    continue;
                }
                if matches!(q.scope, Scope::All | Scope::Geology) {
                    let rock = geology::sample_rock_layer(w.region.surface_rock_at(bx, bz), 0);
                    location(
                        &mut nearest,
                        q,
                        &rock,
                        MarkerKind::Rock,
                        point,
                        "surface rock · 128-block search grid",
                        [189, 167, 140],
                    );
                }
                if matches!(q.scope, Scope::All | Scope::Forests | Scope::Trees)
                    && !w.is_surface_water(bx, bz)
                {
                    let forest = w.region.forest_type(bx >> 4, bz >> 4);
                    if matches!(q.scope, Scope::All | Scope::Forests) {
                        location(
                            &mut nearest,
                            q,
                            forest_name(forest),
                            MarkerKind::Forest,
                            point,
                            "Forest-type sample · 128-block search grid",
                            [105, 177, 127],
                        );
                    }
                    if matches!(q.scope, Scope::All | Scope::Trees) && forest > 0 {
                        let p = Probe {
                            biome,
                            ocean: false,
                            forest,
                            climate: w.climate(bx, bz),
                            ..Probe::default()
                        };
                        for species in geology::estimate_trees(&p, 10_000.).species {
                            if species.count > 0. {
                                location(
                                    &mut nearest,
                                    q,
                                    &species.name,
                                    MarkerKind::Tree,
                                    point,
                                    "Potential tree habitat · climate and forest eligibility · not a placed-tree census · sampled every 128 blocks",
                                    [124, 191, 150],
                                );
                            }
                        }
                    }
                }
            }
        }
        limited |= radius < q.radius;
        markers.extend(nearest.into_values());
        notes.push(format!(
            "terrain sampled every 128 blocks, ≤{:.1} km",
            radius / 1000.
        ));
    }
    Some(Outcome {
        markers: rank(q.origin, q.radius, markers, 4096),
        limited,
        note: notes.join(" · "),
    })
}
fn predict_planetary(w: &World, q: &Query, cancelled: impl Fn() -> bool) -> Option<Outcome> {
    if cancelled() {
        return None;
    }
    let ores = q.scope == Scope::Ores || q.scope == Scope::All && !q.text.trim().is_empty();
    let structures = q.structures_predicted && matches!(q.scope, Scope::All | Scope::Structures);
    let biomes =
        matches!(q.scope, Scope::Biomes) || q.scope == Scope::All && !q.text.trim().is_empty();
    if !ores && !structures && !biomes {
        return Some(Outcome {
            markers: Vec::new(),
            limited: false,
            note: format!("{} · no matching feature scope", w.dimension.label()),
        });
    }
    let feature_radius = if ores || structures {
        q.radius.min(if ores { 2048. } else { 8192. })
    } else {
        0.
    };
    let mut markers = Vec::new();
    let mut limited = feature_radius > 0. && feature_radius < q.radius;
    let mut notes = Vec::new();
    if ores || structures {
        let batch = crate::planetary::features::predict_matching(
            w,
            bounds(q.origin, feature_radius),
            ores,
            structures,
            &q.text,
            &cancelled,
        );
        if cancelled() {
            return None;
        }
        limited |= batch.coverage.truncated;
        markers.extend(batch.markers);
        notes.push(format!(
            "seed-derived ore/structure candidates ≤{:.1} km; generated placement is unconfirmed",
            feature_radius / 1000.
        ));
    }
    if biomes {
        let radius = q.radius.min(4096.);
        let step = 128.;
        let cells = (radius / step).ceil() as i32;
        let mut nearest = HashMap::<String, Marker>::new();
        for dz in -cells..=cells {
            if cancelled() {
                return None;
            }
            for dx in -cells..=cells {
                let point = Point {
                    x: q.origin.x + dx as f64 * step,
                    z: q.origin.z + dz as f64 * step,
                };
                if q.origin.distance(point) > radius
                    || point.x.abs() > 30_000_000.
                    || point.z.abs() > 30_000_000.
                {
                    continue;
                }
                let (x, z) = (point.x.floor() as i32, point.z.floor() as i32);
                let biome = w.biome(x, z);
                if !features::text_matches(&biome, &q.text) {
                    continue;
                }
                let id = format!("{}:biome:{biome}", w.dimension.id());
                if nearest
                    .get(&id)
                    .is_some_and(|old| q.origin.distance(old.point()) <= q.origin.distance(point))
                {
                    continue;
                }
                nearest.insert(
                    id.clone(),
                    Marker {
                        id,
                        label: biome.rsplit('/').next().unwrap_or(&biome).replace('_', " "),
                        namespace: "tfg".into(),
                        x: point.x,
                        z: point.z,
                        y: None,
                        kind: MarkerKind::Biome,
                        confirmed: false,
                        min_y: None,
                        max_y: None,
                        bounds: None,
                        detail: "Planetary biome sample · 128-block search grid".into(),
                        color: [127, 192, 148],
                    },
                );
            }
        }
        limited |= radius < q.radius;
        markers.extend(nearest.into_values());
        notes.push(format!(
            "biome samples every 128 blocks ≤{:.1} km",
            radius / 1000.
        ));
    }
    if cancelled() {
        return None;
    }
    Some(Outcome {
        markers: rank(q.origin, q.radius, markers, 4096),
        limited,
        note: format!("{} · {}", w.dimension.label(), notes.join(" · ")),
    })
}
fn predict_beneath(w: &World, q: &Query, cancelled: impl Fn() -> bool) -> Option<Outcome> {
    let mut markers = Vec::new();
    let mut limited = false;
    let (band_min, band_max) = w.dimension.band(w.slice_y);
    let biome_y = band_min + 8;
    let mut notes = vec![format!(
        "Beneath · layer {band_min}–{band_max} · biomes at Y {biome_y}"
    )];
    let ores = q.scope == Scope::Ores || q.scope == Scope::All && !q.text.trim().is_empty();
    let structures = q.structures_predicted && matches!(q.scope, Scope::All | Scope::Structures);
    if ores || structures {
        let radius = q.radius.min(if ores { 2048. } else { 8192. });
        let batch = features::predict_beneath_matching(
            w,
            bounds(q.origin, radius),
            ores,
            structures,
            &q.text,
            &cancelled,
        );
        limited |= batch.coverage.truncated || radius < q.radius;
        markers.extend(batch.markers);
        notes.push(format!(
            "feature candidates ≤{:.1} km · ore pins include all heights",
            radius / 1000.
        ));
    }
    if matches!(q.scope, Scope::All | Scope::Biomes) && !q.text.trim().is_empty() {
        let radius = q.radius.min(4096.);
        let step = 128.;
        let cells = (radius / step).ceil() as i32;
        let mut nearest = HashMap::<String, Marker>::new();
        for z in -cells..=cells {
            if cancelled() {
                return None;
            }
            for x in -cells..=cells {
                let point = Point {
                    x: q.origin.x + x as f64 * step,
                    z: q.origin.z + z as f64 * step,
                };
                if q.origin.distance(point) > radius
                    || point.x.abs() > 30_000_000.
                    || point.z.abs() > 30_000_000.
                {
                    continue;
                }
                let name = w.biome(point.x.floor() as i32, point.z.floor() as i32);
                location(
                    &mut nearest,
                    q,
                    &name,
                    MarkerKind::Biome,
                    point,
                    &format!("Beneath biome at Y {} · 128-block search grid", biome_y),
                    [170, 148, 179],
                );
            }
        }
        markers.extend(nearest.into_values().map(|mut m| {
            m.y = Some(biome_y as f64);
            m
        }));
        limited |= radius < q.radius;
        notes.push(format!(
            "biomes sampled every 128 blocks, ≤{:.1} km",
            radius / 1000.
        ));
    }
    if matches!(q.scope, Scope::Geology | Scope::Forests | Scope::Trees) {
        notes.push("This layer is not available for the Beneath".into());
    }
    if cancelled() {
        return None;
    }
    Some(Outcome {
        markers: rank(q.origin, q.radius, markers, 4096),
        limited,
        note: notes.join(" · "),
    })
}
fn bounds(p: Point, r: f64) -> Bounds {
    Bounds {
        min_x: p.x - r,
        min_z: p.z - r,
        max_x: p.x + r,
        max_z: p.z + r,
    }
}
pub fn matches_query(m: &Marker, q: &str) -> bool {
    let aliases = if m.kind == MarkerKind::Volcano {
        "volcano volcanoes volcanos volcanic natural landmark"
    } else if m.kind == MarkerKind::Structure
        && m.id.split('@').next() == Some("tfg:illagers/norse_village_ruined")
    {
        "dead abandoned village"
    } else {
        ""
    };
    features::text_matches(
        &format!(
            "{} {} {} {} {} {}",
            m.id,
            m.label,
            m.namespace,
            m.detail,
            aliases,
            if m.kind == MarkerKind::Structure {
                crate::structure_loot::search_terms(&m.id)
            } else {
                ""
            }
        ),
        q,
    )
}
pub fn forest_name(id: u8) -> &'static str {
    match id {
        0 => "No forest",
        1 => "Sparse forest",
        2 => "Forest edge",
        3 => "Normal forest",
        4 => "Old growth forest",
        _ => "Unknown forest",
    }
}
fn location(
    nearest: &mut HashMap<String, Marker>,
    q: &Query,
    name: &str,
    kind: MarkerKind,
    point: Point,
    detail: &str,
    color: [u8; 3],
) {
    if !features::text_matches(name, &q.text) {
        return;
    }
    let id = format!("{kind:?}:{name}");
    if nearest
        .get(&id)
        .is_some_and(|m| q.origin.distance(m.point()) <= q.origin.distance(point))
    {
        return;
    }
    nearest.insert(
        id.clone(),
        Marker {
            id,
            label: name.rsplit('/').next().unwrap_or(name).replace('_', " "),
            namespace: "tfg".into(),
            x: point.x,
            z: point.z,
            y: None,
            kind,
            confirmed: false,
            min_y: None,
            max_y: None,
            detail: detail.into(),
            color,
            bounds: None,
        },
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn planetary_biome_search_returns_dimension_scoped_nearest_samples() {
        let world = World::for_dimension(
            0,
            crate::model::Settings::default(),
            crate::dimension::Dimension::Moon,
            64,
        );
        let query = Query {
            origin: Point::default(),
            radius: 1024.,
            text: "lunar".into(),
            scope: Scope::Biomes,
            structures_predicted: false,
        };
        let outcome = predict(&world, &query);
        assert!(!outcome.markers.is_empty());
        assert!(
            !predict(
                &world,
                &Query {
                    text: String::new(),
                    ..query.clone()
                }
            )
            .markers
            .is_empty()
        );
        for marker in &outcome.markers {
            assert!(marker.id.starts_with("ad_astra:moon:biome:tfg:moon/"));
            assert_eq!(
                marker.id.trim_start_matches("ad_astra:moon:biome:"),
                world.biome(marker.x.floor() as i32, marker.z.floor() as i32)
            );
            assert_eq!(marker.kind, MarkerKind::Biome);
        }
        assert!(predict_cancelable(&world, &query, || true).is_none());

        let mars = World::for_dimension(
            0,
            crate::model::Settings::default(),
            crate::dimension::Dimension::Mars,
            88,
        );
        let mars_query = Query {
            text: "martian".into(),
            ..query
        };
        let outcome = predict(&mars, &mars_query);
        assert!(!outcome.markers.is_empty());
        assert!(
            outcome
                .markers
                .iter()
                .all(|marker| marker.id.starts_with("ad_astra:mars:biome:tfg:mars/"))
        );
    }
    #[test]
    fn ranks_every_marker_kind_by_distance() {
        let make = |kind, x| Marker {
            id: format!("{kind:?}"),
            label: String::new(),
            namespace: String::new(),
            x,
            z: 0.,
            y: None,
            kind,
            confirmed: false,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [0; 3],
            bounds: None,
        };
        let v = rank(
            Point::default(),
            100.,
            [
                make(MarkerKind::Ore, 10.),
                make(MarkerKind::Structure, 30.),
                make(MarkerKind::Biome, 5.),
            ],
            99,
        );
        assert_eq!(
            v.iter().map(|m| m.x).collect::<Vec<_>>(),
            vec![5., 10., 30.]
        );
    }
    #[test]
    fn structure_radius_and_order_use_inclusive_footprint_center() {
        let structure = Marker {
            id: "minecraft:large_ruin".into(),
            label: "Large ruin".into(),
            namespace: "minecraft".into(),
            x: 0.,
            z: 0.,
            y: None,
            kind: MarkerKind::Structure,
            confirmed: true,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [0; 3],
            bounds: Some(crate::model::StructureBounds {
                min_x: 99,
                min_y: 0,
                min_z: -1,
                max_x: 100,
                max_y: 10,
                max_z: 0,
            }),
        };
        assert_eq!(structure.point(), Point { x: 0., z: 0. });
        assert_eq!(structure.map_point(), Point { x: 100., z: 0. });
        assert!(rank(Point { x: 100., z: 0. }, 0.5, [structure.clone()], 5).len() == 1);
        assert!(rank(Point::default(), 50., [structure.clone()], 5).is_empty());
        assert!(rank(Point { x: 100., z: 0. }, 0.5, [structure], 5).len() == 1);
    }
    #[test]
    fn dead_village_aliases_find_recorded_and_seed_markers_without_matching_living_villages() {
        let mut marker = Marker {
            id: "tfg:illagers/norse_village_ruined@354,-419".into(),
            label: "tfg:illagers/norse_village_ruined".into(),
            namespace: "tfg".into(),
            x: 5675.5,
            z: -6704.,
            y: None,
            kind: MarkerKind::Structure,
            confirmed: true,
            min_y: None,
            max_y: None,
            bounds: None,
            detail: String::new(),
            color: [230, 173, 85],
        };
        for confirmed in [false, true] {
            marker.confirmed = confirmed;
            for query in [
                "dead village",
                "abandoned village",
                "norse ruined",
                "norse dead",
            ] {
                assert!(matches_query(&marker, query), "{query}");
            }
        }
        marker.id = "tfg:illagers/norse_village_living@354,-419".into();
        marker.label = "Norse living village".into();
        assert!(!matches_query(&marker, "dead village"));
        assert!(!matches_query(&marker, "abandoned village"));
    }

    #[test]
    fn structure_container_search_finds_loot_vessels_from_the_saved_mudbrick_site() {
        let marker = Marker {
            id: "tfc_ruins:ruin_small@1231,-204".into(),
            label: "Small ruined building".into(),
            namespace: "tfc_ruins".into(),
            x: 19700.,
            z: -3260.5,
            y: None,
            kind: MarkerKind::Structure,
            confirmed: true,
            min_y: None,
            max_y: None,
            bounds: None,
            detail: String::new(),
            color: [230, 173, 85],
        };
        assert!(matches_query(&marker, "loot vessel"));
        assert!(matches_query(&marker, "large vessel"));
        let mut moai = marker;
        moai.id = "tfg:ocean/ocean_moai_0@1231,-204".into();
        moai.label = "Ocean moai".into();
        assert!(!matches_query(&moai, "loot vessel"));
    }

    #[test]
    fn supports_material_aliases_and_multiword_names() {
        assert!(features::text_matches("deep_galena silver lead", "silver"));
        assert!(features::text_matches(
            "tfg:earth/old_mountains",
            "old mountains"
        ));
        assert!(!features::text_matches("platinum", "tin ore"));
    }
    #[test]
    fn cancelled_search_stops_before_world_generation() {
        let w = World::new(1, crate::model::Settings::default());
        let q = Query {
            origin: Point::default(),
            radius: 4096.,
            text: "silver".into(),
            scope: Scope::All,
            structures_predicted: true,
        };
        assert!(predict_cancelable(&w, &q, || true).is_none());
    }

    #[test]
    fn ore_search_matches_real_vein_materials_and_stays_sorted() {
        let w = World::new(7787531943268589495, crate::model::Settings::default());
        let q = Query {
            origin: Point {
                x: -2244.,
                z: -2316.,
            },
            radius: 512.,
            text: "SILVER".into(),
            scope: Scope::Ores,
            structures_predicted: false,
        };
        let out = predict(&w, &q);
        assert!(
            !out.markers.is_empty(),
            "fixture window contains silver-bearing galena"
        );
        assert!(out.markers.iter().any(|m| m.id.contains("galena")));
        assert!(
            out.markers
                .iter()
                .all(|m| m.kind == MarkerKind::Ore && matches_query(m, "silver"))
        );
        assert!(
            out.markers
                .windows(2)
                .all(|m| q.origin.distance(m[0].point()) <= q.origin.distance(m[1].point()))
        );
    }

    #[test]
    fn biome_search_reports_sampling_and_finds_origin() {
        let w = World::new(7787531943268589495, crate::model::Settings::default());
        let q = Query {
            origin: Point {
                x: -2244.,
                z: -2316.,
            },
            radius: 128.,
            text: String::new(),
            scope: Scope::Biomes,
            structures_predicted: false,
        };
        let out = predict(&w, &q);
        assert!(!out.markers.is_empty());
        assert_eq!(out.markers[0].point().distance(q.origin), 0.);
        assert!(out.note.contains("128"));
    }
}
