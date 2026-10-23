//! Structure placement and map search helpers.
//!
//! Placed markers from a save are authoritative. Random-spread results here are only
//! seed-derived candidates: biome and structure placement gates are evaluated where
//! available, but mod-specific predicates and terrain checks are not all represented.
use crate::random::{JavaRandom, Xoroshiro};
use crate::{
    model::{Bounds, Climate, Marker, MarkerKind, Point},
    world::World,
};
use serde_json::Value;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const STRUCTURE_DATA: &str = include_str!("../assets/data/tfg/installed-structure-runtime.json");
const TFG_STRUCTURE_DATA: &str = include_str!("../assets/data/tfg/structures.json");
const TFG_VEINS: &str = include_str!("../assets/data/tfg/veins.json");
const TFG_BIOME_TAGS: &str = include_str!("../assets/data/tfg/biome-tags.json");
const TFG_KAOLIN: &str = include_str!("../assets/data/tfg/kaolin.json");
const BENEATH_ORES: &str = include_str!("beneath/ore-features.json");
const MAX_MARKERS: usize = 4_096;
// A 512-block map region spans up to 1,024 cells for spacing-1 mod sets. Keep enough
// headroom to evaluate that normal viewport fully while bounding broad nearest searches.
const STRUCTURE_PLACEMENT_BUDGET: usize = 262_144;
const MAX_ORE_CHECKS: usize = 1_000_000;
const MAX_DIKE_CHECKS: usize = 100_000;
const LOD_LIMITS: [f64; 13] = [
    0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0, 1024.0, 2048.0,
];

/// Stable zoom bucket for viewport feature tile caching. A feature visible at a bucket's
/// maximum remains visible throughout that bucket; tier 13 hides all predicted features.
pub fn lod_tier(blocks_per_point: f64) -> u8 {
    if !blocks_per_point.is_finite() || blocks_per_point < 0.0 {
        return 13;
    }
    LOD_LIMITS
        .iter()
        .position(|limit| blocks_per_point <= *limit)
        .unwrap_or(13) as u8
}

/// Upper scale represented by a visibility cache tier. Tier 13 is the all-hidden tier.
pub fn lod_max_blocks_per_point(tier: u8) -> f64 {
    LOD_LIMITS.get(tier as usize).copied().unwrap_or(0.0)
}

/// Predict only the features that can still be distinguished at the current map scale.
/// The viewport scanner has bounded *work* budgets but no marker-count display cap.
pub fn predict_visible(
    world: &mut World,
    bounds: Bounds,
    ores: bool,
    structures: bool,
    blocks_per_point: f64,
    cancel: &impl Fn() -> bool,
) -> FeatureBatch {
    if !valid_bounds(bounds) || lod_tier(blocks_per_point) == 13 {
        return FeatureBatch::default();
    }
    if cancel() {
        return FeatureBatch {
            markers: Vec::new(),
            coverage: FeatureCoverage {
                truncated: true,
                ..FeatureCoverage::default()
            },
        };
    }
    let seed = world.seed;
    let shared = RefCell::new(world);
    let mut combined = FeatureBatch::default();
    if structures {
        let mut biome = |x, z| shared.borrow_mut().biome(x, z);
        let mut climate = |x, z| Some(shared.borrow_mut().climate(x, z));
        let mut forest = |x, z| shared.borrow_mut().region.forest_type(x, z);
        let mut height = |x, z| shared.borrow_mut().height(x, z).map(|v| v as i32);
        let batch = structure_candidates_internal(
            seed,
            bounds,
            &mut biome,
            &mut climate,
            &mut forest,
            &mut height,
            Some(blocks_per_point),
            false,
            Some(cancel),
        );
        combined.coverage.truncated |= batch.coverage.truncated;
        combined.coverage.scanned_cells += batch.coverage.scanned_cells;
        combined.markers.extend(batch.markers);
    }
    if ores && !cancel() {
        let mut biome = |x, z| shared.borrow_mut().biome(x, z);
        let mut climate = |x, z| Some(shared.borrow_mut().climate(x, z));
        let mut height = |x, z| shared.borrow_mut().height(x, z).map(|v| v as i32);
        let mut rock = |x, y, z| shared.borrow_mut().rock_at(x, y, z);
        let batch = ore_candidates_matching_internal(
            seed,
            bounds,
            "",
            &mut biome,
            &mut climate,
            &mut height,
            &mut rock,
            Some(blocks_per_point),
            false,
            Some(cancel),
        );
        combined.coverage.truncated |= batch.coverage.truncated;
        combined.coverage.scanned_cells += batch.coverage.scanned_cells;
        combined.markers.extend(batch.markers);
    }
    combined.coverage.marker_count = combined.markers.len();
    combined
}

/// Seed-derived Beneath ore and structure candidates. Biome checks use each feature's actual
/// vertical placement because the Nether's biome stack changes substantially with height.
pub fn predict_beneath_visible(
    world: &mut World,
    bounds: Bounds,
    ores: bool,
    structures: bool,
    blocks_per_point: f64,
    cancelled: &impl Fn() -> bool,
) -> FeatureBatch {
    if world.dimension != crate::dimension::Dimension::Beneath || !valid_bounds(bounds) {
        return FeatureBatch::default();
    }
    if lod_tier(blocks_per_point) == 13 || cancelled() {
        return FeatureBatch {
            coverage: FeatureCoverage {
                truncated: cancelled(),
                ..FeatureCoverage::default()
            },
            ..FeatureBatch::default()
        };
    }
    let mut batch = FeatureBatch::default();
    if ores {
        batch = beneath_ore_candidates(world, bounds, "", Some(blocks_per_point), false, cancelled);
    }
    if structures && !cancelled() {
        let b = beneath_structure_candidates(world, bounds, "", Some(blocks_per_point), cancelled);
        batch.coverage.truncated |= b.coverage.truncated;
        batch.coverage.scanned_cells += b.coverage.scanned_cells;
        batch.markers.extend(b.markers);
    }
    batch.coverage.marker_count = batch.markers.len();
    batch
}

/// Search Beneath ore and structure candidates. Every returned position is a prediction.
pub fn predict_beneath_matching(
    world: &World,
    bounds: Bounds,
    ores: bool,
    structures: bool,
    text: &str,
    cancelled: &impl Fn() -> bool,
) -> FeatureBatch {
    if world.dimension != crate::dimension::Dimension::Beneath
        || !valid_bounds(bounds)
        || cancelled()
    {
        return FeatureBatch {
            coverage: FeatureCoverage {
                truncated: cancelled(),
                ..FeatureCoverage::default()
            },
            ..FeatureBatch::default()
        };
    }
    let mut batch = FeatureBatch::default();
    if ores {
        batch = beneath_ore_candidates(world, bounds, text, None, false, cancelled);
    }
    if structures && !cancelled() {
        let b = beneath_structure_candidates(world, bounds, text, None, cancelled);
        batch.coverage.truncated |= b.coverage.truncated;
        batch.coverage.scanned_cells += b.coverage.scanned_cells;
        batch.markers.extend(b.markers);
    }
    batch.coverage.marker_count = batch.markers.len();
    batch
}

fn beneath_ore_candidates(
    world: &World,
    bounds: Bounds,
    query: &str,
    visible_bpp: Option<f64>,
    output_cap: bool,
    cancelled: &impl Fn() -> bool,
) -> FeatureBatch {
    let doc = beneath_data();
    let veins = doc["veins"].as_object().expect("Beneath veins map");
    let tags = &doc["biome_tags"];
    let min_x = (bounds.min_x.floor() as i32).div_euclid(16);
    let max_x = (bounds.max_x.ceil() as i32).div_euclid(16);
    let min_z = (bounds.min_z.floor() as i32).div_euclid(16);
    let max_z = (bounds.max_z.ceil() as i32).div_euclid(16);
    let mut out = Vec::new();
    let mut scanned = 0usize;
    let mut truncated = false;
    for (id, v) in veins {
        if cancelled() {
            truncated = true;
            break;
        }
        let kind = v["type"].as_str().unwrap_or("");
        if !matches!(kind, "disc" | "cluster" | "pipe") {
            continue;
        }
        let materials = material_names(v, &HashSet::new());
        let search = format!(
            "beneath ore mineral {} {}",
            id.replace('_', " "),
            materials.join(" ")
        );
        if !text_matches(&search, query) {
            continue;
        }
        let Some((rarity, min_y, max_y)) = v["rarity"]
            .as_i64()
            .zip(v["min_y"].as_i64())
            .zip(v["max_y"].as_i64())
            .map(|((a, b), c)| (a, b, c))
        else {
            continue;
        };
        let size = if kind == "pipe" {
            v["height"].as_i64().or(v["radius"].as_i64())
        } else {
            v["size"].as_i64()
        };
        let Some(size) = size else { continue };
        if rarity <= 0 || size < 0 {
            continue;
        }
        let fake = Marker {
            id: format!("{id}@0,0"),
            label: id.clone(),
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
            color: [0; 3],
        };
        if visible_bpp.is_some_and(|b| b > marker_max_blocks_per_point(&fake)) {
            continue;
        }
        let digest = md5::compute(v["random_name"].as_str().unwrap_or(id).as_bytes()).0;
        let vein_seed = i64::from_be_bytes(digest[..8].try_into().unwrap())
            ^ i64::from_be_bytes(digest[8..].try_into().unwrap());
        let tag = v["biomes"].as_str().unwrap_or("").trim_start_matches('#');
        let allowed = tags[tag]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect::<HashSet<_>>());
        if !tag.is_empty() && allowed.is_none() {
            continue;
        }
        let reach = if kind == "pipe" {
            v["radius"].as_i64().unwrap_or(0) as i32
        } else {
            size as i32
        };
        // TFC's native chunk-radius helper is 1 + (size >> 4), including discs.
        let padding = 1 + (reach >> 4);
        let vertical = if kind == "disc" {
            v["height"].as_i64().unwrap_or(size)
        } else {
            size
        } as i32;
        'chunks: for cz in min_z - padding..=max_z + padding {
            for cx in min_x - padding..=max_x + padding {
                if cancelled() || scanned >= MAX_ORE_CHECKS {
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
                        let (dx, dz) = metaball_center_2d(&mut rng, size as i32);
                        (x + js_round(dx), z + js_round(dz))
                    }
                    "cluster" => {
                        let (dx, _, dz) = metaball_center_3d(&mut rng, size as i32);
                        (x + js_round(dx), z + js_round(dz))
                    }
                    _ => (x, z),
                };
                if !bounds.contains(mx as f64, mz as f64) {
                    continue;
                }
                if allowed
                    .as_ref()
                    .is_some_and(|set| !set.contains(world.biome_at(x, y, z).as_str()))
                {
                    continue;
                }
                let bottom = y - vertical;
                let top = y + vertical;
                out.push(Marker {
                    id: format!("{id}@{cx},{cz}"),
                    label: format!("{} · Y {bottom}–{top}", id.replace('_', " ")),
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
                        "Beneath {kind} vein · feature biome checked at Y={y} · contains {}",
                        materials.join(", ")
                    ),
                    color: ore_color(id),
                });
                if output_cap && out.len() >= MAX_MARKERS {
                    truncated = true;
                    break 'chunks;
                }
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

