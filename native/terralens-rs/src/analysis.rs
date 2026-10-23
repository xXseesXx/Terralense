//! Bounded regional summaries and fixed-seed base-location scoring.
use crate::{
    features,
    model::{Bounds, Point, Probe},
    search::forest_name,
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
const MAX_ANALYSIS_SAMPLES: usize = 16_384;
const MAX_LOCATION_SAMPLES: usize = 2_048;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Query {
    pub bounds: Bounds,
    pub sample_budget: usize,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            sample_budget: 4096,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Share {
    pub name: String,
    pub count: usize,
    pub percent: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClimateStats {
    pub temperature_min: f64,
    pub temperature_max: f64,
    pub temperature_mean: f64,
    pub rainfall_min: f64,
    pub rainfall_max: f64,
    pub rainfall_mean: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub bounds: Bounds,
    pub step_blocks: f64,
    pub sample_count: usize,
    pub land_samples: usize,
    pub ocean_samples: usize,
    pub coverage_percent: f64,
    pub biomes: Vec<Share>,
    pub rocks: Vec<Share>,
    pub forests: Vec<Share>,
    pub climate: ClimateStats,
    pub trees_per_hectare: f64,
    pub tree_species: Vec<Share>,
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LocationQuery {
    pub origin: Point,
    pub radius: f64,
    pub min_temperature: Option<f64>,
    pub max_temperature: Option<f64>,
    pub min_rainfall: Option<f64>,
    pub max_rainfall: Option<f64>,
    pub rock: Option<String>,
    pub ore: Option<String>,
    pub ore_distance: f64,
    pub rock_distance: f64,
    pub sample_budget: usize,
}
impl Default for LocationQuery {
    fn default() -> Self {
        Self {
            origin: Point::default(),
            radius: 4096.,
            min_temperature: None,
            max_temperature: None,
            min_rainfall: None,
            max_rainfall: None,
            rock: None,
            ore: None,
            ore_distance: 1024.,
            rock_distance: 0.,
            sample_budget: 1024,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocationCandidate {
    pub point: Point,
    pub distance: f64,
    pub biome: String,
    pub temperature: f64,
    pub rainfall: f64,
    pub rock: String,
    pub nearest_ore: Option<String>,
    pub ore_distance: Option<f64>,
    pub score: f64,
    #[serde(default)]
    pub rock_distance: Option<f64>,
    #[serde(default)]
    pub nearest_rock_point: Option<Point>,
    #[serde(default)]
    pub nearest_rock: Option<String>,
    #[serde(default)]
    pub nearest_ore_point: Option<Point>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocationResults {
    pub candidates: Vec<LocationCandidate>,
    pub samples: usize,
    pub land_samples: usize,
    pub step_blocks: f64,
    pub coverage_percent: f64,
    pub note: String,
    #[serde(default)]
    pub actual_radius: f64,
    #[serde(default)]
    pub requested_radius: f64,
    /// The completed request, rather than controls which may have changed since.
    #[serde(default)]
    pub query: Option<LocationQuery>,
}
fn valid_bounds(b: Bounds) -> bool {
    [b.min_x, b.min_z, b.max_x, b.max_z]
        .into_iter()
        .all(f64::is_finite)
        && b.min_x < b.max_x
        && b.min_z < b.max_z
        && b.min_x.abs().max(b.max_x.abs()) <= 30_000_000.
        && b.min_z.abs().max(b.max_z.abs()) <= 30_000_000.
}
fn grid(b: Bounds, budget: usize) -> f64 {
    let area = (b.max_x - b.min_x) * (b.max_z - b.min_z);
    let mut step = ((area / budget.max(1) as f64).sqrt().ceil().max(16.) / 16.).ceil() * 16.;
    loop {
        let nx = ((b.max_x - b.min_x) / step).ceil().max(1.) as usize;
        let nz = ((b.max_z - b.min_z) / step).ceil().max(1.) as usize;
        if nx.saturating_mul(nz) <= budget.max(1) {
            return step;
        }
        step += 16.;
    }
}
fn shares(counts: HashMap<String, usize>, denom: usize) -> Vec<Share> {
    let mut v: Vec<_> = counts
        .into_iter()
        .map(|(name, count)| Share {
            name,
            count,
            percent: if denom == 0 {
                0.
            } else {
                count as f64 * 100. / denom as f64
            },
        })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    v
}
pub fn analyze(w: &World, q: &Query, cancelled: impl Fn() -> bool) -> Option<Report> {
    let b = q.bounds;
    if !valid_bounds(b) {
        return None;
    }
    let step = grid(b, q.sample_budget.clamp(64, MAX_ANALYSIS_SAMPLES));
    let (nx, nz) = (
        ((b.max_x - b.min_x) / step).ceil() as usize,
        ((b.max_z - b.min_z) / step).ceil() as usize,
    );
    let total = nx.saturating_mul(nz);
    let (mut biomes, mut rocks, mut forests, mut species) = (
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    );
    let (mut ocean, mut land) = (0, 0);
    let (mut tn, mut tx, mut rn, mut rx) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    let (mut tsum, mut rsum, mut tree_sum) = (0., 0., 0.);
    let mut tree_n = 0;
    for iz in 0..nz {
        if cancelled() {
            return None;
        }
        for ix in 0..nx {
            if ix % 16 == 0 && cancelled() {
                return None;
            }
            let x = (b.min_x + (ix as f64 + 0.5) * step)
                .min(b.max_x - 0.5)
                .floor() as i32;
            let z = (b.min_z + (iz as f64 + 0.5) * step)
                .min(b.max_z - 0.5)
                .floor() as i32;
            let id = w.region.biome(x, z);
            if crate::biome::is_ocean(id) {
                ocean += 1;
                continue;
            }
            land += 1;
            *biomes.entry(w.biome(x, z)).or_insert(0) += 1;
            let rock = crate::geology::sample_rock_layer(w.region.surface_rock_at(x, z), 0);
            *rocks.entry(rock).or_insert(0) += 1;
            let forest = w.region.forest_type(x >> 4, z >> 4);
            *forests.entry(forest_name(forest).to_owned()).or_insert(0) += 1;
            let c = w.climate(x, z);
            tn = tn.min(c.temperature);
            tx = tx.max(c.temperature);
            rn = rn.min(c.rainfall);
            rx = rx.max(c.rainfall);
            tsum += c.temperature;
            rsum += c.rainfall;
            if tree_n < 64 && (ix + iz * nx) % (total / 64).max(1) == 0 {
                let p = Probe {
                    biome: w.biome(x, z),
                    ocean: false,
                    climate: c.clone(),
                    forest,
                    ..Probe::default()
                };
                let e = w.tree_estimate(&p, 10_000.);
                tree_sum += e.per_hectare;
                for s in e.species {
                    if s.count > 0. {
                        *species.entry(s.name).or_insert(0) += s.count.round() as usize
                    }
                }
                tree_n += 1
            }
        }
    }
    let climate = if land == 0 {
        ClimateStats::default()
    } else {
        ClimateStats {
            temperature_min: tn,
            temperature_max: tx,
            temperature_mean: tsum / land as f64,
            rainfall_min: rn,
            rainfall_max: rx,
            rainfall_mean: rsum / land as f64,
        }
    };
    let species_total = species.values().copied().sum::<usize>();
    Some(Report {
        bounds: b,
        step_blocks: step,
        sample_count: total,
        land_samples: land,
        ocean_samples: ocean,
        coverage_percent: if total == 0 { 0. } else { 100. },
        biomes: shares(biomes, land),
        rocks: shares(rocks, land),
        forests: shares(forests, land),
        climate,
        trees_per_hectare: if tree_n == 0 {
            0.
        } else {
            tree_sum / tree_n as f64
        },
        tree_species: shares(species, species_total),
        note: if land == 0 {
            "No land in sampled area".into()
        } else {
            format!(
                "Land-only climate, rock and forest shares · {step:.0}-block grid · tree estimate from {tree_n} samples"
            )
        },
    })
}
fn bounds_at(p: Point, r: f64) -> Bounds {
    Bounds {
        min_x: p.x - r,
        min_z: p.z - r,
        max_x: p.x + r,
        max_z: p.z + r,
    }
}
pub fn locate(
    w: &World,
    q: &LocationQuery,
    cancelled: impl Fn() -> bool,
) -> Option<LocationResults> {
    if cancelled()
        || !q.origin.x.is_finite()
        || !q.origin.z.is_finite()
        || !q.radius.is_finite()
        || q.radius <= 0.
        || !q.ore_distance.is_finite()
        || !q.rock_distance.is_finite()
    {
        return None;
    }
    if [
        q.min_temperature,
        q.max_temperature,
        q.min_rainfall,
        q.max_rainfall,
    ]
    .into_iter()
    .flatten()
    .any(|v| !v.is_finite())
        || q.min_temperature
            .zip(q.max_temperature)
            .is_some_and(|(a, b)| a > b)
        || q.min_rainfall
            .zip(q.max_rainfall)
            .is_some_and(|(a, b)| a > b)
    {
        return None;
    }
    let requested = q.radius.min(32_000.);
    let ore_query = q.ore.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let ore_distance = q.ore_distance.clamp(128., 1024.);
    // A complete ore neighborhood remains inside the existing bounded 2km scan.
    let radius = if ore_query.is_some() {
        requested.min(2048. - ore_distance)
    } else {
        requested
    };
    let bounds = bounds_at(q.origin, radius);
    if !valid_bounds(bounds) {
        return None;
    }
    let step = grid(bounds, q.sample_budget.clamp(64, MAX_LOCATION_SAMPLES));
    let nx = ((bounds.max_x - bounds.min_x) / step).ceil() as usize;
    let nz = ((bounds.max_z - bounds.min_z) / step).ceil() as usize;
    let ores = if let Some(query) = ore_query {
        features::ore_candidates_matching(
            w.seed,
            bounds_at(q.origin, radius + ore_distance),
            query,
            |x, z| w.biome(x, z),
            |x, z| Some(w.climate(x, z)),
            |x, z| w.height(x, z).map(|v| v as i32),
            |x, y, z| w.rock_at(x, y, z),
        )
    } else {
        features::FeatureBatch::default()
    };
    if cancelled() {
        return None;
    }
    struct Sample {
        point: Point,
        biome: String,
        temp: f64,
        rain: f64,
        rock: String,
    }
    let mut sampled = Vec::new();
    let mut samples = 0;
    for index in 0..nx * nz + 1 {
        if index % 16 == 0 && cancelled() {
            return None;
        }
        let point = if index == 0 {
            q.origin
        } else {
            let i = index - 1;
            Point {
                x: (bounds.min_x + (i % nx) as f64 * step + step / 2.).min(bounds.max_x - 0.5),
                z: (bounds.min_z + (i / nx) as f64 * step + step / 2.).min(bounds.max_z - 0.5),
            }
        };
        if point.distance(q.origin) > radius {
            continue;
        }
        samples += 1;
        let (x, z) = (point.x.floor() as i32, point.z.floor() as i32);
        if crate::biome::is_ocean(w.region.biome(x, z)) {
            continue;
        }
        let climate = w.climate(x, z);
        sampled.push(Sample {
            point,
            biome: w.biome(x, z),
            temp: climate.temperature,
            rain: climate.rainfall,
            rock: crate::geology::sample_rock_layer(w.region.surface_rock_at(x, z), 0),
        });
    }
    let rock_query = q
        .rock
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_lowercase);
    let rocks: Vec<_> = sampled
        .iter()
        .filter(|s| {
            rock_query
                .as_ref()
                .is_some_and(|q| s.rock.to_lowercase().contains(q))
        })
        .collect();
    let mut candidates = Vec::new();
    for (index, sample) in sampled.iter().enumerate() {
        if index % 16 == 0 && cancelled() {
            return None;
        }
        if q.min_temperature.is_some_and(|v| sample.temp < v)
            || q.max_temperature.is_some_and(|v| sample.temp > v)
            || q.min_rainfall.is_some_and(|v| sample.rain < v)
            || q.max_rainfall.is_some_and(|v| sample.rain > v)
        {
            continue;
        }
        let nearest_rock = if rock_query.is_some() {
            rocks
                .iter()
                .map(|s| (sample.point.distance(s.point), *s))
                .filter(|(d, _)| *d <= q.rock_distance.clamp(0., 2048.))
                .min_by(|a, b| a.0.total_cmp(&b.0))
        } else {
            None
        };
        if rock_query.is_some() && nearest_rock.is_none() {
            continue;
        }
        let nearest = ores
            .markers
            .iter()
            .map(|m| (sample.point.distance(m.point()), m))
            .filter(|(d, _)| *d <= ore_distance)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if ore_query.is_some() && nearest.is_none() {
            continue;
        }
        let distance = sample.point.distance(q.origin);
        candidates.push(LocationCandidate {
            point: sample.point,
            distance,
            biome: sample.biome.clone(),
            temperature: sample.temp,
            rainfall: sample.rain,
            rock: sample.rock.clone(),
            nearest_ore: nearest.map(|(_, m)| m.label.clone()),
            ore_distance: nearest.map(|(d, _)| d),
            rock_distance: nearest_rock.map(|(d, _)| d),
            nearest_rock_point: nearest_rock.map(|(_, s)| s.point),
            nearest_rock: nearest_rock.map(|(_, s)| s.rock.clone()),
            nearest_ore_point: nearest.map(|(_, m)| m.point()),
            score: distance,
        });
    }
    candidates.sort_by(|a, b| {
        a.distance.total_cmp(&b.distance).then_with(|| {
            a.ore_distance
                .unwrap_or(0.)
                .total_cmp(&b.ore_distance.unwrap_or(0.))
        })
    });
    // Keep distinct sampled sites throughout the requested area. A nearest-result
    // cap would leave an artificial central island of matches on the map.
    let mut distinct: Vec<LocationCandidate> = Vec::new();
    for candidate in candidates {
        if distinct
            .iter()
            .all(|other| candidate.point.distance(other.point) >= step * 2.)
        {
            distinct.push(candidate);
        }
    }
    Some(LocationResults {
        candidates: distinct,
        samples,
        land_samples: sampled.len(),
        step_blocks: step,
        coverage_percent: 100. * (radius / requested).powi(2),
        actual_radius: radius,
        requested_radius: requested,
        query: Some(q.clone()),
        note: format!(
            "{radius:.0}-block radius · {step:.0}-block sampled grid{} · nearest first",
            if ores.coverage.truncated {
                " · ore candidate scan limited"
            } else {
                ""
            }
        ),
    })
}
impl LocationCandidate {
    pub fn marker(&self) -> crate::model::Marker {
        crate::model::Marker {
            id: "terralens:location".into(),
            label: self.biome.clone(),
            namespace: "terralens".into(),
            x: self.point.x,
            z: self.point.z,
            y: None,
            kind: crate::model::MarkerKind::Biome,
            confirmed: false,
            min_y: None,
            max_y: None,
            detail: format!(
                "{:.1} °C · {:.0} mm · {}{}{}",
                self.temperature,
                self.rainfall,
                self.rock,
                self.rock_distance
                    .map(|d| format!(" · target rock ~{d:.0} blocks"))
                    .unwrap_or_default(),
                self.nearest_ore
                    .as_ref()
                    .zip(self.ore_distance)
                    .map(|(ore, d)| format!(" · {ore} ~{d:.0} blocks"))
                    .unwrap_or_default()
            ),
            color: [130, 224, 191],
            bounds: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Settings, world::World};
    #[test]
    fn shares_exclude_ocean_and_sum_over_land() {
        let w = World::new(4241021225841912682, Settings::default());
        let q = Query {
            bounds: Bounds {
                min_x: -4096.,
                min_z: -4096.,
                max_x: 4096.,
                max_z: 4096.,
            },
            sample_budget: 1024,
        };
        let r = analyze(&w, &q, || false).unwrap();
        assert!(r.ocean_samples > 0);
        assert_eq!(
            r.biomes.iter().map(|x| x.count).sum::<usize>(),
            r.land_samples
        );
        assert!((r.biomes.iter().map(|x| x.percent).sum::<f64>() - 100.).abs() < 1e-6)
    }
    #[test]
    fn location_constraints_keep_land_match_origin_and_sort_by_distance() {
        let w = World::new(7787531943268589495, Settings::default());
        let origin = Point {
            x: -2244.,
            z: -2316.,
        };
        let result = locate(
            &w,
            &LocationQuery {
                origin,
                radius: 512.,
                sample_budget: 64,
                ..Default::default()
            },
            || false,
        )
        .unwrap();
        assert!(!result.candidates.is_empty());
        assert!(
            result
                .candidates
                .windows(2)
                .all(|pair| pair[0].distance <= pair[1].distance)
        );
        for candidate in &result.candidates {
            assert!(!crate::biome::is_ocean(
                w.region
                    .biome(candidate.point.x as i32, candidate.point.z as i32)
            ));
            assert!(candidate.distance <= 512.);
        }
        let climate = w.climate(origin.x as i32, origin.z as i32);
        let exact = locate(
            &w,
            &LocationQuery {
                origin,
                radius: 512.,
                min_temperature: Some(climate.temperature - 0.01),
                max_temperature: Some(climate.temperature + 0.01),
                ..Default::default()
            },
            || false,
        )
        .unwrap();
        assert!(
            exact
                .candidates
                .iter()
                .all(|c| (c.temperature - climate.temperature).abs() <= 0.01)
        );
    }
    #[test]
    fn analysis_sampling_stays_bounded_for_long_thin_regions() {
        let b = Bounds {
            min_x: -30_000_000.,
            min_z: 0.,
            max_x: 30_000_000.,
            max_z: 1.,
        };
        let step = grid(b, 64);
        let samples = ((b.max_x - b.min_x) / step).ceil() as usize
            * ((b.max_z - b.min_z) / step).ceil() as usize;
        assert!(samples <= 64);
    }
    #[test]
    fn site_links_point_to_matching_rock_and_ore_at_the_reported_distances() {
        let w = World::new(7787531943268589495, Settings::default());
        let origin = Point {
            x: -2244.,
            z: -2316.,
        };
        let result = locate(
            &w,
            &LocationQuery {
                origin,
                radius: 1024.,
                sample_budget: 256,
                rock: Some("chalk".into()),
                rock_distance: 768.,
                ore: Some("copper".into()),
                ore_distance: 512.,
                ..Default::default()
            },
            || false,
        )
        .unwrap();
        assert!(!result.candidates.is_empty());
        for site in &result.candidates {
            let rock = site
                .nearest_rock_point
                .expect("matching sampled rock location");
            let ore = site
                .nearest_ore_point
                .expect("matching predicted ore location");
            assert_eq!(site.nearest_rock.as_deref(), Some("chalk"));
            assert_eq!(
                crate::geology::sample_rock_layer(
                    w.region.surface_rock_at(rock.x as i32, rock.z as i32),
                    0
                ),
                "chalk"
            );
            assert!((site.point.distance(rock) - site.rock_distance.unwrap()).abs() < 1e-6);
            assert!((site.point.distance(ore) - site.ore_distance.unwrap()).abs() < 1e-6);
            assert!(site.point.distance(rock) <= 768.);
            assert!(site.point.distance(ore) <= 512.);
            assert!(site.nearest_ore.as_deref().unwrap().contains("copper"));
        }
    }
    #[test]
    fn site_evidence_retains_completed_query_and_exact_sampled_rock_position() {
        let w = World::new(7787531943268589495, Settings::default());
        let origin = Point {
            x: -2244.,
            z: -2316.,
        };
        let rock = crate::geology::sample_rock_layer(
            w.region.surface_rock_at(origin.x as i32, origin.z as i32),
            0,
        );
        let query = LocationQuery {
            origin,
            radius: 512.,
            rock: Some(rock.clone()),
            rock_distance: 0.,
            sample_budget: 64,
            ..Default::default()
        };
        let result = locate(&w, &query, || false).unwrap();
        let site = result
            .candidates
            .iter()
            .find(|c| c.point == origin)
            .expect("origin is a matching sampled site");
        assert_eq!(site.nearest_rock_point, Some(origin));
        assert_eq!(site.nearest_rock.as_deref(), Some(rock.as_str()));
        assert_eq!(site.rock_distance, Some(0.));
        assert!(site.nearest_ore_point.is_none());
        assert_eq!(result.query.as_ref().unwrap().origin, origin);
        assert_eq!(result.query.as_ref().unwrap().rock, query.rock);
        let copy: LocationResults =
            serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap();
        assert_eq!(
            copy.candidates[0].nearest_rock_point,
            result.candidates[0].nearest_rock_point
        );
        let old: LocationCandidate = serde_json::from_str(r#"{"point":{"x":0.0,"z":0.0},"distance":0.0,"biome":"plains","temperature":10.0,"rainfall":100.0,"rock":"granite","nearest_ore":null,"ore_distance":null,"score":0.0}"#).unwrap();
        assert!(old.nearest_rock_point.is_none());
    }
    #[test]
    fn cancellation_and_invalid_location_are_handled() {
        let w = World::new(1, Settings::default());
        let q = Query {
            bounds: Bounds {
                min_x: 0.,
                min_z: 0.,
                max_x: 2048.,
                max_z: 2048.,
            },
            sample_budget: 2048,
        };
        assert!(analyze(&w, &q, || true).is_none());
        let bad = LocationQuery {
            origin: Point { x: f64::NAN, z: 0. },
            ..Default::default()
        };
        assert!(locate(&w, &bad, || false).is_none())
    }
}
