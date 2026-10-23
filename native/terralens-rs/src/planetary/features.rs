//! Seed-derived Moon/Mars vein and random-spread feature candidates.
//!
//! This ports only active, data-driven TFC vein and structure-set placement. It does not
//! claim generated structure footprints or block-level validation.
use crate::{
    dimension::Dimension,
    features::{self, FeatureBatch, FeatureCoverage},
    model::{Bounds, Marker, MarkerKind},
    random::{JavaRandom, Xoroshiro},
    world::World,
};
use serde_json::Value;
use std::{collections::HashSet, sync::OnceLock};

const FEATURE_DATA: &str = include_str!("feature-assets/moon-mars-features.json");
const MAX_CHECKS: usize = 1_000_000;

fn data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(FEATURE_DATA).expect("validated planetary feature data")
    })
}
fn key(d: Dimension) -> &'static str {
    match d {
        Dimension::Moon => "moon",
        Dimension::Mars => "mars",
        _ => "",
    }
}
fn label(id: &str) -> String {
    id.rsplit(':').next().unwrap_or(id).replace(['_', '/'], " ")
}

/// Ore IDs include their dimension; structures retain registry IDs shared with saved starts.
/// Tuple order is `(ores, structures)`.
pub fn feature_catalog(
    dimension: Dimension,
) -> &'static (Vec<(String, String)>, Vec<(String, String)>) {
    static MOON: OnceLock<(Vec<(String, String)>, Vec<(String, String)>)> = OnceLock::new();
    static MARS: OnceLock<(Vec<(String, String)>, Vec<(String, String)>)> = OnceLock::new();
    let cell = match dimension {
        Dimension::Moon => &MOON,
        Dimension::Mars => &MARS,
        _ => {
            static EMPTY: (Vec<(String, String)>, Vec<(String, String)>) = (Vec::new(), Vec::new());
            return &EMPTY;
        }
    };
    cell.get_or_init(|| {
        let d = &data()[key(dimension)];
        let prefix = key(dimension);
        let ores = d["veins"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(id, _)| (format!("{prefix}:{id}"), label(id)))
            .collect();
        let structures = crate::structure_support::catalog(dimension).clone();
        (ores, structures)
    })
}

