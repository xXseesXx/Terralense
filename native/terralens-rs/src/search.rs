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
    Ores,
    Biomes,
    Geology,
    Forests,
    Trees,
}
impl Scope {
    pub const ALL: [Self; 7] = [
        Self::All,
        Self::Structures,
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
        .filter(|m| m.x.is_finite() && m.z.is_finite() && origin.distance(m.point()) <= radius)
        .collect();
    values.sort_by(|a, b| {
        origin
            .distance(a.point())
            .total_cmp(&origin.distance(b.point()))
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
    let mut markers = Vec::new();
    let mut limited = false;
    let mut notes = Vec::new();
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
                if matches!(q.scope, Scope::All | Scope::Forests | Scope::Trees) {
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
    features::text_matches(
        &format!("{} {} {} {}", m.id, m.label, m.namespace, m.detail),
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