fn beneath_data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(BENEATH_ORES).expect("checked Beneath feature asset"))
}

/// Displayable IDs and labels for Beneath ore and structure visibility filters.
pub fn beneath_feature_catalog() -> &'static (Vec<(String, String)>, Vec<(String, String)>) {
    static CATALOG: OnceLock<(Vec<(String, String)>, Vec<(String, String)>)> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let data = beneath_data();
        let label = |id: &str| id.rsplit(':').next().unwrap_or(id).replace(['_', '/'], " ");
        let ores = data["veins"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(id, _)| (id.clone(), label(id)))
            .collect();
        let structures = data["structures"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["id"].as_str().map(|id| (id.to_owned(), label(id))))
            .chain([(
                "minecraft:nether_complexes".to_owned(),
                "Nether fortress / bastion".to_owned(),
            )])
            .collect();
        (ores, structures)
    })
}

fn beneath_structure_candidates(
    world: &World,
    bounds: Bounds,
    query: &str,
    visible_bpp: Option<f64>,
    cancelled: &impl Fn() -> bool,
) -> FeatureBatch {
    const BIOMES: &[&str] = &[
        "tfg:nether/decaying_caverns",
        "tfg:nether/salt_caves",
        "tfg:nether/muggy_bog",
        "tfg:nether/webbed_lair",
        "tfg:nether/lush_hollow",
        "tfg:nether/geothermal_springs",
        "tfg:nether/tar_pools",
        "tfg:nether/thorny_brambles",
        "tfg:nether/basalt_deltas",
        "tfg:nether/ash_forest",
        "tfg:nether/lava_floes",
    ];
    let sets = [
        CandidateSet {
            id: "tfg:beneath/tower".into(),
            salt: 264226226,
            spacing: 15,
            separation: 5,
            triangular: false,
            climate: None,
            climate_placement: false,
            structures: vec![CandidateStructure {
                id: "tfg:beneath/tower".into(),
                weight: 1,
                biomes: BIOMES.iter().map(|s| s.to_string()).collect(),
                conditions: vec![],
            }],
        },
        CandidateSet {
            id: "tfg:beneath/outpost".into(),
            salt: 264552552,
            spacing: 15,
            separation: 5,
            triangular: false,
            climate: None,
            climate_placement: false,
            structures: vec![CandidateStructure {
                id: "tfg:beneath/outpost".into(),
                weight: 1,
                biomes: BIOMES.iter().map(|s| s.to_string()).collect(),
                conditions: vec![],
            }],
        },
        CandidateSet {
            id: "minecraft:nether_complexes".into(),
            salt: 30_084_232,
            spacing: 27,
            separation: 4,
            triangular: false,
            climate: None,
            climate_placement: false,
            structures: vec![CandidateStructure {
                id: "minecraft:nether_complexes".into(),
                weight: 1,
                biomes: BIOMES.iter().map(|s| s.to_string()).collect(),
                conditions: vec![],
            }],
        },
    ];
    let mut out = Vec::new();
    let mut scanned = 0usize;
    let mut truncated = false;
    for set in &sets {
        let search_id = if set.id == "minecraft:nether_complexes" {
            "minecraft:nether_complexes nether fortress bastion remnant"
        } else {
            &set.id
        };
        if !text_matches(search_id, query) {
            continue;
        }
        let fake = Marker {
            id: format!("{}@0,0", set.id),
            label: set.id.clone(),
            namespace: "tfg".into(),
            x: 0.,
            z: 0.,
            y: Some(93.),
            kind: MarkerKind::Structure,
            confirmed: false,
            min_y: None,
            max_y: None,
            bounds: None,
            detail: String::new(),
            color: [0; 3],
        };
        if visible_bpp.is_some_and(|b| b > marker_max_blocks_per_point(&fake)) {
            continue;
        }
        let sx = (bounds.min_x.floor() as i32).div_euclid(16);
        let ex = (bounds.max_x.ceil() as i32).div_euclid(16);
        let sz = (bounds.min_z.floor() as i32).div_euclid(16);
        let ez = (bounds.max_z.ceil() as i32).div_euclid(16);
        let a = sx.div_euclid(set.spacing);
        let b = ex.div_euclid(set.spacing);
        let c = sz.div_euclid(set.spacing);
        let d = ez.div_euclid(set.spacing);
        'grid: for gz in c - 1..=d + 1 {
            for gx in a - 1..=b + 1 {
                if cancelled() || scanned >= STRUCTURE_PLACEMENT_BUDGET {
                    truncated = true;
                    break 'grid;
                }
                scanned += 1;
                let (cx, cz) = potential(world.seed, set, gx, gz);
                let x = cx * 16 + 8;
                let z = cz * 16 + 8;
                if !bounds.contains(x as f64, z as f64)
                    || !set.structures[0].biomes.contains(&world.biome_at(x, 93, z))
                {
                    continue;
                }
                // Placement exclusion zones test the other set's deterministic spread grid.
                // Tower excludes vanilla complexes; outpost excludes biome-valid tower starts.
                if set.id.ends_with("tower")
                    && candidate_near_set(world.seed, &sets[2], cx, cz, 3, None, world)
                {
                    continue;
                }
                if set.id.ends_with("outpost") {
                    if candidate_near_set(
                        world.seed,
                        &sets[0],
                        cx,
                        cz,
                        3,
                        Some(&sets[0].structures[0].biomes),
                        world,
                    ) {
                        continue;
                    }
                }
                let complex = set.id == "minecraft:nether_complexes";
                out.push(Marker {
                    id: format!("{}@{cx},{cz}", set.id),
                    label: if complex { "Nether fortress / bastion".into() } else { set.id.rsplit('/').next().unwrap_or("structure").into() },
                    namespace: if complex { "minecraft".into() } else { "tfg".into() },
                    x: x as f64, z: z as f64, y: Some(93.), kind: MarkerKind::Structure,
                    confirmed: false, min_y: None, max_y: None,
                    bounds: None,
                detail: if complex { "Nether complex placement candidate · may be a fortress or bastion; selected child and terrain generation are not confirmed.".into() } else { "Beneath structure placement candidate · biome checked at Y=93; terrain generation may reject it.".into() },
                    color: [230, 173, 85],
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
fn candidate_near_set(
    seed: i64,
    set: &CandidateSet,
    cx: i32,
    cz: i32,
    radius: i32,
    biome_gate: Option<&HashSet<String>>,
    world: &World,
) -> bool {
    for gz in (cz - radius - 2).div_euclid(set.spacing)..=(cz + radius + 2).div_euclid(set.spacing)
    {
        for gx in
            (cx - radius - 2).div_euclid(set.spacing)..=(cx + radius + 2).div_euclid(set.spacing)
        {
            let (x, z) = potential(seed, set, gx, gz);
            if (x - cx).abs() <= radius
                && (z - cz).abs() <= radius
                && biome_gate
                    .is_none_or(|gate| gate.contains(&world.biome_at(x * 16 + 8, 93, z * 16 + 8)))
            {
                return true;
            }
        }
    }
    false
}

/// Largest map scale at which an automatically predicted feature should remain visible.
/// The cutoffs align exactly with [`lod_tier`], so cached viewport tiles cannot miss features.
pub fn marker_max_blocks_per_point(marker: &Marker) -> f64 {
    let (base, valuable) = match marker.kind {
        MarkerKind::Ore => ore_visibility(marker.id.split('@').next().unwrap_or("")),
        MarkerKind::Structure => {
            structure_visibility(marker.id.split('@').next().unwrap_or(""), &marker.label)
        }
        _ => return f64::INFINITY,
    };
    visibility_cutoff(base, valuable)
}

fn ore_visibility(id: &str) -> (f64, bool) {
    static RARITIES: OnceLock<HashMap<String, (f64, bool)>> = OnceLock::new();
    let values = RARITIES.get_or_init(|| {
        let data: Value = serde_json::from_str(TFG_VEINS).expect("checked in TFG vein table");
        data["veins"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(id, vein)| {
                let rarity = vein["rarity"].as_f64().unwrap_or(1.0).max(1.0);
                let materials = material_names(vein, &HashSet::new())
                    .join(" ")
                    .to_lowercase();
                let name = id.to_lowercase();
                let valuable = [
                    "gold",
                    "silver",
                    "platinum",
                    "diamond",
                    "ruby",
                    "sapphire",
                    "emerald",
                    "amethyst",
                    "native_copper",
                ]
                .iter()
                .any(|word| name.contains(word) || materials.contains(word));
                (
                    id.clone(),
                    // Raw item sprites occupy 22 screen points. Leave space
                    // between occurrences before keeping a type at this zoom.
                    ((16.0 * rarity.sqrt() / 32.0).clamp(0.5, 2048.0), valuable),
                )
            })
            .collect()
    });
    values
        .get(id)
        .copied()
        .unwrap_or_else(|| beneath_ore_visibility(id))
}

fn beneath_ore_visibility(id: &str) -> (f64, bool) {
    static VALUES: OnceLock<HashMap<String, (f64, bool)>> = OnceLock::new();
    VALUES
        .get_or_init(|| {
            let doc = beneath_data();
            doc["veins"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(id, vein)| {
                    let rarity = vein["rarity"].as_f64().unwrap_or(1.).max(1.);
                    let materials = material_names(vein, &HashSet::new())
                        .join(" ")
                        .to_lowercase();
                    let valuable = [
                        "gold", "silver", "platinum", "diamond", "ruby", "sapphire", "emerald",
                        "amethyst",
                    ]
                    .iter()
                    .any(|word| id.to_lowercase().contains(word) || materials.contains(word));
                    (
                        id.clone(),
                        ((16. * rarity.sqrt() / 32.).clamp(0.5, 2048.), valuable),
                    )
                })
                .collect()
        })
        .get(id)
        .copied()
        .unwrap_or((128., false))
}

fn structure_visibility(set_id: &str, structure_id: &str) -> (f64, bool) {
    static SPACING: OnceLock<HashMap<String, f64>> = OnceLock::new();
    let spacings = SPACING.get_or_init(|| {
        let mut values: HashMap<String, f64> = HashMap::new();
        for set in runtime_sets() {
            values.insert(set.id.clone(), set.spacing as f64);
            for structure in &set.structures {
                values
                    .entry(structure.id.clone())
                    .and_modify(|v| *v = v.min(set.spacing as f64))
                    .or_insert(set.spacing as f64);
            }
        }
        values
    });
    let spacing = spacings
        .get(set_id)
        .or_else(|| spacings.get(structure_id))
        .copied()
        .unwrap_or(24.0);
    let set_id = set_id.to_ascii_lowercase();
    let structure_id = structure_id.to_ascii_lowercase();
    let contains = |word: &str| set_id.contains(word) || structure_id.contains(word);
    let small = ["ruin", "hut", "house", "camp", "outpost", "tower"]
        .iter()
        .any(|w| contains(w));
    let notable = [
        "temple",
        "fortress",
        "stronghold",
        "pyramid",
        "village",
        "city",
        "settlement",
        "dungeon",
    ]
    .iter()
    .any(|w| contains(w));
    let pixels_between = spacing * 16.0 / 24.0;
    let factor = if notable {
        2.5
    } else if small {
        0.5
    } else {
        1.0
    };
    ((pixels_between * factor).clamp(0.5, 2048.0), notable)
}

fn visibility_cutoff(base: f64, valuable: bool) -> f64 {
    let adjusted = if valuable { base * 1.65 } else { base };
    LOD_LIMITS
        .iter()
        .copied()
        .find(|limit| adjusted <= *limit)
        .unwrap_or(2048.0)
}

#[derive(Clone, Debug, Default)]
pub struct FeatureCoverage {
    pub truncated: bool,
    pub scanned_cells: usize,
    pub marker_count: usize,
}
#[derive(Clone, Debug, Default)]
pub struct FeatureBatch {
    pub markers: Vec<Marker>,
    pub coverage: FeatureCoverage,
}

#[derive(Clone, Debug)]
struct CandidateSet {
    id: String,
    salt: i32,
    spacing: i32,
    separation: i32,
    triangular: bool,
    climate: Option<ClimateRule>,
    climate_placement: bool,
    structures: Vec<CandidateStructure>,
}
#[derive(Clone, Debug, Default)]
struct ClimateRule {
    min_temperature: Option<f64>,
    max_temperature: Option<f64>,
    min_rainfall: Option<f64>,
    max_rainfall: Option<f64>,
    min_forest: u8,
    max_forest: u8,
}
#[derive(Clone, Debug)]
enum CandidateCondition {
    AvoidBiomes {
        biomes: HashSet<String>,
        radius: i32,
        step: i32,
        allowed: i32,
    },
    Height {
        world_surface: bool,
        min: i32,
        max: i32,
    },
    Unsupported,
}
#[derive(Clone, Debug)]
struct CandidateStructure {
    id: String,
    weight: i32,
    biomes: HashSet<String>,
    conditions: Vec<CandidateCondition>,
}

#[derive(Clone, Copy)]
struct Dike {
    x: i32,
    y: i32,
    z: i32,
    sign: f64,
    skew_x: f64,
    skew_z: f64,
    skew: i32,
    slant: i32,
    radius: i32,
    height: i32,
}
impl Dike {
    fn contains(self, x: i32, y: i32, z: i32) -> bool {
        let dy = y - self.y;
        if dy.abs() >= self.height {
            return false;
        }
        let ys = dy as f64 / self.height as f64;
        let mut dx = (x - self.x) as f64;
        let mut dz = (z - self.z) as f64;
        dx += self.skew as f64 * self.skew_x * ys;
        dz += self.skew as f64 * self.skew_z * ys;
        let yf = self.sign * ys + 0.5;
        let radius = self.radius as f64 * (1. - yf) + (self.radius - self.slant) as f64 * yf;
        dx * dx + dz * dz < radius * radius
    }
}

fn climate_rule(value: &Value) -> Option<ClimateRule> {
    if !value.is_object() {
        return None;
    }
    let forest = |v: &Value, default: u8| match v.as_str().unwrap_or("") {
        "none" => 0,
        "sparse" => 1,
        "edge" => 2,
        "normal" => 3,
        "old_growth" => 4,
        _ => default,
    };
    Some(ClimateRule {
        min_temperature: value["min_temperature"].as_f64(),
        max_temperature: value["max_temperature"].as_f64(),
        min_rainfall: value["min_rainfall"].as_f64(),
        max_rainfall: value["max_rainfall"].as_f64(),
        min_forest: forest(&value["min_forest"], 0),
        max_forest: forest(&value["max_forest"], 4),
    })
}
fn parse_condition(c: &Value) -> CandidateCondition {
    match c["type"].as_str().unwrap_or("") {
        "avoid_biomes" => CandidateCondition::AvoidBiomes {
            biomes: c["biomes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            radius: c["radius"].as_i64().unwrap_or(0) as i32,
            step: c["step"].as_i64().unwrap_or(1).max(1) as i32,
            allowed: c["allowedCount"].as_i64().unwrap_or(0) as i32,
        },
        "height" => CandidateCondition::Height {
            world_surface: c["heightmap"].as_str() == Some("WORLD_SURFACE_WG"),
            min: c["min"].as_i64().unwrap_or(i32::MIN as i64) as i32,
            max: c["max"].as_i64().unwrap_or(i32::MAX as i64) as i32,
        },
        _ => CandidateCondition::Unsupported,
    }
}

fn runtime_sets() -> &'static [CandidateSet] {
    static SETS: OnceLock<Vec<CandidateSet>> = OnceLock::new();
    SETS.get_or_init(build_runtime_sets)
}
fn build_runtime_sets() -> Vec<CandidateSet> {
    let doc: Value =
        serde_json::from_str(STRUCTURE_DATA).expect("checked in structure runtime data");
    let definitions = doc["definitions"].as_array().expect("definitions array");
    let mut raw_tags: HashMap<String, Vec<String>> = HashMap::new();
    let mut structures: HashMap<String, HashSet<String>> = HashMap::new();
    for item in definitions {
        let id = item["id"].as_str().unwrap_or_default();
        if item["kind"] == "biome_tag" {
            let members = item["data"]["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    entry
                        .as_str()
                        .or_else(|| entry["id"].as_str())
                        .map(str::to_owned)
                })
                .collect();
            raw_tags.insert(id.to_owned(), members);
        }
    }
    fn resolve_tag(
        id: &str,
        raw: &HashMap<String, Vec<String>>,
        seen: &mut HashSet<String>,
        out: &mut HashSet<String>,
    ) {
        if !seen.insert(id.to_owned()) {
            return;
        }
        if let Some(members) = raw.get(id) {
            for member in members {
                if let Some(nested) = member.strip_prefix('#') {
                    resolve_tag(nested, raw, seen, out);
                } else {
                    out.insert(member.clone());
                }
            }
        }
        seen.remove(id);
    }
    let tags: HashMap<String, HashSet<String>> = raw_tags
        .keys()
        .map(|id| {
            let mut out = HashSet::new();
            resolve_tag(id, &raw_tags, &mut HashSet::new(), &mut out);
            (id.clone(), out)
        })
        .collect();
    for item in definitions.iter().filter(|d| d["kind"] == "structure") {
        let id = item["id"].as_str().unwrap_or_default();
        let biome_tag = item["data"]["biomes"].as_str().unwrap_or_default();
        if let Some(tag) = biome_tag.strip_prefix('#') {
            if let Some(members) = tags.get(tag) {
                structures.insert(id.to_owned(), members.clone());
            }
        } else if !biome_tag.is_empty() {
            structures.insert(id.to_owned(), [biome_tag.to_owned()].into_iter().collect());
        }
    }
    let mut out = Vec::new();
    for item in definitions.iter().filter(|d| d["kind"] == "structure_set") {
        let placement = &item["data"]["placement"];
        if placement["type"].as_str() != Some("minecraft:random_spread") {
            continue;
        }
        let (Some(salt), Some(spacing), Some(separation)) = (
            placement["salt"].as_i64(),
            placement["spacing"].as_i64(),
            placement["separation"].as_i64(),
        ) else {
            continue;
        };
        if spacing <= 0 || separation < 0 || separation >= spacing {
            continue;
        }
        let entries = item["data"]["structures"].as_array().into_iter().flatten();
        let mut unresolved_biome = false;
        let candidates = entries
            .filter_map(|entry| {
                let id = entry["structure"].as_str()?.to_owned();
                let weight = entry["weight"]
                    .as_i64()
                    .unwrap_or(1)
                    .clamp(1, i32::MAX as i64) as i32;
                let Some(biomes) = structures.get(&id).cloned() else {
                    unresolved_biome = true;
                    return None;
                };
                Some(CandidateStructure {
                    id,
                    weight,
                    biomes,
                    conditions: Vec::new(),
                })
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() || unresolved_biome {
            continue;
        }
        let frequency = placement["frequency"].as_f64().unwrap_or(1.0);
        if frequency != 1.0
            || placement["frequency_reduction_method"]
                .as_str()
                .is_some_and(|m| m != "default")
        {
            continue;
        }
        out.push(CandidateSet {
            id: item["id"].as_str().unwrap_or("unknown:set").to_owned(),
            salt: salt as i32,
            spacing: spacing as i32,
            separation: separation as i32,
            triangular: placement["spread_type"].as_str() == Some("triangular"),
            climate: None,
            climate_placement: false,
            structures: candidates,
        });
    }
    // TFG's verified structure definitions include climate-spread settlements not in
    // the general installed-mod inventory. Preserve those set definitions as candidate data.
    let tfg: Value =
        serde_json::from_str(TFG_STRUCTURE_DATA).expect("checked in TFG structure data");
    for set in tfg["sets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["dimension"] == "overworld")
    {
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
        let mut entries = Vec::new();
        for s in set["structures"].as_array().into_iter().flatten() {
            let Some(id) = s["id"].as_str() else {
                continue;
            };
            let biomes = s["biomes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            entries.push(CandidateStructure {
                id: id.to_owned(),
                weight: s["weight"].as_i64().unwrap_or(1).max(1) as i32,
                biomes,
                conditions: s["conditions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(parse_condition)
                    .collect(),
            });
        }
        if !entries.is_empty() {
            out.retain(|other| other.id != set["id"].as_str().unwrap_or_default());
            out.push(CandidateSet {
                id: set["id"].as_str().unwrap_or("tfg:unknown").to_owned(),
                salt: salt as i32,
                spacing: spacing as i32,
                separation: separation as i32,
                triangular: p["spreadType"] == "triangular",
                climate: climate_rule(&p["climate"]),
                climate_placement: p["type"] == "tfc:climate",
                structures: entries,
            });
        }
    }
    out
}

fn potential(seed: i64, set: &CandidateSet, cell_x: i32, cell_z: i32) -> (i32, i32) {
    let seed = (cell_x as i64)
        .wrapping_mul(341873128712)
        .wrapping_add((cell_z as i64).wrapping_mul(132897987541))
        .wrapping_add(seed)
        .wrapping_add(set.salt as i64);
    let mut rng = JavaRandom::new(seed);
    let range = set.spacing - set.separation;
    let offset = |r: &mut JavaRandom| {
        if set.triangular {
            (r.next_int_bound(range) + r.next_int_bound(range)) / 2
        } else {
            r.next_int_bound(range)
        }
    };
    (
        cell_x
            .wrapping_mul(set.spacing)
            .wrapping_add(offset(&mut rng)),
        cell_z
            .wrapping_mul(set.spacing)
            .wrapping_add(offset(&mut rng)),
    )
}

/// Return the nearest placement cells to the centre first, without allocating a grid that
/// grows with the requested search radius. The frontier is ordered by exact squared distance.
fn centered_cells(
    min_x: i32,
    max_x: i32,
    min_z: i32,
    max_z: i32,
    center_x: i32,
    center_z: i32,
    limit: usize,
) -> Vec<(i32, i32)> {
    if limit == 0 || min_x > max_x || min_z > max_z {
        return Vec::new();
    }
    let sx = center_x.clamp(min_x, max_x);
    let sz = center_z.clamp(min_z, max_z);
    let mut heap = BinaryHeap::new();
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(limit);
    let push = |x: i32,
                z: i32,
                heap: &mut BinaryHeap<Reverse<(i64, i32, i32)>>,
                seen: &mut HashSet<(i32, i32)>| {
        if x < min_x || x > max_x || z < min_z || z > max_z || !seen.insert((x, z)) {
            return;
        }
        let dx = x as i64 - center_x as i64;
        let dz = z as i64 - center_z as i64;
        heap.push(Reverse((dx * dx + dz * dz, z, x)));
    };
    push(sx, sz, &mut heap, &mut seen);
    while out.len() < limit {
        let Some(Reverse((_, z, x))) = heap.pop() else {
            break;
        };
        out.push((x, z));
        push(x - 1, z, &mut heap, &mut seen);
        push(x + 1, z, &mut heap, &mut seen);
        push(x, z - 1, &mut heap, &mut seen);
        push(x, z + 1, &mut heap, &mut seen);
    }
    out
}

/// Generate seed-derived installed structure candidates intersecting a block rectangle.
/// `biome_at` receives block coordinates at the candidate chunk origin. Empty biome lists
/// mean that the installed table does not provide an evaluable biome gate.
pub fn structure_candidates<F>(seed: i64, bounds: Bounds, mut biome_at: F) -> Vec<Marker>
where
    F: FnMut(i32, i32) -> String,
{
    structure_candidates_with_coverage(seed, bounds, &mut biome_at).markers
}

pub fn structure_candidates_with_coverage<F>(
    seed: i64,
    bounds: Bounds,
    mut biome_at: F,
) -> FeatureBatch
where
    F: FnMut(i32, i32) -> String,
{
    let mut no_climate = |_: i32, _: i32| None;
    let mut no_forest = |_: i32, _: i32| 0;
    let mut no_height = |_: i32, _: i32| None;
    structure_candidates_internal(
        seed,
        bounds,
        &mut biome_at,
        &mut no_climate,
        &mut no_forest,
        &mut no_height,
        None,
        true,
        None,
    )
}

/// Full native TFG structure gates. Climate, forest, biome and height probes are sampled at
/// the candidate chunk's minimum corner, matching the existing seed generator's contract.
pub fn structure_candidates_with_world<B, C, F, H>(
    seed: i64,
    bounds: Bounds,
    mut biome_at: B,
    mut climate_at: C,
    mut forest_at: F,
    mut surface_y: H,
) -> FeatureBatch
where
    B: FnMut(i32, i32) -> String,
    C: FnMut(i32, i32) -> Option<Climate>,
    F: FnMut(i32, i32) -> u8,
    H: FnMut(i32, i32) -> Option<i32>,
{
    structure_candidates_internal(
        seed,
        bounds,
        &mut biome_at,
        &mut climate_at,
        &mut forest_at,
        &mut surface_y,
        None,
        true,
        None,
    )
}

fn structure_candidates_internal<B, C, F, H>(
    seed: i64,
    bounds: Bounds,
    biome_at: &mut B,
    climate_at: &mut C,
    forest_at: &mut F,
    surface_y: &mut H,
    visible_bpp: Option<f64>,
    output_cap: bool,
    cancel: Option<&dyn Fn() -> bool>,
) -> FeatureBatch
where
    B: FnMut(i32, i32) -> String,
    C: FnMut(i32, i32) -> Option<Climate>,
    F: FnMut(i32, i32) -> u8,
    H: FnMut(i32, i32) -> Option<i32>,
{
    if !valid_bounds(bounds) {
        return FeatureBatch::default();
    }
    let sets = runtime_sets();
    let min_cx = (bounds.min_x.floor() as i32).div_euclid(16);
    let max_cx = (bounds.max_x.ceil() as i32).div_euclid(16);
    let min_cz = (bounds.min_z.floor() as i32).div_euclid(16);
    let max_cz = (bounds.max_z.ceil() as i32).div_euclid(16);
    let mut out = Vec::new();
    let mut scanned_cells = 0usize;
    let mut truncated = false;
    let center_cx = ((min_cx as i64 + max_cx as i64) / 2) as i32;
    let center_cz = ((min_cz as i64 + max_cz as i64) / 2) as i32;
    let quota = std::cmp::max(1, STRUCTURE_PLACEMENT_BUDGET / sets.len().max(1));
    let remainder = STRUCTURE_PLACEMENT_BUDGET.saturating_sub(quota * sets.len());
    for (set_index, set) in sets.iter().enumerate() {
        if cancel.is_some_and(|stop| stop()) {
            truncated = true;
            break;
        }
        if let Some(bpp) = visible_bpp {
            if !set.structures.iter().any(|s| {
                let (base, notable) = structure_visibility(&set.id, &s.id);
                bpp <= visibility_cutoff(base, notable)
            }) {
                continue;
            }
        }
        let sx = set.spacing;
        let min_x = min_cx.div_euclid(sx);
        let max_x = max_cx.div_euclid(sx);
        let min_z = min_cz.div_euclid(sx);
        let max_z = max_cz.div_euclid(sx);
        let cells =
            (max_x as i64 - min_x as i64 + 1).saturating_mul(max_z as i64 - min_z as i64 + 1);
        if cells <= 0 {
            continue;
        }
        let limit = quota + usize::from(set_index < remainder);
        if cells as usize > limit {
            truncated = true;
        }
        let cells = centered_cells(
            min_x,
            max_x,
            min_z,
            max_z,
            center_cx.div_euclid(sx),
            center_cz.div_euclid(sx),
            limit.min(cells as usize),
        );
        for (cx, cz) in cells {
            if cancel.is_some_and(|stop| stop()) {
                truncated = true;
                break;
            }
            scanned_cells += 1;
            let (chunk_x, chunk_z) = potential(seed, &set, cx, cz);
            let x = chunk_x as f64 * 16.0 + 8.0;
            let z = chunk_z as f64 * 16.0 + 8.0;
            if !bounds.contains(x, z) {
                continue;
            }
            let biome = biome_at(chunk_x * 16, chunk_z * 16);
            if set.climate_placement {
                let Some(rule) = &set.climate else {
                    continue;
                };
                let Some(climate) = climate_at(chunk_x * 16, chunk_z * 16) else {
                    continue;
                };
                let forest = forest_at(chunk_x, chunk_z);
                if rule
                    .min_temperature
                    .is_some_and(|v| climate.temperature < v)
                    || rule
                        .max_temperature
                        .is_some_and(|v| climate.temperature > v)
                    || rule.min_rainfall.is_some_and(|v| climate.rainfall < v)
                    || rule.max_rainfall.is_some_and(|v| climate.rainfall > v)
                    || forest < rule.min_forest
                    || forest > rule.max_forest
                {
                    continue;
                }
            }
            let entries: Vec<_> = set.structures.iter().collect();
            let Some(entry) = weighted_order(seed, chunk_x, chunk_z, entries)
                .into_iter()
                .find(|s| {
                    s.biomes.contains(&biome)
                        && conditions_allow(
                            &s.conditions,
                            biome_at,
                            surface_y,
                            chunk_x * 16,
                            chunk_z * 16,
                        )
                })
            else {
                continue;
            };
            if let Some(bpp) = visible_bpp {
                let (base, notable) = structure_visibility(&set.id, &entry.id);
                if bpp > visibility_cutoff(base, notable) {
                    continue;
                }
            }
            out.push(Marker {
                id: format!("{}@{},{}", set.id, chunk_x, chunk_z),
                label: entry.id.clone(),
                namespace: entry.id.split(':').next().unwrap_or("minecraft").to_owned(),
                x,
                z,
                y: None,
                kind: MarkerKind::Structure,
                confirmed: false,
                min_y: None,
                max_y: None,
                bounds: None,
                detail: "Seed-derived placement candidate; generation is not confirmed.".into(),
                color: [230, 173, 85],
            });
            if output_cap && out.len() >= MAX_MARKERS {
                truncated = true;
                break;
            }
        }
        if output_cap && out.len() >= MAX_MARKERS {
            break;
        }
    }
    FeatureBatch {
        coverage: FeatureCoverage {
            truncated,
            scanned_cells,
            marker_count: out.len(),
        },
        markers: out,
    }
}

fn conditions_allow<B, H>(
    conditions: &[CandidateCondition],
    biome_at: &mut B,
    surface_y: &mut H,
    x: i32,
    z: i32,
) -> bool
where
    B: FnMut(i32, i32) -> String,
    H: FnMut(i32, i32) -> Option<i32>,
{
    for condition in conditions {
        match condition {
            CandidateCondition::AvoidBiomes {
                biomes,
                radius,
                step,
                allowed,
            } => {
                let mut count = 0;
                for dz in (-*radius..=*radius).step_by(*step as usize) {
                    for dx in (-*radius..=*radius).step_by(*step as usize) {
                        let sampled = biome_at(x + dx, z + dz);
                        if !sampled.is_empty() && biomes.contains(&sampled) {
                            count += 1;
                            if count > *allowed {
                                return false;
                            }
                        }
                    }
                }
            }
            CandidateCondition::Height {
                world_surface,
                min,
                max,
            } => {
                let Some(ground) = surface_y(x, z) else {
                    return false;
                };
                let h = if *world_surface {
                    ground.max(63)
                } else {
                    ground
                };
                if h < *min || h > *max {
                    return false;
                }
            }
            CandidateCondition::Unsupported => return false,
        }
    }
    true
}

fn structure_large_feature_seed(seed: i64, x: i32, z: i32) -> i64 {
    let mut r = JavaRandom::new(seed);
    let l = r.next_long();
    let m = r.next_long();
    (x as i64).wrapping_mul(l) ^ (z as i64).wrapping_mul(m) ^ seed
}
fn weighted_order<'a>(
    seed: i64,
    x: i32,
    z: i32,
    mut entries: Vec<&'a CandidateStructure>,
) -> Vec<&'a CandidateStructure> {
    if entries.len() <= 1 {
        return entries;
    }
    let mut r = JavaRandom::new(structure_large_feature_seed(seed, x, z));
    let mut order = Vec::with_capacity(entries.len());
    while !entries.is_empty() {
        let total: i32 = entries.iter().map(|e| e.weight).sum();
        if total <= 0 {
            order.append(&mut entries);
            break;
        }
        let mut pick = r.next_int_bound(total);
        let mut at = 0;
        for entry in &entries {
            pick -= entry.weight;
            if pick < 0 {
                break;
            }
            at += 1;
        }
        order.push(entries.remove(at.min(entries.len() - 1)));
    }
    order
}
fn valid_bounds(b: Bounds) -> bool {
    b.min_x.is_finite()
        && b.min_z.is_finite()
        && b.max_x.is_finite()
        && b.max_z.is_finite()
        && b.min_x <= b.max_x
        && b.min_z <= b.max_z
}
fn rock_name_from_key(key: &str) -> Option<String> {
    let name = key.rsplit('/').next()?.split(':').last()?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// Quiet mineral colors stay recognizable on both land and ocean backdrops.
fn ore_color(id: &str) -> [u8; 3] {
    let tokens: Vec<_> = id.split([':', '_', '-']).collect();
    for (names, color) in [
        (&["copper", "tetrahedrite", "malachite"][..], [208, 145, 88]),
        (
            &["iron", "hematite", "magnetite", "limonite"][..],
            [187, 114, 102],
        ),
        (&["tin", "cassiterite"][..], [166, 191, 204]),
        (&["gold"][..], [225, 190, 97]),
        (&["silver", "platinum"][..], [206, 210, 223]),
        (&["coal", "graphite", "lignite"][..], [113, 122, 134]),
        (&["nickel", "garnierite", "emerald"][..], [115, 183, 139]),
        (&["lapis", "lazurite", "sapphire"][..], [103, 147, 220]),
        (&["ruby", "redstone"][..], [208, 111, 136]),
        (&["kaolin", "gypsum", "borax"][..], [232, 216, 199]),
    ] {
        if names.iter().any(|name| tokens.contains(name)) {
            return color;
        }
    }
    let h = id
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    [
        120 + (h % 95) as u8,
        125 + ((h >> 8) % 85) as u8,
        130 + ((h >> 16) % 75) as u8,
    ]
}

pub fn text_matches(text: &str, query: &str) -> bool {
    let text = text.to_lowercase().replace(['_', '/', ':', '-'], " ");
    query
        .to_lowercase()
        .replace(['_', '/', ':', '-'], " ")
        .split_whitespace()
        .all(|term| text.contains(term))
}
fn material_names(vein: &Value, known: &HashSet<String>) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    if let Some(blocks) = vein["blocks"].as_object() {
        for (key, entries) in blocks {
            let rock = rock_name_from_key(key).unwrap_or_default();
            for entry in entries.as_array().into_iter().flatten() {
                let block = entry["block"].as_str().unwrap_or("");
                let path = block.split_once(':').map_or(block, |(_, p)| p);
                if path.is_empty() || path.starts_with("rock/") {
                    continue;
                }
                let mut name = if path.starts_with("ore/") {
                    path.split('/').nth(1).unwrap_or("").to_owned()
                } else {
                    path.rsplit('/').next().unwrap_or(path).to_owned()
                };
                name = name
                    .strip_prefix("raw_")
                    .unwrap_or(&name)
                    .trim_end_matches("_ore")
                    .trim_end_matches("_block")
                    .to_owned();
                if !rock.is_empty() {
                    name = name
                        .strip_prefix(&format!("{rock}_"))
                        .unwrap_or(&name)
                        .to_owned();
                }
                if let Some(other) = known
                    .iter()
                    .find(|r| name.starts_with(&(r.to_string() + "_")) && name.len() > r.len() + 1)
                {
                    name = name[other.len() + 1..].to_owned();
                }
                if !name.is_empty() {
                    names.insert(name.replace('_', " "));
                }
            }
        }
    }
    names.into_iter().collect()
}

fn produces_material(vein: &Value, known_rocks: &HashSet<String>) -> bool {
    vein["blocks"].as_object().is_some_and(|blocks| {
        blocks.iter().any(|(rock_key, entries)| {
            let rock = rock_name_from_key(rock_key).unwrap_or_default();
            entries.as_array().into_iter().flatten().any(|entry| {
                let block = entry["block"].as_str().unwrap_or_default();
                if block.is_empty() {
                    return false;
                }
                let path = block.split_once(':').map_or(block, |(_, p)| p);
                if path.starts_with("ore/") {
                    return true;
                }
                if path.starts_with("rock/") {
                    return false;
                }
                let mut name = path
                    .rsplit('/')
                    .next()
                    .unwrap_or(path)
                    .strip_prefix("raw_")
                    .unwrap_or(path.rsplit('/').next().unwrap_or(path))
                    .trim_end_matches("_ore")
                    .trim_end_matches("_block")
                    .to_owned();
                if !rock.is_empty() {
                    name = name
                        .strip_prefix(&(rock.clone() + "_"))
                        .unwrap_or(&name)
                        .to_owned();
                }
                known_rocks
                    .iter()
                    .any(|r| name.starts_with(&(r.clone() + "_")) && name.len() > r.len() + 1)
                    || !name.is_empty()
            })
        })
    })
}

/// Stable distance search across all loaded saved markers plus optional bounded seed candidates.
/// Exact starts sort before predictions at an identical distance. Use a full save scan for `saved`
/// to make this independent of the current map viewport.
pub fn nearest_structures(
    point: Point,
    radius: f64,
    saved: &[Marker],
    candidates: &[Marker],
    limit: usize,
) -> Vec<Marker> {
    if !point.x.is_finite() || !point.z.is_finite() || !radius.is_finite() || radius < 0.0 {
        return Vec::new();
    }
    let mut ranked: Vec<(f64, bool, String, Marker)> = saved
        .iter()
        .chain(candidates.iter())
        .filter(|m| matches!(m.kind, MarkerKind::Structure))
        .map(|m| (point.distance(m.point()), !m.confirmed, m.key(), m.clone()))
        .filter(|(d, _, _, _)| *d <= radius)
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut seen = HashSet::new();
    ranked
        .into_iter()
        .filter_map(|(_, _, key, m)| if seen.insert(key) { Some(m) } else { None })
        .take(limit.min(4096))
        .collect()
}

/// Generate TFG vein placement candidates using the source shape center and placement gates.
/// They remain predictions because a map cannot prove all individual block replacements.
pub fn ore_candidates<B, S, R>(
    seed: i64,
    bounds: Bounds,
    biome_at: B,
    surface_y: S,
    rock_at: R,
) -> FeatureBatch
where
    B: FnMut(i32, i32) -> String,
    S: FnMut(i32, i32) -> Option<i32>,
    R: FnMut(i32, i32, i32) -> Option<String>,
{
    ore_candidates_with_climate(seed, bounds, biome_at, |_, _| None, surface_y, rock_at)
}

pub fn ore_candidates_with_climate<B, C, S, R>(
    seed: i64,
    bounds: Bounds,
    biome_at: B,
    climate_at: C,
    surface_y: S,
    rock_at: R,
) -> FeatureBatch
where
    B: FnMut(i32, i32) -> String,
    C: FnMut(i32, i32) -> Option<Climate>,
    S: FnMut(i32, i32) -> Option<i32>,
    R: FnMut(i32, i32, i32) -> Option<String>,
{
    ore_candidates_matching(seed, bounds, "", biome_at, climate_at, surface_y, rock_at)
}

pub fn ore_candidates_matching<B, C, S, R>(
    seed: i64,
    bounds: Bounds,
    query: &str,
    mut biome_at: B,
    mut climate_at: C,
    mut surface_y: S,
    mut rock_at: R,
) -> FeatureBatch
where
    B: FnMut(i32, i32) -> String,
    C: FnMut(i32, i32) -> Option<Climate>,
    S: FnMut(i32, i32) -> Option<i32>,
    R: FnMut(i32, i32, i32) -> Option<String>,
{
    ore_candidates_matching_internal(
        seed,
        bounds,
        query,
        &mut biome_at,
        &mut climate_at,
        &mut surface_y,
        &mut rock_at,
        None,
        true,
        None,
    )
}

fn ore_candidates_matching_internal<B, C, S, R>(
    seed: i64,
    bounds: Bounds,
    query: &str,
    mut biome_at: B,
    mut climate_at: C,
    mut surface_y: S,
    mut rock_at: R,
    visible_bpp: Option<f64>,
    output_cap: bool,
    cancel: Option<&dyn Fn() -> bool>,
) -> FeatureBatch
where
    B: FnMut(i32, i32) -> String,
    C: FnMut(i32, i32) -> Option<Climate>,
    S: FnMut(i32, i32) -> Option<i32>,
    R: FnMut(i32, i32, i32) -> Option<String>,
{
    if !valid_bounds(bounds) {
        return FeatureBatch::default();
    }
    static ORE_DATA: OnceLock<(Value, Value)> = OnceLock::new();
    let (raw, tags_doc) = ORE_DATA.get_or_init(|| {
        (
            serde_json::from_str(TFG_VEINS).expect("checked in TFG vein table"),
            serde_json::from_str(TFG_BIOME_TAGS).expect("checked in TFG biome tags"),
        )
    });
    let mut out = Vec::new();
    let mut scanned = 0usize;
    let mut truncated = false;
    let min_x = (bounds.min_x.floor() as i32).div_euclid(16);
    let max_x = (bounds.max_x.ceil() as i32).div_euclid(16);
    let min_z = (bounds.min_z.floor() as i32).div_euclid(16);
    let max_z = (bounds.max_z.ceil() as i32).div_euclid(16);
    let Some(veins) = raw["veins"].as_object() else {
        return FeatureBatch::default();
    };
    let known_rocks: HashSet<String> = veins
        .values()
        .filter_map(|v| v["blocks"].as_object())
        .flat_map(|blocks| blocks.keys().filter_map(|key| rock_name_from_key(key)))
        .collect();
    // Rock dikes only affect ore host checks. Once those ore types have faded
    // out, scanning dikes would consume the work budget for invisible features.
    let active_veins = visible_bpp.is_none_or(|bpp| {
        veins.iter().any(|(id, v)| {
            let kind = v["type"].as_str().unwrap_or("");
            matches!(kind, "disc" | "cluster" | "pipe")
                && !is_dike_definition(v)
                && v["near_lava"].as_bool() != Some(true)
                && produces_material(v, &known_rocks)
                && {
                    let (base, valuable) = ore_visibility(id);
                    bpp <= visibility_cutoff(base, valuable)
                }
        })
    });
    let (dikes, dikes_truncated) = if active_veins {
        dikes_for(seed, bounds, veins, cancel)
    } else {
        (Vec::new(), false)
    };
    truncated |= dikes_truncated;
    for (vein_id, v) in veins {
        if cancel.is_some_and(|stop| stop()) {
            truncated = true;
            break;
        }
        let kind = v["type"].as_str().unwrap_or("");
        if kind != "disc" && kind != "cluster" && kind != "pipe" {
            continue;
        }
        if kind == "pipe" && is_dike_definition(v) {
            continue;
        }
        if v["near_lava"].as_bool() == Some(true) {
            continue;
        }
        if !produces_material(v, &known_rocks) {
            continue;
        }
        let materials = material_names(v, &known_rocks);
        let searchable = format!(
            "ore mineral {} {}",
            vein_id.replace('_', " "),
            materials.join(" ")
        );
        if !text_matches(&searchable, query) {
            continue;
        }
        let (Some(name), Some(rarity), Some(min_y), Some(max_y)) = (
            v["random_name"].as_str(),
            v["rarity"].as_i64(),
            v["min_y"].as_i64(),
            v["max_y"].as_i64(),
        ) else {
            continue;
        };
        let Some(size) = (if kind == "pipe" {
            v["height"].as_i64().or(v["radius"].as_i64())
        } else {
            v["size"].as_i64()
        }) else {
            continue;
        };
        if rarity <= 0 || size < 0 {
            continue;
        }
        if visible_bpp.is_some_and(|bpp| {
            let marker = Marker {
                id: format!("{vein_id}@0,0"),
                label: vein_id.clone(),
                namespace: "tfg".into(),
                x: 0.0,
                z: 0.0,
                y: None,
                kind: MarkerKind::Ore,
                confirmed: false,
                min_y: None,
                max_y: None,
                bounds: None,
                detail: String::new(),
                color: [0; 3],
            };
            bpp > marker_max_blocks_per_point(&marker)
        }) {
            continue;
        }
        let vertical_extent = if kind == "disc" {
            v["height"].as_i64().unwrap_or(size)
        } else {
            size
        };
        if kind == "pipe"
            && (v["height"].as_i64().is_none()
                || v["radius"].as_i64().is_none()
                || v["min_skew"].as_i64().is_none()
                || v["max_skew"].as_i64().is_none()
                || v["min_slant"].as_i64().is_none()
                || v["max_slant"].as_i64().is_none()
                || v["sign"].as_f64().is_none())
        {
            continue;
        }
        let digest = md5::compute(name.as_bytes()).0;
        let lo = i64::from_be_bytes(digest[0..8].try_into().unwrap());
        let hi = i64::from_be_bytes(digest[8..16].try_into().unwrap());
        let vein_seed = lo ^ hi;
        let tag_id = v["biomes"].as_str().unwrap_or("").trim_start_matches('#');
        let allowed = if tag_id.is_empty() {
            None
        } else {
            tags_doc["tags"][tag_id]
                .as_array()
                .map(|arr| arr.iter().filter_map(Value::as_str).collect::<HashSet<_>>())
        };
        if !tag_id.is_empty() && allowed.is_none() {
            continue;
        }
        let project = v["project"].as_bool() == Some(true);
        let host_rocks = v["blocks"]
            .as_object()
            .map(|o| {
                o.keys()
                    .filter_map(|key| rock_name_from_key(key))
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let reach = if kind == "pipe" {
            v["radius"].as_i64().unwrap_or(0) as i32
        } else {
            size as i32
        };
        let radius_chunks = if kind == "disc" { 0 } else { 1 + (reach >> 4) };
        for cz in (min_z - radius_chunks)..=(max_z + radius_chunks) {
            for cx in (min_x - radius_chunks)..=(max_x + radius_chunks) {
                if cancel.is_some_and(|stop| stop()) {
                    truncated = true;
                    break;
                }
                if scanned >= MAX_ORE_CHECKS {
                    truncated = true;
                    break;
                }
                scanned += 1;
                let wx = (cx as i64).wrapping_mul(61728364132);
                let wz = (cz as i64).wrapping_mul(16298364123);
                let mut rng = Xoroshiro::from_state((seed ^ wx) as u64, (vein_seed ^ wz) as u64);
                if rng.next_int_bound(rarity as i32) != 0 {
                    continue;
                }
                if kind == "pipe" {
                    rng.next_float();
                }
                let x = cx * 16 + rng.next_int_bound(16);
                let y0 = default_y(min_y as i32, max_y as i32, size as i32, &mut rng);
                let z = cz * 16 + rng.next_int_bound(16);
                let (mx, mz) = match kind {
                    "disc" => {
                        let (dx, dz) = metaball_center_2d(&mut rng, size as i32);
                        (x + js_round(dx), z + js_round(dz))
                    }
                    "cluster" => {
                        let (cx, _, cz) = metaball_center_3d(&mut rng, size as i32);
                        (x + js_round(cx), z + js_round(cz))
                    }
                    _ => (x, z),
                };
                if kind == "pipe" {
                    let _sign = (v["sign"].as_f64().unwrap_or(0.) as f32) < rng.next_float();
                    let skew = uniform_int(
                        &mut rng,
                        v["min_skew"].as_i64().unwrap_or(0) as i32,
                        1 + v["max_skew"].as_i64().unwrap_or(0) as i32,
                    );
                    let _slant = uniform_int(
                        &mut rng,
                        v["min_slant"].as_i64().unwrap_or(0) as i32,
                        1 + v["max_slant"].as_i64().unwrap_or(0) as i32,
                    );
                    let _ = skew;
                }
                let surface = surface_y(x, z);
                if project && surface.is_none() {
                    continue;
                }
                let y = y0 + if project { surface.unwrap() } else { 0 };
                let bottom = y - vertical_extent as i32;
                let top = y + vertical_extent as i32;
                if let Some(ground) = surface {
                    if bottom > ground + 8 {
                        continue;
                    }
                }
                if let Some(members) = &allowed {
                    if !members.contains(biome_at(x, z).as_str()) {
                        continue;
                    }
                }
                if dikes.iter().any(|d| d.contains(mx, (bottom + top) / 2, mz)) {
                    continue;
                }
                if !host_rocks.is_empty() {
                    if let Some(rock) = rock_at(mx, (bottom + top) / 2, mz) {
                        let rock = rock.strip_prefix("raw_").unwrap_or(&rock);
                        if !host_rocks.contains(rock) {
                            continue;
                        }
                    }
                }
                let px = mx as f64;
                let pz = mz as f64;
                out.push(Marker {
                    id: format!("{vein_id}@{cx},{cz}"),
                    label: format!("{}", vein_id.replace('_', " ")),
                    namespace: "tfg".into(),
                    x: px,
                    z: pz,
                    y: Some(y as f64),
                    kind: MarkerKind::Ore,
                    confirmed: false,
                    min_y: Some(bottom as f64),
                    max_y: Some(top as f64),
                    bounds: None,
                    detail: format!("{kind} vein · contains {}", materials.join(", ")),
                    color: ore_color(vein_id),
                });
                if output_cap && out.len() >= MAX_MARKERS {
                    truncated = true;
                    break;
                }
            }
            if (output_cap && out.len() >= MAX_MARKERS) || scanned >= MAX_ORE_CHECKS {
                break;
            }
        }
        if (output_cap && out.len() >= MAX_MARKERS) || scanned >= MAX_ORE_CHECKS {
            break;
        }
    }
    if !truncated
        && (!output_cap || out.len() < MAX_MARKERS)
        && text_matches("kaolin clay ore mineral", query)
        && visible_bpp.is_none_or(|bpp| {
            let marker = Marker {
                id: "kaolin_disc@0,0".into(),
                label: "kaolin".into(),
                namespace: "tfg".into(),
                x: 0.0,
                z: 0.0,
                y: None,
                kind: MarkerKind::Ore,
                confirmed: false,
                min_y: None,
                max_y: None,
                bounds: None,
                detail: String::new(),
                color: [0; 3],
            };
            bpp <= marker_max_blocks_per_point(&marker)
        })
    {
        static KAOLIN: OnceLock<Value> = OnceLock::new();
        let kaolin = KAOLIN
            .get_or_init(|| serde_json::from_str(TFG_KAOLIN).expect("checked in TFG kaolin data"));
        let config = &kaolin["configuredFeature"]["config"];
        let placement = &kaolin["placedFeature"]["placement"][0];
        if let (Some(name), Some(rarity), Some(size), Some(height), Some(min_y), Some(max_y)) = (
            config["random_name"].as_str(),
            config["rarity"].as_i64(),
            config["size"].as_i64(),
            config["height"].as_i64(),
            config["min_y"].as_i64(),
            config["max_y"].as_i64(),
        ) {
            let digest = md5::compute(name.as_bytes()).0;
            let vein_seed = i64::from_be_bytes(digest[0..8].try_into().unwrap())
                ^ i64::from_be_bytes(digest[8..16].try_into().unwrap());
            let allowed: HashSet<&str> = kaolin["biomes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            for cz in min_z..=max_z {
                for cx in min_x..=max_x {
                    if cancel.is_some_and(|stop| stop())
                        || scanned >= MAX_ORE_CHECKS
                        || (output_cap && out.len() >= MAX_MARKERS)
                    {
                        truncated = true;
                        break;
                    }
                    scanned += 1;
                    let mut rng = Xoroshiro::from_state(
                        (seed ^ (cx as i64).wrapping_mul(61728364132)) as u64,
                        (vein_seed ^ (cz as i64).wrapping_mul(16298364123)) as u64,
                    );
                    if rng.next_int_bound(rarity as i32) != 0 {
                        continue;
                    }
                    let x = cx * 16 + rng.next_int_bound(16);
                    let y = default_y(min_y as i32, max_y as i32, size as i32, &mut rng);
                    let z = cz * 16 + rng.next_int_bound(16);
                    let (dx, dz) = metaball_center_2d(&mut rng, size as i32);
                    if !allowed.contains(biome_at(x, z).as_str()) {
                        continue;
                    }
                    let x0 = (x - size as i32).div_euclid(16);
                    let x1 = (x + size as i32).div_euclid(16);
                    let z0 = (z - size as i32).div_euclid(16);
                    let z1 = (z + size as i32).div_euclid(16);
                    let mut climate_ok = false;
                    'climate: for tz in z0..=z1 {
                        for tx in x0..=x1 {
                            if let Some(c) = climate_at(tx * 16, tz * 16) {
                                if c.temperature
                                    >= placement["min_temperature"]
                                        .as_f64()
                                        .unwrap_or(f64::INFINITY)
                                    && c.rainfall
                                        >= placement["min_rainfall"]
                                            .as_f64()
                                            .unwrap_or(f64::INFINITY)
                                {
                                    climate_ok = true;
                                    break 'climate;
                                }
                            }
                        }
                    }
                    if !climate_ok {
                        continue;
                    }
                    let px = (x + js_round(dx)) as f64;
                    let pz = (z + js_round(dz)) as f64;
                    out.push(Marker {
                        id: format!("kaolin_disc@{cx},{cz}"),
                        label: "Kaolin".into(),
                        namespace: "tfc".into(),
                        x: px,
                        z: pz,
                        y: Some(y as f64),
                        kind: MarkerKind::Ore,
                        confirmed: false,
                        min_y: Some(y as f64 - height as f64),
                        max_y: Some(y as f64 + height as f64),
                        bounds: None,
                        detail: "Kaolin clay ore · climate and biome gates pass.".into(),
                        color: [232, 216, 199],
                    });
                }
                if scanned >= MAX_ORE_CHECKS || (output_cap && out.len() >= MAX_MARKERS) {
                    break;
                }
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

fn default_y(min: i32, max: i32, shrink: i32, rng: &mut Xoroshiro) -> i32 {
    let range = max - min - 2 * shrink;
    if range > 0 {
        min + shrink + rng.next_int_bound(range)
    } else {
        (min + max) / 2
    }
}
fn uniform_int(rng: &mut Xoroshiro, min: i32, max_exclusive: i32) -> i32 {
    if min == max_exclusive {
        min
    } else {
        min + rng.next_int_bound(max_exclusive - min)
    }
}

fn js_round(x: f64) -> i32 {
    (x + 0.5).floor() as i32
}
fn triangle(rng: &mut Xoroshiro, delta: f64) -> f64 {
    (rng.next_double() - rng.next_double()) * delta
}
fn metaball_center_2d(rng: &mut Xoroshiro, size: i32) -> (f64, f64) {
    let min = f32::from(0.1f32 * size as f32) as f64;
    let max = f32::from(0.3f32 * size as f32) as f64;
    let radius = f32::from(0.5f32 * size as f32) as f64;
    let count = 3 + rng.next_int_bound(5);
    let (mut total, mut x, mut z) = (0., 0., 0.);
    for _ in 0..count {
        let bx = triangle(rng, radius);
        let bz = triangle(rng, radius);
        let w = rng.next_double() * (max - min) + min;
        let mass = w * w;
        total += mass;
        x += mass * bx;
        z += mass * bz;
    }
    if total == 0. {
        (0., 0.)
    } else {
        (x / total, z / total)
    }
}
fn metaball_center_3d(rng: &mut Xoroshiro, size: i32) -> (f64, f64, f64) {
    let max = f32::from(0.3f32 * size as f32) as f64;
    let radius = f32::from(0.5f32 * size as f32) as f64;
    let count = 5 + rng.next_int_bound(2);
    let (mut total, mut x, mut y, mut z) = (0., 0., 0., 0.);
    for _ in 0..count {
        let bx = triangle(rng, radius);
        let by = triangle(rng, radius);
        let bz = triangle(rng, radius);
        let w = rng.next_double() * max;
        let mass = w * w * w;
        total += mass;
        x += mass * bx;
        y += mass * by;
        z += mass * bz;
    }
    if total == 0. {
        (0., 0., 0.)
    } else {
        (x / total, y / total, z / total)
    }
}

fn is_dike_definition(v: &Value) -> bool {
    let Some(blocks) = v["blocks"].as_object() else {
        return false;
    };
    let all: Vec<&str> = blocks
        .values()
        .flat_map(|entries| entries.as_array().into_iter().flatten())
        .filter_map(|e| e["block"].as_str())
        .collect();
    !all.is_empty()
        && all
            .iter()
            .all(|b| b.contains(":rock/raw/") || b.contains(":rock/hardened/"))
}

fn dikes_for(
    seed: i64,
    bounds: Bounds,
    veins: &serde_json::Map<String, Value>,
    cancel: Option<&dyn Fn() -> bool>,
) -> (Vec<Dike>, bool) {
    static SIN: OnceLock<Vec<f32>> = OnceLock::new();
    let table = SIN.get_or_init(|| {
        (0..65536)
            .map(|i| (i as f64 * std::f64::consts::TAU / 65536.).sin() as f32)
            .collect()
    });
    let sin = |v: f32| table[((v * 10430.378f32) as i32 & 65535) as usize] as f64;
    let cos = |v: f32| table[((v * 10430.378f32 + 16384.) as i32 & 65535) as usize] as f64;
    let min_cx = (bounds.min_x.floor() as i32).div_euclid(16);
    let max_cx = (bounds.max_x.ceil() as i32).div_euclid(16);
    let min_cz = (bounds.min_z.floor() as i32).div_euclid(16);
    let max_cz = (bounds.max_z.ceil() as i32).div_euclid(16);
    let mut out = Vec::new();
    let mut scanned = 0usize;
    let mut truncated = false;
    for v in veins
        .values()
        .filter(|v| v["type"] == "pipe" && is_dike_definition(v))
    {
        let (
            Some(name),
            Some(rarity),
            Some(radius),
            Some(height),
            Some(min_y),
            Some(max_y),
            Some(max_skew),
        ) = (
            v["random_name"].as_str(),
            v["rarity"].as_i64(),
            v["radius"].as_i64(),
            v["height"].as_i64(),
            v["min_y"].as_i64(),
            v["max_y"].as_i64(),
            v["max_skew"].as_i64(),
        )
        else {
            continue;
        };
        if rarity <= 0 {
            continue;
        }
        let digest = md5::compute(name.as_bytes()).0;
        let name_seed = i64::from_be_bytes(digest[0..8].try_into().unwrap())
            ^ i64::from_be_bytes(digest[8..16].try_into().unwrap());
        let reach = radius as i32 + max_skew as i32;
        let chunk_radius = 1 + (reach >> 4);
        for cz in min_cz - chunk_radius..=max_cz + chunk_radius {
            for cx in min_cx - chunk_radius..=max_cx + chunk_radius {
                if cancel.is_some_and(|stop| stop()) {
                    truncated = true;
                    break;
                }
                if scanned >= MAX_DIKE_CHECKS {
                    truncated = true;
                    break;
                }
                scanned += 1;
                let mut rng = Xoroshiro::from_state(
                    (seed ^ (cx as i64).wrapping_mul(61728364132)) as u64,
                    (name_seed ^ (cz as i64).wrapping_mul(16298364123)) as u64,
                );
                if rng.next_int_bound(rarity as i32) != 0 {
                    continue;
                }
                let angle = rng.next_float() * std::f32::consts::TAU;
                let x = cx * 16 + rng.next_int_bound(16);
                let y = default_y(min_y as i32, max_y as i32, height as i32, &mut rng);
                let z = cz * 16 + rng.next_int_bound(16);
                let sign = if v["sign"].as_f64().unwrap_or(0.) < rng.next_float() as f64 {
                    1.
                } else {
                    -1.
                };
                let skew = uniform_int(
                    &mut rng,
                    v["min_skew"].as_i64().unwrap_or(0) as i32,
                    1 + max_skew as i32,
                );
                let slant = uniform_int(
                    &mut rng,
                    v["min_slant"].as_i64().unwrap_or(0) as i32,
                    1 + v["max_slant"].as_i64().unwrap_or(0) as i32,
                );
                out.push(Dike {
                    x,
                    y,
                    z,
                    sign,
                    skew_x: cos(angle),
                    skew_z: sin(angle),
                    skew,
                    slant,
                    radius: radius as i32,
                    height: height as i32,
                });
            }
            if scanned >= MAX_DIKE_CHECKS {
                break;
            }
        }
    }
    (out, truncated)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_beneath_asset_has_all_native_ore_shapes_and_biome_tags() {
        let data = beneath_data();
        let veins = data["veins"].as_object().unwrap();
        assert_eq!(veins.len(), 29);
        let mut shapes = HashMap::<&str, usize>::new();
        for vein in veins.values() {
            *shapes.entry(vein["type"].as_str().unwrap()).or_default() += 1;
            assert_eq!(vein["project"], false);
            assert_eq!(vein["project_offset"], false);
            assert_eq!(vein["near_lava"], false);
            let tag = vein["biomes"].as_str().unwrap().trim_start_matches('#');
            assert!(
                !data["biome_tags"][tag].as_array().unwrap().is_empty(),
                "{tag}"
            );
        }
        assert_eq!(shapes.get("cluster"), Some(&19));
        assert_eq!(shapes.get("pipe"), Some(&7));
        assert_eq!(shapes.get("disc"), Some(&3));
        assert_eq!(data["structures"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn beneath_candidates_are_seeded_y_aware_and_searchable() {
        let mut world = World::for_dimension(
            -6696614430994881185,
            crate::model::Settings::default(),
            crate::dimension::Dimension::Beneath,
            96,
        );
        let bounds = Bounds {
            min_x: -512.,
            min_z: -512.,
            max_x: 512.,
            max_z: 512.,
        };
        let full = predict_beneath_visible(&mut world, bounds, true, false, 0.25, &|| false);
        assert!(!full.markers.is_empty());
        assert!(full.markers.iter().all(|m| !m.confirmed
            && m.y.is_some_and(|y| (0. ..=207.).contains(&y))
            && m.min_y.is_some()
            && m.max_y.is_some()));
        let gold = predict_beneath_matching(&world, bounds, true, false, "gold", &|| false);
        assert!(!gold.markers.is_empty());
        assert!(
            gold.markers
                .iter()
                .all(|m| m.id.contains("gold") || m.detail.to_lowercase().contains("gold"))
        );
        let hidden = predict_beneath_visible(&mut world, bounds, true, true, 4096., &|| false);
        assert!(hidden.markers.is_empty());
    }

    fn marker(id: &str, label: &str, kind: MarkerKind) -> Marker {
        Marker {
            id: id.into(),
            label: label.into(),
            namespace: "tfg".into(),
            x: 0.0,
            z: 0.0,
            y: None,
            kind,
            confirmed: false,
            min_y: None,
            max_y: None,
            bounds: None,
            detail: String::new(),
            color: [0; 3],
        }
    }
    #[test]
    fn lod_buckets_are_monotonic_and_have_an_all_hidden_tier() {
        let scales = [
            0.0, 0.5, 0.6, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0, 1024.0,
            2048.0, 2048.1, 4096.0,
        ];
        let tiers: Vec<_> = scales.iter().map(|scale| lod_tier(*scale)).collect();
        assert!(tiers.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(lod_tier(f64::NAN), 13);
        assert_eq!(lod_tier(2048.0), 12);
        assert_eq!(lod_tier(2048.1), 13);
        assert_eq!(lod_tier(4096.0), 13);
    }
    #[test]
    fn invisible_ore_types_do_not_scan_hidden_dikes_or_exhaust_the_work_budget() {
        let batch = ore_candidates_matching_internal(
            42,
            Bounds {
                min_x: -1_000_000.,
                min_z: -1_000_000.,
                max_x: 1_000_000.,
                max_z: 1_000_000.,
            },
            "",
            |_, _| panic!("hidden ore requested a biome"),
            |_, _| panic!("hidden ore requested climate"),
            |_, _| panic!("hidden ore requested terrain"),
            |_, _, _| panic!("hidden ore requested host rock"),
            Some(2048.),
            false,
            Some(&|| false),
        );
        assert!(batch.markers.is_empty());
        assert!(!batch.coverage.truncated);
        assert_eq!(batch.coverage.scanned_cells, 0);
    }
    #[test]
    fn common_ores_and_small_ruins_fade_before_rare_valuable_features() {
        let common_ore = marker("surface_coal@1,2", "surface coal", MarkerKind::Ore);
        let rare_ore = marker("normal_gold@1,2", "normal gold", MarkerKind::Ore);
        assert!(marker_max_blocks_per_point(&common_ore) < marker_max_blocks_per_point(&rare_ore));

        let ruin = marker(
            "tfg:illagers/illager_camps@1,2",
            "tfg:illagers/camps",
            MarkerKind::Structure,
        );
        let village = marker(
            "tfg:illagers/illager_camps@1,2",
            "tfg:illagers/village",
            MarkerKind::Structure,
        );
        assert!(marker_max_blocks_per_point(&ruin) < marker_max_blocks_per_point(&village));
    }
    #[test]
    fn marker_lod_thresholds_are_quantized_and_ids_ignore_placement_coordinates() {
        let first = marker("normal_gold@1,2", "normal gold", MarkerKind::Ore);
        let second = marker("normal_gold@900,-400", "normal gold", MarkerKind::Ore);
        let cutoff = marker_max_blocks_per_point(&first);
        assert_eq!(cutoff, marker_max_blocks_per_point(&second));
        assert!(LOD_LIMITS.contains(&cutoff));
        assert!(
            marker_max_blocks_per_point(&marker(
                "normal_copper@0,0",
                "normal copper",
                MarkerKind::Ore
            )) <= 2048.0
        );
    }
    #[test]
    fn random_spread_candidate_is_stable_and_in_bounds() {
        let b = Bounds {
            min_x: -512.,
            min_z: -512.,
            max_x: 512.,
            max_z: 512.,
        };
        let a = structure_candidates(7787531943268589495, b, |_, _| "tfg:earth/plains".into());
        let c = structure_candidates(7787531943268589495, b, |_, _| "tfg:earth/plains".into());
        assert_eq!(
            a.iter().map(Marker::key).collect::<Vec<_>>(),
            c.iter().map(Marker::key).collect::<Vec<_>>()
        );
        assert!(a.iter().all(|m| b.contains(m.x, m.z)));
        assert!(a.iter().any(|m| matches!(m.kind, MarkerKind::Structure)));
    }
    #[test]
    fn structure_candidate_cap_is_reported() {
        let b = Bounds {
            min_x: -1.0e7,
            min_z: -1.0e7,
            max_x: 1.0e7,
            max_z: 1.0e7,
        };
        let batch = structure_candidates_with_coverage(42, b, |_, _| "tfg:earth/plains".into());
        assert!(batch.coverage.truncated);
    }
    #[test]
    fn nearest_search_uses_radius_stable_ties_and_prefers_confirmed() {
        let make = |id: &str, x: f64, confirmed: bool| Marker {
            id: id.into(),
            label: id.into(),
            namespace: "tfg".into(),
            x,
            z: 0.,
            y: None,
            kind: MarkerKind::Structure,
            confirmed,
            min_y: None,
            max_y: None,
            bounds: None,
            detail: String::new(),
            color: [0; 3],
        };
        let p = Point { x: 0., z: 0. };
        let predicted = make("same", 3., false);
        let confirmed = make("same", 3., true);
        let far = make("far", 8., true);
        let got = nearest_structures(p, 5., &[predicted.clone(), far], &[confirmed.clone()], 10);
        assert_eq!(got.len(), 1);
        assert!(got[0].confirmed);
    }
    #[test]
    fn java_y_range_matches_feature_shrink_behavior() {
        let mut random = Xoroshiro::from_state(1, 2);
        let y = default_y(-20, 30, 5, &mut random);
        assert!((-15..25).contains(&y));
        let mut random = Xoroshiro::from_state(1, 2);
        assert_eq!(default_y(-20, 30, 30, &mut random), 5);
    }
    #[test]
    fn tfg_ore_candidates_apply_repeatable_world_gates() {
        let b = Bounds {
            min_x: -512.,
            min_z: -512.,
            max_x: 512.,
            max_z: 512.,
        };
        let build = || {
            ore_candidates_with_climate(
                7787531943268589495,
                b,
                |_, _| "tfg:earth/plains".into(),
                |_, _| {
                    Some(Climate {
                        temperature: 20.,
                        rainfall: 350.,
                    })
                },
                |_, _| Some(200),
                |_, _, _| Some("granite".into()),
            )
        };
        let a = build();
        let c = build();
        assert!(!a.markers.is_empty());
        assert_eq!(
            a.markers.iter().map(Marker::key).collect::<Vec<_>>(),
            c.markers.iter().map(Marker::key).collect::<Vec<_>>()
        );
        assert!(
            a.markers
                .iter()
                .all(|m| m.x.is_finite() && m.z.is_finite() && !m.confirmed && m.min_y <= m.max_y)
        );
    }
}