pub fn predict_visible(
    world: &mut World,
    bounds: Bounds,
    ores: bool,
    structures: bool,
    bpp: f64,
    cancel: &impl Fn() -> bool,
) -> FeatureBatch {
    predict(world, bounds, ores, structures, "", Some(bpp), cancel)
}
pub fn predict_matching(
    world: &World,
    bounds: Bounds,
    ores: bool,
    structures: bool,
    text: &str,
    cancel: &impl Fn() -> bool,
) -> FeatureBatch {
    predict(world, bounds, ores, structures, text, None, cancel)
}
/// Per-definition visibility threshold used both while scanning and by the shared marker filter.
pub fn marker_max_blocks_per_point(marker: &Marker) -> Option<f64> {
    let id = marker.id.split('@').next()?;
    let (base, notable) = match marker.kind {
        MarkerKind::Ore => {
            let (dimension, raw_id) = id.split_once(':')?;
            let d = data().get(dimension)?;
            let config = d["veins"].get(raw_id)?;
            let rarity = config["config"]["rarity"].as_f64().unwrap_or(1.).max(1.);
            let notable = [
                "gold", "silver", "platinum", "diamond", "ruby", "sapphire", "emerald", "amethyst",
            ]
            .iter()
            .any(|word| raw_id.to_lowercase().contains(word));
            (16. * rarity.sqrt() / 32., notable)
        }
        MarkerKind::Structure => {
            let set = ["moon", "mars"]
                .into_iter()
                .find_map(|dimension| data()[dimension]["structure_sets"].get(id))?;
            let spacing = set["placement"]["spacing"].as_f64().unwrap_or(24.);
            let lower = id.to_lowercase();
            let small = [
                "ruin", "meteor", "fossil", "outpost", "wreck", "well", "camp", "house", "tower",
                "dungeon",
            ]
            .iter()
            .any(|word| lower.contains(word));
            let notable = [
                "temple",
                "monument",
                "mansion",
                "stronghold",
                "moonbase",
                "village",
                "pyramid",
                "city",
            ]
            .iter()
            .any(|word| lower.contains(word));
            let factor = if notable {
                2.5
            } else if small {
                0.5
            } else {
                1.
            };
            (spacing * 16. / 24. * factor, notable)
        }
        _ => return None,
    };
    let adjusted = if notable { base * 1.65 } else { base };
    Some(
        [
            0.5, 1., 2., 4., 8., 16., 32., 64., 128., 256., 512., 1024., 2048.,
        ]
        .into_iter()
        .find(|limit| adjusted <= *limit)
        .unwrap_or(2048.),
    )
}
fn predict(
    world: &World,
    bounds: Bounds,
    ores: bool,
    structures: bool,
    text: &str,
    bpp: Option<f64>,
    cancel: &impl Fn() -> bool,
) -> FeatureBatch {
    if !world.dimension.is_planetary()
        || !valid_bounds(bounds)
        || cancel()
        || bpp.is_some_and(|scale| features::lod_tier(scale) == 13)
    {
        return FeatureBatch {
            coverage: FeatureCoverage {
                truncated: cancel(),
                ..Default::default()
            },
            ..Default::default()
        };
    }
    let d = &data()[key(world.dimension)];
    let mut batch = FeatureBatch::default();
    if ores {
        batch = predict_ores(world, bounds, text, bpp, d, cancel);
    }
    if structures && !cancel() {
        let other = predict_structures(world, bounds, text, bpp, d, cancel);
        batch.coverage.truncated |= other.coverage.truncated;
        batch.coverage.scanned_cells += other.coverage.scanned_cells;
        batch.markers.extend(other.markers);
    }
    batch.coverage.marker_count = batch.markers.len();
    batch
}

