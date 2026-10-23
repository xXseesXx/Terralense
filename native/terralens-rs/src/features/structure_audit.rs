//! Uncapped 100 km diagnostic structure candidate scan for the installed TFG source profile.
//!
//! One record is emitted for each actual weighted structure-set member in each supported
//! dimension. `candidate_found` is only a biome/placement-gate candidate; structure start checks,
//! terrain checks and saved generated starts are separate evidence.

use super::*;
use crate::{dimension::Dimension, model::Settings};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const APPLICABILITY: &str = include_str!("../../assets/data/tfg/structure-applicability.json");
const RADIUS: i32 = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    CandidateFound,
    NoValidCandidateWithinRadius,
    SourceBiomeIncompatible,
    UnsupportedPlacement,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructureResult {
    pub dimension: String,
    pub set_id: String,
    pub structure_id: String,
    pub status: Status,
    pub x: Option<i32>,
    pub z: Option<i32>,
    pub distance: Option<f64>,
    pub candidates_checked: u64,
    pub biome_rejections: u64,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DimensionResult {
    pub dimension: String,
    pub placement_sets: usize,
    pub structures: Vec<StructureResult>,
    pub saved_start_evidence: Option<SavedStartEvidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedStartEvidence {
    pub generated_chunks_scanned: usize,
    pub starts: usize,
    pub starts_with_bounds: usize,
    pub decode_errors: usize,
    pub nearest_starts: Vec<SavedStart>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedStart {
    pub structure_id: String,
    pub x: f64,
    pub z: f64,
    pub distance: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub seed: i64,
    pub center: [i32; 2],
    pub radius_blocks: i32,
    pub dimensions: Vec<DimensionResult>,
    pub evidence_limit: String,
}

/// Compare every emitted audit witness against the dimension's production candidate scanner
/// using a tight bounds query centered on that exact placement. The result lists any witnesses
/// the UI/search backend would fail to return for the same seed and coordinates.
pub fn verify_report_witnesses(report: &Report) -> Vec<String> {
    let mut mismatches = Vec::new();
    for dim in &report.dimensions {
        let Some(dimension) = Dimension::from_id(&dim.dimension) else {
            mismatches.push(format!("{}: unsupported dimension", dim.dimension));
            continue;
        };
        let slice_y = if dimension == Dimension::Beneath {
            93
        } else {
            dimension.default_y()
        };
        let world = World::for_dimension(report.seed, Settings::default(), dimension, slice_y);
        for result in dim
            .structures
            .iter()
            .filter(|result| matches!(&result.status, Status::CandidateFound))
        {
            let (Some(x), Some(z)) = (result.x, result.z) else {
                mismatches.push(format!(
                    "{} {}: candidate status has no coordinates",
                    dim.dimension, result.structure_id
                ));
                continue;
            };
            let bounds = Bounds {
                min_x: x as f64 - 0.25,
                max_x: x as f64 + 0.25,
                min_z: z as f64 - 0.25,
                max_z: z as f64 + 0.25,
            };
            let markers = match dimension {
                Dimension::Overworld => {
                    let mut biome = |bx, bz| world.biome(bx, bz);
                    let mut climate = |bx, bz| Some(world.climate(bx, bz));
                    let mut forest = |cx, cz| world.region.forest_type(cx, cz);
                    let mut height = |hx, hz| world.height(hx, hz).map(|value| value as i32);
                    structure_candidates_with_world(
                        report.seed,
                        bounds,
                        &mut biome,
                        &mut climate,
                        &mut forest,
                        &mut height,
                    )
                    .markers
                }
                Dimension::Beneath => {
                    predict_beneath_matching(&world, bounds, false, true, "", &|| false).markers
                }
                Dimension::Moon | Dimension::Mars => {
                    crate::planetary::features::predict_matching(
                        &world,
                        bounds,
                        false,
                        true,
                        "",
                        &|| false,
                    )
                    .markers
                }
            };
            let found = markers.iter().any(|marker| {
                marker.id.split('@').next() == Some(result.structure_id.as_str())
                    && (marker.map_point().x - x as f64).abs() < 0.01
                    && (marker.map_point().z - z as f64).abs() < 0.01
            });
            if !found {
                mismatches.push(format!(
                    "{} {} at ({x},{z}) is absent from the production scanner",
                    dim.dimension, result.structure_id
                ));
            }
        }
    }
    mismatches
}

#[derive(Clone)]
struct AuditMember {
    id: String,
    weight: i32,
    biomes: HashSet<String>,
    conditions: Vec<CandidateCondition>,
}

#[derive(Clone)]
struct AuditSet {
    id: String,
    salt: i32,
    spacing: i32,
    separation: i32,
    triangular: bool,
    climate: Option<ClimateRule>,
    placement_supported: bool,
    unsupported_reason: Option<String>,
    exclusion: Option<(String, i32)>,
    members: Vec<AuditMember>,
}

/// Run the installed source inventory at seed 0, centered on block coordinate (0,0), in a
/// 100,000-block circle. No viewer marker or candidate budget is used.
pub fn scan_reference_world() -> Report {
    let source: Value =
        serde_json::from_str(APPLICABILITY).expect("checked structure applicability inventory");
    let mut dimensions = Vec::new();
    for dimension in Dimension::ALL {
        let sample_y = if dimension == Dimension::Beneath {
            93
        } else {
            dimension.default_y()
        };
        let mut world = World::for_dimension(0, Settings::default(), dimension, sample_y);
        let (sets, universe) = source_sets(&source, dimension);
        let mut structures = Vec::new();
        for set in &sets {
            structures.extend(scan_set(&mut world, set, &universe, &sets));
        }
        structures.sort_by(|a, b| {
            a.set_id
                .cmp(&b.set_id)
                .then(a.structure_id.cmp(&b.structure_id))
        });
        dimensions.push(DimensionResult {
            dimension: dimension.id().to_owned(),
            placement_sets: sets.len(),
            structures,
            saved_start_evidence: None,
        });
    }
    add_saved_start_evidence(&mut dimensions);
    Report {
        schema: 1,
        seed: 0,
        center: [0, 0],
        radius_blocks: RADIUS,
        dimensions,
        evidence_limit: "A seed-derived random-spread placement has selected the weighted child and passed the installed biome and climate gates. This does not claim that Minecraft successfully generated a structure start or that terrain-specific placement checks passed; saved Anvil starts are listed as separate evidence.".into(),
    }
}

/// Read any already-generated seed-zero saves in the configured Prism instance. This only
/// opens Anvil region files through the existing read-only scanner; it never launches Minecraft
/// or writes to a save. It can independently confirm structure IDs and locations.
fn add_saved_start_evidence(dimensions: &mut [DimensionResult]) {
    let Ok(worlds) = crate::prism::discover_worlds(&crate::prism::default_instance()) else {
        return;
    };
    for saved_world in worlds.into_iter().filter(|world| world.seed == 0) {
        for dim in saved_world.dimensions {
            let Some(result) = dimensions.iter_mut().find(|r| r.dimension == dim.id) else {
                continue;
            };
            let Ok(scan) = crate::prism::scan_structures(&dim.path) else {
                continue;
            };
            let mut starts = scan
                .markers
                .iter()
                .filter(|m| {
                    let p = m.map_point();
                    p.x.hypot(p.z) <= RADIUS as f64
                })
                .collect::<Vec<_>>();
            starts.sort_by(|a, b| {
                a.map_point()
                    .x
                    .hypot(a.map_point().z)
                    .total_cmp(&b.map_point().x.hypot(b.map_point().z))
            });
            let starts_count = starts.len();
            let starts_with_bounds = starts.iter().filter(|m| m.bounds.is_some()).count();
            let mut seen_types = HashSet::new();
            let nearest_starts = starts
                .into_iter()
                .filter_map(|m| {
                    let p = m.map_point();
                    let id = if m.id.contains(':') {
                        m.id.clone()
                    } else {
                        format!("{}:{}", m.namespace, m.id)
                    };
                    // Native structure-start IDs may append a placement/child suffix after
                    // `@`; report the nearest evidence once per actual registry type.
                    let structure_id = id.split('@').next().unwrap_or(&id).to_owned();
                    seen_types
                        .insert(structure_id.clone())
                        .then_some(SavedStart {
                            structure_id,
                            x: p.x,
                            z: p.z,
                            distance: p.x.hypot(p.z),
                        })
                })
                .collect();
            result.saved_start_evidence = Some(SavedStartEvidence {
                generated_chunks_scanned: scan.chunks,
                starts: starts_count,
                starts_with_bounds,
                decode_errors: scan.errors.len(),
                nearest_starts,
            });
        }
    }
}

fn source_sets(source: &Value, dimension: Dimension) -> (Vec<AuditSet>, HashSet<String>) {
    let dimension_id = dimension.id();
    let mut profiles = source["supported_dimensions"]
        .as_array()
        .into_iter()
        .flatten();
    let universe = profiles
        .find(|entry| entry["id"] == dimension_id)
        .and_then(|entry| entry["biomes"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    let mut sets = Vec::new();
    let definitions = source["structures"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|record| Some((record["id"].as_str()?, &record["definition"])))
        .collect::<HashMap<_, _>>();
    for set in source["sets"].as_array().into_iter().flatten() {
        let p = &set["placement"];
        let placement_type = p["type"].as_str().unwrap_or("unknown");
        let placement_supported =
            matches!(placement_type, "minecraft:random_spread" | "tfc:climate");
        let (salt, spacing, separation) = (
            p["salt"].as_i64(),
            p["spacing"].as_i64(),
            p["separation"].as_i64(),
        );
        let members = set["children"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|child| {
                let id = child["id"].as_str()?;
                let biomes = child["dimension_biomes"][dimension_id]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                Some(AuditMember {
                    id: id.to_owned(),
                    weight: child["weight"]
                        .as_i64()
                        .unwrap_or(1)
                        .clamp(1, i32::MAX as i64) as i32,
                    biomes,
                    conditions: definitions
                        .get(id)
                        .and_then(|definition| definition["conditions"].as_array())
                        .into_iter()
                        .flatten()
                        .map(parse_condition)
                        .collect(),
                })
            })
            .collect::<Vec<_>>();
        if members.is_empty() {
            continue;
        }
        sets.push(AuditSet {
            id: set["id"].as_str().unwrap_or("unknown:set").to_owned(),
            salt: salt.unwrap_or(0) as i32,
            spacing: spacing.unwrap_or(0) as i32,
            separation: separation.unwrap_or(0) as i32,
            triangular: p["spread_type"] == "triangular",
            climate: audit_climate_rule(&p["climate"]),
            placement_supported,
            unsupported_reason: if !placement_supported {
                Some(format!("Unsupported placement type `{placement_type}`"))
            } else if salt.is_none() || spacing.is_none() || separation.is_none() {
                Some("Missing source placement fields".into())
            } else if p["frequency"].as_f64().is_some_and(|v| v != 1.0)
                || p["frequency_reduction_method"]
                    .as_str()
                    .is_some_and(|v| v != "default")
            {
                Some("Frequency reduction is not modeled by this audit".into())
            } else {
                None
            },
            exclusion: p["exclusion_zone"].as_object().map(|v| {
                (
                    v.get("other_set")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    v.get("chunk_count").and_then(Value::as_i64).unwrap_or(0) as i32,
                )
            }),
            members,
        });
    }
    sets.sort_by(|a, b| a.id.cmp(&b.id));
    (sets, universe)
}

fn audit_climate_rule(value: &Value) -> Option<ClimateRule> {
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

fn scan_set(
    world: &mut World,
    set: &AuditSet,
    universe: &HashSet<String>,
    all_sets: &[AuditSet],
) -> Vec<StructureResult> {
    let mut results = Vec::new();
    let mut pending = HashSet::new();
    let mut checked = HashMap::<String, u64>::new();
    let mut rejected = HashMap::<String, u64>::new();
    for member in &set.members {
        if member.biomes.is_disjoint(universe) {
            results.push(StructureResult {
                dimension: world.dimension.id().into(), set_id: set.id.clone(), structure_id: member.id.clone(),
                status: Status::SourceBiomeIncompatible, x: None, z: None, distance: None,
                candidates_checked: 0, biome_rejections: 0,
                detail: "The installed structure biome selector has no intersection with the registered biome universe for this dimension.".into(),
            });
        } else if let Some(reason) = &set.unsupported_reason {
            results.push(StructureResult {
                dimension: world.dimension.id().into(),
                set_id: set.id.clone(),
                structure_id: member.id.clone(),
                status: Status::UnsupportedPlacement,
                x: None,
                z: None,
                distance: None,
                candidates_checked: 0,
                biome_rejections: 0,
                detail: reason.clone(),
            });
        } else {
            pending.insert(member.id.clone());
        }
    }
    if pending.is_empty() {
        return results;
    }
    if set.spacing <= 0 || set.separation < 0 || set.separation >= set.spacing {
        for id in pending {
            results.push(StructureResult {
                dimension: world.dimension.id().into(),
                set_id: set.id.clone(),
                structure_id: id,
                status: Status::UnsupportedPlacement,
                x: None,
                z: None,
                distance: None,
                candidates_checked: 0,
                biome_rejections: 0,
                detail: "Invalid random-spread spacing/separation in source data.".into(),
            });
        }
        return results;
    }

    // Placement output can shift by at most one spacing cell beyond the origin-centered chunk
    // radius. The actual block-radius predicate is applied to every placement point.
    let cell_radius = RADIUS.div_euclid(16 * set.spacing) + 2;
    'rings: for ring in 0..=cell_radius {
        if pending.is_empty() {
            break;
        }
        if ring == 0 {
            audit_cell(
                world,
                set,
                all_sets,
                0,
                0,
                &mut pending,
                &mut checked,
                &mut rejected,
                &mut results,
            );
            continue;
        }
        for gx in -ring..=ring {
            audit_cell(
                world,
                set,
                all_sets,
                gx,
                -ring,
                &mut pending,
                &mut checked,
                &mut rejected,
                &mut results,
            );
            if pending.is_empty() {
                break 'rings;
            }
            audit_cell(
                world,
                set,
                all_sets,
                gx,
                ring,
                &mut pending,
                &mut checked,
                &mut rejected,
                &mut results,
            );
            if pending.is_empty() {
                break 'rings;
            }
        }
        for gz in (-ring + 1)..ring {
            audit_cell(
                world,
                set,
                all_sets,
                -ring,
                gz,
                &mut pending,
                &mut checked,
                &mut rejected,
                &mut results,
            );
            if pending.is_empty() {
                break 'rings;
            }
            audit_cell(
                world,
                set,
                all_sets,
                ring,
                gz,
                &mut pending,
                &mut checked,
                &mut rejected,
                &mut results,
            );
            if pending.is_empty() {
                break 'rings;
            }
        }
    }
    for id in pending {
        results.push(StructureResult {
            dimension: world.dimension.id().into(), set_id: set.id.clone(), structure_id: id.clone(),
            status: Status::NoValidCandidateWithinRadius, x: None, z: None, distance: None,
            candidates_checked: checked.get(&id).copied().unwrap_or_default(),
            biome_rejections: rejected.get(&id).copied().unwrap_or_default(),
            detail: "The complete 100,000-block circle contained no random-spread candidate in this member's allowed biome selector. This is not proof that a saved structure does not exist.".into(),
        });
    }
    results
}

#[allow(clippy::too_many_arguments)]
fn audit_cell(
    world: &mut World,
    set: &AuditSet,
    all_sets: &[AuditSet],
    gx: i32,
    gz: i32,
    pending: &mut HashSet<String>,
    checked: &mut HashMap<String, u64>,
    rejected: &mut HashMap<String, u64>,
    results: &mut Vec<StructureResult>,
) {
    if pending.is_empty() {
        return;
    }
    let (chunk_x, chunk_z) = random_spread(world.seed, set, gx, gz);
    let x = chunk_x.saturating_mul(16).saturating_add(8);
    let z = chunk_z.saturating_mul(16).saturating_add(8);
    if (x as i64).pow(2) as u128 + (z as i64).pow(2) as u128 > (RADIUS as u128).pow(2) {
        return;
    }
    // Match each production scanner's biome probe: TFC's overworld random-spread sampler
    // checks the chunk origin, while Beneath and planetary samplers check the chunk center.
    let (biome_x, biome_z, biome_y) = match world.dimension {
        Dimension::Overworld => (chunk_x * 16, chunk_z * 16, world.slice_y),
        Dimension::Beneath => (x, z, 93),
        Dimension::Moon | Dimension::Mars => (
            x,
            z,
            world
                .height(x, z)
                .unwrap_or(world.dimension.default_y() as f64) as i32,
        ),
    };
    let biome = world.biome_at(biome_x, biome_y, biome_z);
    let climate_allowed = set.climate.as_ref().is_none_or(|rule| {
        let climate = world.climate(chunk_x * 16, chunk_z * 16);
        let forest = world.region.forest_type(chunk_x, chunk_z);
        !rule
            .min_temperature
            .is_some_and(|v| climate.temperature < v)
            && !rule
                .max_temperature
                .is_some_and(|v| climate.temperature > v)
            && !rule.min_rainfall.is_some_and(|v| climate.rainfall < v)
            && !rule.max_rainfall.is_some_and(|v| climate.rainfall > v)
            && forest >= rule.min_forest
            && forest <= rule.max_forest
    });
    let excluded = set.exclusion.as_ref().is_some_and(|(other_id, radius)| {
        all_sets
            .iter()
            .find(|other| &other.id == other_id)
            .is_some_and(|other| {
                excluded_by_placement(world.seed, other, chunk_x, chunk_z, *radius)
            })
    });
    // Keep every weighted child in the source draw, including children with an empty biome
    // selector for this dimension. Production draws first and then rejects an incompatible
    // selected child; removing it here would change the placement RNG result.
    let weighted_members = set
        .members
        .iter()
        .map(|m| CandidateStructure {
            id: m.id.clone(),
            weight: m.weight,
            biomes: m.biomes.clone(),
            conditions: m.conditions.clone(),
        })
        .collect::<Vec<_>>();
    let ordered = weighted_order(
        world.seed,
        chunk_x,
        chunk_z,
        weighted_members.iter().collect(),
    );
    for member in &set.members {
        if !pending.contains(&member.id) {
            continue;
        }
        *checked.entry(member.id.clone()).or_default() += 1;
        if !member.biomes.contains(&biome) {
            *rejected.entry(member.id.clone()).or_default() += 1;
        }
    }
    let mut biome_at = |bx, bz| world.biome(bx, bz);
    let mut surface_y = |sx, sz| world.height(sx, sz).map(|height| height as i32);
    let Some(chosen) = ordered.into_iter().find(|member| {
        member.biomes.contains(&biome)
            && climate_allowed
            && !excluded
            && conditions_allow(
                &member.conditions,
                &mut biome_at,
                &mut surface_y,
                biome_x,
                biome_z,
            )
    }) else {
        return;
    };
    if !pending.remove(&chosen.id) {
        return;
    }
    results.push(StructureResult {
        dimension: world.dimension.id().into(), set_id: set.id.clone(), structure_id: chosen.id.clone(),
        status: Status::CandidateFound, x: Some(x), z: Some(z), distance: Some((x as f64).hypot(z as f64)),
        candidates_checked: checked.get(&chosen.id).copied().unwrap_or_default(),
        biome_rejections: rejected.get(&chosen.id).copied().unwrap_or_default(),
        detail: "First seed-derived source placement cell within radius whose weighted child selected this structure and whose registered biome passed the installed selector.".into(),
    });
}

fn excluded_by_placement(seed: i64, other: &AuditSet, cx: i32, cz: i32, radius: i32) -> bool {
    if !other.placement_supported
        || other.spacing <= 0
        || other.separation < 0
        || other.separation >= other.spacing
    {
        return false;
    }
    let min_x = (cx - radius - 2).div_euclid(other.spacing);
    let max_x = (cx + radius + 2).div_euclid(other.spacing);
    let min_z = (cz - radius - 2).div_euclid(other.spacing);
    let max_z = (cz + radius + 2).div_euclid(other.spacing);
    for gz in min_z..=max_z {
        for gx in min_x..=max_x {
            let (ox, oz) = random_spread(seed, other, gx, gz);
            if (ox - cx).abs() <= radius && (oz - cz).abs() <= radius {
                return true;
            }
        }
    }
    false
}

fn random_spread(seed: i64, set: &AuditSet, grid_x: i32, grid_z: i32) -> (i32, i32) {
    let placement_seed = (grid_x as i64)
        .wrapping_mul(341873128712)
        .wrapping_add((grid_z as i64).wrapping_mul(132897987541))
        .wrapping_add(seed)
        .wrapping_add(set.salt as i64);
    let mut rng = JavaRandom::new(placement_seed);
    let range = set.spacing - set.separation;
    let offset = |rng: &mut JavaRandom| {
        if set.triangular {
            (rng.next_int_bound(range) + rng.next_int_bound(range)) / 2
        } else {
            rng.next_int_bound(range)
        }
    };
    (
        grid_x
            .wrapping_mul(set.spacing)
            .wrapping_add(offset(&mut rng)),
        grid_z
            .wrapping_mul(set.spacing)
            .wrapping_add(offset(&mut rng)),
    )
}

#[cfg(test)]
mod diagnostics_tests {
    use super::*;

    #[test]
    fn checked_seed_zero_report_witnesses_exist_in_production_scanners() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/data/structure-audit-seed0-100km.json");
        let json = std::fs::read_to_string(path).expect("generated seed-zero structure report");
        let report: Report = serde_json::from_str(&json).expect("valid structure report");
        let mismatches = verify_report_witnesses(&report);
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }
}