fn predict_ores(
    world: &World,
    bounds: Bounds,
    query: &str,
    bpp: Option<f64>,
    d: &Value,
    cancel: &impl Fn() -> bool,
) -> FeatureBatch {
    let mut out = Vec::new();
    let mut scanned = 0;
    let mut truncated = false;
    let min_x = (bounds.min_x.floor() as i32).div_euclid(16);
    let max_x = (bounds.max_x.ceil() as i32).div_euclid(16);
    let min_z = (bounds.min_z.floor() as i32).div_euclid(16);
    let max_z = (bounds.max_z.ceil() as i32).div_euclid(16);
    for (id, v) in d["veins"].as_object().into_iter().flatten() {
        if cancel() {
            truncated = true;
            break;
        }
        let typ = v["type"]
            .as_str()
            .unwrap_or("")
            .rsplit(':')
            .next()
            .unwrap_or("");
        let kind = match typ {
            "cluster_vein" => "cluster",
            "disc_vein" => "disc",
            "pipe_vein" => "pipe",
            _ => continue,
        };
        let cfg = &v["config"];
        let Some(rarity) = cfg["rarity"].as_i64().filter(|x| *x > 0) else {
            continue;
        };
        let Some(min_y) = cfg["min_y"].as_i64() else {
            continue;
        };
        let Some(max_y) = cfg["max_y"].as_i64() else {
            continue;
        };
        let size = if kind == "pipe" {
            cfg["height"].as_i64().or(cfg["radius"].as_i64())
        } else {
            cfg["size"].as_i64()
        };
        let Some(size) = size.filter(|x| *x >= 0) else {
            continue;
        };
        let material = materials(cfg);
        let full = format!(
            "{} {} {}",
            id.replace('_', " "),
            material.join(" "),
            world.dimension.label()
        );
        let qualified = format!("{}:{}", key(world.dimension), id);
        if !features::text_matches(&format!("{qualified} {full}"), query) {
            continue;
        }
        let m = Marker {
            id: format!("{qualified}@0,0"),
            label: label(id),
            namespace: "tfg".into(),
            x: 0.,
            z: 0.,
            y: None,
            kind: MarkerKind::Ore,
            confirmed: false,
            min_y: None,
            max_y: None,
            bounds: None,
            detail: String::new(),
            color: ore_color(id),
        };
        if bpp.is_some_and(|scale| scale > features::marker_max_blocks_per_point(&m)) {
            continue;
        }
        let digest = md5::compute(cfg["random_name"].as_str().unwrap_or(id).as_bytes()).0;
        let vein_seed = i64::from_be_bytes(digest[..8].try_into().unwrap())
            ^ i64::from_be_bytes(digest[8..].try_into().unwrap());
        let reach = if kind == "pipe" {
            cfg["radius"].as_i64().unwrap_or(0) as i32
        } else {
            size as i32
        };
        let padding = 1 + (reach >> 4);
        let vertical = if kind == "disc" {
            cfg["height"].as_i64().unwrap_or(size)
        } else {
            size
        } as i32;
        'chunks: for cz in min_z - padding..=max_z + padding {
            for cx in min_x - padding..=max_x + padding {
                if cancel() || scanned >= MAX_CHECKS {
                    truncated = true;
                    break 'chunks;
                }
                scanned += 1;
                let mut rng = Xoroshiro::from_state(
                    (world.seed ^ (cx as i64).wrapping_mul(61728364132)) as u64,
                    (vein_seed ^ (cz as i64).wrapping_mul(16298364123)) as u64,
                );
                if rng.next_int_bound(rarity as i32) != 0 {
                    continue;
                }
                if kind == "pipe" {
                    rng.next_float();
                }
                let x = cx * 16 + rng.next_int_bound(16);
                let y = default_y(min_y as i32, max_y as i32, size as i32, &mut rng);
                let z = cz * 16 + rng.next_int_bound(16);
                let (mx, mz) = match kind {
                    "disc" => {
                        let (dx, dz) = metaball2(&mut rng, size as i32);
                        (x + xround(dx), z + zround(dz))
                    }
                    "cluster" => {
                        let (dx, _, dz) = metaball3(&mut rng, size as i32);
                        (x + xround(dx), z + zround(dz))
                    }
                    _ => (x, z),
                };
                if !bounds.contains(mx as f64, mz as f64) {
                    continue;
                }
                let bottom = y - vertical;
                let top = y + vertical;
                // Native TFC feature generation skips a vein whose entire vertical extent
                // sits above the local ground. Surface height is used only when the
                // planetary sampler can supply it; block-level host replacement remains
                // unverified until the planet's rock layers are ported.
                if world
                    .height(x, z)
                    .is_some_and(|ground| bottom as f64 > ground + 8.0)
                {
                    continue;
                }
                out.push(Marker {
                    id: format!("{qualified}@{cx},{cz}"),
                    label: format!("{} · Y {bottom}–{top}", label(id)),
                    namespace: "tfg".into(),
                    x: mx as f64,
                    z: mz as f64,
                    y: Some(y as f64),
                    kind: MarkerKind::Ore,
                    confirmed: false,
                    min_y: Some(bottom as f64),
                    max_y: Some(top as f64),
                    bounds: None,
                detail: format!(
                    "{} {} vein candidate · Y={y} · host-rock replacements unverified · outputs include {}",
                        world.dimension.label(),
                        kind,
                        material.join(", ")
                    ),
                    color: ore_color(id),
                });
            }
        }
    }
    FeatureBatch {
        coverage: FeatureCoverage {
            truncated,
            scanned_cells: scanned,
            marker_count: out.len(),
        },
        markers: out,
    }
}

fn predict_structures(
    world: &World,
    bounds: Bounds,
    query: &str,
    bpp: Option<f64>,
    d: &Value,
    cancel: &impl Fn() -> bool,
) -> FeatureBatch {
    let min_cx = (bounds.min_x.floor() as i32).div_euclid(16);
    let max_cx = (bounds.max_x.ceil() as i32).div_euclid(16);
    let min_cz = (bounds.min_z.floor() as i32).div_euclid(16);
    let max_cz = (bounds.max_z.ceil() as i32).div_euclid(16);
    let mut markers = Vec::new();
    let mut scanned = 0;
    let mut truncated = false;
    for (_set_id, set) in d["structure_sets"].as_object().into_iter().flatten() {
        let p = &set["placement"];
        let (Some(salt), Some(spacing), Some(separation)) = (
            p["salt"].as_i64(),
            p["spacing"].as_i64(),
            p["separation"].as_i64(),
        ) else {
            continue;
        };
        if spacing <= 0 || separation < 0 || separation >= spacing {
            continue;
        }
        let structures = set["structures"]
            .as_array()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        for st in structures {
            if cancel() {
                truncated = true;
                break;
            }
            let Some(id) = st["structure"].as_str() else {
                continue;
            };
            if !crate::structure_support::biome_compatible(world.dimension, id) {
                continue;
            }
            let qid = id.to_owned();
            if !features::text_matches(
                &format!("{qid} {} {}", label(id), world.dimension.label()),
                query,
            ) {
                continue;
            }
            let (Some(min_chunk_x), Some(max_chunk_x), Some(min_chunk_z), Some(max_chunk_z)) = (
                Some(min_cx.div_euclid(spacing as i32)),
                Some(max_cx.div_euclid(spacing as i32)),
                Some(min_cz.div_euclid(spacing as i32)),
                Some(max_cz.div_euclid(spacing as i32)),
            ) else {
                continue;
            };
            for cz in min_chunk_z..=max_chunk_z {
                for cx in min_chunk_x..=max_chunk_x {
                    if cancel() || scanned >= MAX_CHECKS {
                        truncated = true;
                        break;
                    }
                    scanned += 1;
                    let (chunk_x, chunk_z) = spread(
                        world.seed,
                        salt as i32,
                        spacing as i32,
                        separation as i32,
                        cx,
                        cz,
                    );
                    let x = chunk_x as f64 * 16.0 + 8.;
                    let z = chunk_z as f64 * 16.0 + 8.;
                    if !bounds.contains(x, z) {
                        continue;
                    }
                    let surface_y = world.height(chunk_x * 16 + 8, chunk_z * 16 + 8);
                    let biome_y = surface_y.unwrap_or(world.dimension.default_y() as f64) as i32;
                    let allowed = set["biomes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<HashSet<_>>();
                    if !allowed.is_empty()
                        && !allowed.contains(
                            world
                                .biome_at(chunk_x * 16 + 8, biome_y, chunk_z * 16 + 8)
                                .as_str(),
                        )
                    {
                        continue;
                    }
                    // Vanilla exclusion_zone: candidate's center must be clear of the referenced set.
                    if let Some(ex) = p.get("exclusion_zone") {
                        let other = ex["other_set"].as_str().unwrap_or("");
                        let radius = ex["chunk_count"].as_i64().unwrap_or(0) as i32;
                        if let Some(os) = d["structure_sets"].get(other) {
                            let op = &os["placement"];
                            let other_biomes = os["biomes"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .collect::<HashSet<_>>();
                            if let (Some(osalt), Some(osp), Some(osep)) = (
                                op["salt"].as_i64(),
                                op["spacing"].as_i64(),
                                op["separation"].as_i64(),
                            ) {
                                let bx = chunk_x.div_euclid(osp as i32);
                                let bz = chunk_z.div_euclid(osp as i32);
                                let mut blocked = false;
                                for dz in -radius..=radius {
                                    for dx in -radius..=radius {
                                        let (ox, oz) = spread(
                                            world.seed,
                                            osalt as i32,
                                            osp as i32,
                                            osep as i32,
                                            bx + dx,
                                            bz + dz,
                                        );
                                        let in_allowed_biome = !other_biomes.is_empty()
                                            && other_biomes.contains(
                                                world.biome(ox * 16 + 8, oz * 16 + 8).as_str(),
                                            );
                                        if in_allowed_biome
                                            && (ox - chunk_x).abs() <= radius
                                            && (oz - chunk_z).abs() <= radius
                                        {
                                            blocked = true;
                                            break;
                                        }
                                    }
                                    if blocked {
                                        break;
                                    }
                                }
                                if blocked {
                                    continue;
                                }
                            }
                        }
                    }
                    let biome_note = if surface_y.is_some() {
                        "biome gate checked at sampled surface height"
                    } else {
                        "biome gate sampled at the dimension default Y because surface height is unavailable"
                    };
                    let m = Marker {
                        id: format!("{qid}@{chunk_x},{chunk_z}"),
                        label: id.to_owned(),
                        namespace: id.split(':').next().unwrap_or("tfg").to_owned(),
                        x,
                        z,
                        y: surface_y,
                        kind: MarkerKind::Structure,
                        confirmed: false,
                        min_y: None,
                        max_y: None,
                        bounds: None,
                        detail: format!(
                            "Seed-derived random-spread candidate; {biome_note}; generation and jigsaw footprint unconfirmed."
                        ),
                        color: [230, 173, 85],
                    };
                    if bpp.is_some_and(|s| s > features::marker_max_blocks_per_point(&m)) {
                        continue;
                    }
                    markers.push(m);
                }
            }
        }
    }
    FeatureBatch {
        coverage: FeatureCoverage {
            truncated,
            scanned_cells: scanned,
            marker_count: markers.len(),
        },
        markers,
    }
}

fn spread(seed: i64, salt: i32, spacing: i32, separation: i32, cx: i32, cz: i32) -> (i32, i32) {
    let s = (cx as i64)
        .wrapping_mul(341873128712)
        .wrapping_add((cz as i64).wrapping_mul(132897987541))
        .wrapping_add(seed)
        .wrapping_add(salt as i64);
    let mut r = JavaRandom::new(s);
    let range = spacing - separation;
    (
        cx * spacing + r.next_int_bound(range),
        cz * spacing + r.next_int_bound(range),
    )
}
fn default_y(min: i32, max: i32, shrink: i32, r: &mut Xoroshiro) -> i32 {
    let range = max - min - 2 * shrink;
    if range > 0 {
        min + shrink + r.next_int_bound(range)
    } else {
        (min + max) / 2
    }
}
fn tri(r: &mut Xoroshiro, delta: f64) -> f64 {
    (r.next_double() - r.next_double()) * delta
}
fn metaball2(r: &mut Xoroshiro, size: i32) -> (f64, f64) {
    let min = f32::from(0.1f32 * size as f32) as f64;
    let max = f32::from(0.3f32 * size as f32) as f64;
    let radius = f32::from(0.5f32 * size as f32) as f64;
    let n = 3 + r.next_int_bound(5);
    let (mut total, mut x, mut z) = (0., 0., 0.);
    for _ in 0..n {
        let bx = tri(r, radius);
        let bz = tri(r, radius);
        let w = r.next_double() * (max - min) + min;
        let m = w * w;
        total += m;
        x += m * bx;
        z += m * bz;
    }
    if total == 0. {
        (0., 0.)
    } else {
        (x / total, z / total)
    }
}
fn metaball3(r: &mut Xoroshiro, size: i32) -> (f64, f64, f64) {
    let max = f32::from(0.3f32 * size as f32) as f64;
    let radius = f32::from(0.5f32 * size as f32) as f64;
    let n = 5 + r.next_int_bound(2);
    let (mut total, mut x, mut y, mut z) = (0., 0., 0., 0.);
    for _ in 0..n {
        let bx = tri(r, radius);
        let by = tri(r, radius);
        let bz = tri(r, radius);
        let w = r.next_double() * max;
        let m = w * w * w;
        total += m;
        x += m * bx;
        y += m * by;
        z += m * bz;
    }
    if total == 0. {
        (0., 0., 0.)
    } else {
        (x / total, y / total, z / total)
    }
}
fn xround(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}
fn zround(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}
fn materials(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for b in v["blocks"].as_array().into_iter().flatten() {
        for e in b["with"].as_array().into_iter().flatten() {
            if let Some(id) = e["block"].as_str() {
                if let Some((_, m)) = id.rsplit_once(':') {
                    out.push(m.replace("_ore", "").replace("_", " "));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}
fn ore_color(id: &str) -> [u8; 3] {
    let h = md5::compute(id.as_bytes());
    [95 + h[0] % 140, 75 + h[1] % 130, 60 + h[2] % 150]
}
fn valid_bounds(b: Bounds) -> bool {
    b.min_x.is_finite()
        && b.min_z.is_finite()
        && b.max_x.is_finite()
        && b.max_z.is_finite()
        && b.min_x <= b.max_x
        && b.min_z <= b.max_z
        && (b.max_x - b.min_x) <= 1_000_000.
        && (b.max_z - b.min_z) <= 1_000_000.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_planetary_catalogs_are_dimension_scoped() {
        for dimension in [Dimension::Overworld, Dimension::Beneath] {
            let catalog = feature_catalog(dimension);
            assert!(catalog.0.is_empty() && catalog.1.is_empty());
        }
        let (mo, ms) = feature_catalog(Dimension::Moon);
        let (ra, rs) = feature_catalog(Dimension::Mars);
        assert_eq!(mo.len(), 27);
        assert_eq!(ra.len(), 28);
        assert!(mo.iter().all(|(id, _)| id.starts_with("moon:")));
        assert!(ra.iter().all(|(id, _)| id.starts_with("mars:")));
        assert!(ms.iter().any(|(id, _)| id == "tfg:moon/moonbase"));
        assert!(ms.iter().any(|(id, _)| id == "ad_astra:moon_dungeon"));
        assert!(rs.iter().any(|(id, _)| id == "ad_astra:mars_temple"));
        assert!(ms.iter().all(|(id, _)| id != "ad_astra:mars_temple"));
        assert!(rs.iter().all(|(id, _)| id != "ad_astra:moon_dungeon"));
    }

    #[test]
    fn active_planet_veins_use_supported_tfc_generators_and_no_projection_flags() {
        for dimension in ["moon", "mars"] {
            for vein in data()[dimension]["veins"].as_object().unwrap().values() {
                assert!(matches!(
                    vein["type"].as_str(),
                    Some("tfc:cluster_vein" | "tfc:disc_vein" | "tfc:pipe_vein")
                ));
                assert_eq!(vein["config"]["project"], false);
                assert_eq!(vein["config"]["project_offset"], false);
                assert_eq!(vein["config"]["near_lava"], false);
                assert!(
                    vein["config"]["blocks"]
                        .as_array()
                        .is_some_and(|b| !b.is_empty())
                );
            }
        }
    }

    #[test]
    fn raw_structure_ids_receive_dimension_structure_lod() {
        let marker = |id: &str| Marker {
            id: format!("{id}@0,0"),
            label: id.into(),
            namespace: id.split(':').next().unwrap_or_default().into(),
            x: 0.,
            z: 0.,
            y: None,
            kind: MarkerKind::Structure,
            confirmed: false,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [0; 3],
            bounds: None,
        };
        let meteor = marker("tfg:moon/meteors");
        let base = marker("tfg:moon/moonbase");
        assert!(
            marker_max_blocks_per_point(&meteor).unwrap()
                < marker_max_blocks_per_point(&base).unwrap()
        );
        assert_eq!(
            features::marker_max_blocks_per_point(&base),
            marker_max_blocks_per_point(&base).unwrap()
        );
    }

    #[test]
    fn saved_planet_ore_blocks_fall_near_seed_predicted_veins() {
        let fixture: Value = serde_json::from_str(include_str!("seed0_ores.json")).unwrap();
        assert_eq!(fixture["seed"].as_i64(), Some(0));
        for (dimension_id, dimension) in [
            ("ad_astra:moon", Dimension::Moon),
            ("ad_astra:mars", Dimension::Mars),
        ] {
            let observations = fixture["dimensions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["dimension"] == dimension_id)
                .unwrap()["observations"]
                .as_array()
                .unwrap();
            let feature_data = &data()[key(dimension)];
            let expected = observations
                .iter()
                .filter(|o| {
                    let x = o["x"].as_i64().unwrap_or(i64::MIN);
                    let z = o["z"].as_i64().unwrap_or(i64::MIN);
                    if !(0..1024).contains(&x) || !(0..1024).contains(&z) {
                        return false;
                    }
                    let resource = o["resource"].as_str().unwrap_or_default();
                    feature_data["veins"]
                        .as_object()
                        .unwrap()
                        .values()
                        .any(|vein| {
                            vein["config"]["blocks"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .flat_map(|blocks| blocks["with"].as_array().into_iter().flatten())
                                .any(|output| output["block"] == resource)
                        })
                })
                .collect::<Vec<_>>();
            let bounds = Bounds {
                min_x: -128.,
                min_z: -128.,
                max_x: 1152.,
                max_z: 1152.,
            };
            let world = World::for_dimension(
                0,
                crate::model::Settings::default(),
                dimension,
                dimension.default_y(),
            );
            let batch = predict_matching(&world, bounds, true, false, "", &|| false);
            let mut matched = 0usize;
            for observation in &expected {
                let resource = observation["resource"].as_str().unwrap();
                let x = observation["x"].as_i64().unwrap() as f64;
                let y = observation["y"].as_i64().unwrap() as f64;
                let z = observation["z"].as_i64().unwrap() as f64;
                let vein_ids = feature_data["veins"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(_, vein)| {
                        vein["config"]["blocks"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .flat_map(|blocks| blocks["with"].as_array().into_iter().flatten())
                            .any(|output| output["block"] == resource)
                    })
                    .map(|(id, _)| format!("{}:{id}@", key(dimension)))
                    .collect::<Vec<_>>();
                if batch.markers.iter().any(|marker| {
                    vein_ids.iter().any(|prefix| marker.id.starts_with(prefix))
                        && (marker.x - x).hypot(marker.z - z) <= 64.
                        && marker.min_y.is_some_and(|low| low <= y)
                        && marker.max_y.is_some_and(|high| high >= y)
                }) {
                    matched += 1;
                }
            }
            assert!(
                expected.len() > 25,
                "too few fixture blocks match active veins"
            );
            assert!(
                matched * 2 >= expected.len(),
                "{dimension_id}: only {matched}/{} saved feature-ore blocks near candidates ({} markers, truncated={})",
                expected.len(),
                batch.markers.len(),
                batch.coverage.truncated
            );
        }
    }

    #[test]
    fn seed_queries_return_dimension_scoped_candidates() {
        let bounds = Bounds {
            min_x: -4096.,
            min_z: -4096.,
            max_x: 4096.,
            max_z: 4096.,
        };
        let moon = World::for_dimension(0, crate::model::Settings::default(), Dimension::Moon, 64);
        let ores = predict_matching(&moon, bounds, true, false, "gold", &|| false);
        assert!(
            ores.markers
                .iter()
                .any(|m| m.kind == MarkerKind::Ore && m.id.starts_with("moon:moon_gold@"))
        );
        let structures = predict_matching(&moon, bounds, false, true, "moonbase", &|| false);
        assert!(!structures.markers.is_empty());
        assert!(
            structures
                .markers
                .iter()
                .all(|m| m.id.starts_with("tfg:moon/moonbase@"))
        );
        let dungeon = predict_matching(&moon, bounds, false, true, "moon dungeon", &|| false);
        assert!(dungeon.markers.is_empty());
        let mars = World::for_dimension(0, crate::model::Settings::default(), Dimension::Mars, 88);
        let structures = predict_matching(&mars, bounds, false, true, "mars temple", &|| false);
        // The active TFG Mars biome source has no overlap with Ad Astra's registered
        // mars_temple tag, so the exact biome gate must suppress those spread points.
        assert!(structures.markers.is_empty());
    }
}
