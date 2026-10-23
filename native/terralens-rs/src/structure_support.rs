//! Registered structure controls and precise seed-preview biome support.
//!
//! Catalog discovery is deliberately broader than the pinned sampler's biome intersections:
//! a negative sample or an incomplete static profile cannot remove a registered control.
//! Placement checks still compare literal registry IDs, without guessing biome aliases.
//! Saved starts remain independent evidence and never depend on preview support.
use crate::dimension::Dimension;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::OnceLock,
};

const SOURCE: &str = include_str!("../assets/data/tfg/structure-applicability.json");

struct Profile {
    biomes: HashMap<String, HashSet<String>>,
    sets: HashMap<String, Vec<String>>,
    catalog: Vec<(String, String)>,
    registered: HashSet<String>,
}

/// Registry data and source provenance, also consumed by the opt-in 100 km audit.
pub fn source() -> &'static Value {
    static SOURCE_DATA: OnceLock<Value> = OnceLock::new();
    SOURCE_DATA
        .get_or_init(|| serde_json::from_str(SOURCE).expect("structure applicability profile"))
}

fn explicitly_disabled(child: &Value) -> bool {
    child["biome_selector"]
        .as_array()
        .is_some_and(Vec::is_empty)
}

/// Identify the dimension of a registry biome, including native biomes outside our sampler.
/// This only sorts controls into dimensions; it never substitutes IDs in placement gates.
fn biome_dimension(id: &str) -> Option<Dimension> {
    match id {
        "minecraft:nether_wastes"
        | "minecraft:soul_sand_valley"
        | "minecraft:crimson_forest"
        | "minecraft:warped_forest"
        | "minecraft:basalt_deltas" => Some(Dimension::Beneath),
        "minecraft:the_end"
        | "minecraft:end_highlands"
        | "minecraft:end_midlands"
        | "minecraft:small_end_islands"
        | "minecraft:end_barrens" => None,
        _ if id.starts_with("tfg:nether/") => Some(Dimension::Beneath),
        _ if id.starts_with("tfg:moon/") || id.starts_with("ad_astra:lunar_") => {
            Some(Dimension::Moon)
        }
        _ if id.starts_with("tfg:mars/") || id.starts_with("ad_astra:martian_") => {
            Some(Dimension::Mars)
        }
        _ if id.starts_with("minecraft:")
            || id.starts_with("tfg:earth/")
            || id.starts_with("tfc:") =>
        {
            Some(Dimension::Overworld)
        }
        _ => None,
    }
}

fn plausible_dimension(child: &Value, dimension: Dimension) -> bool {
    child["dimension_biomes"][dimension.id()]
        .as_array()
        .is_some_and(|biomes| !biomes.is_empty())
        || child["resolved_selector_biomes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|biome| biome_dimension(biome) == Some(dimension))
}

fn profiles() -> &'static HashMap<Dimension, Profile> {
    static PROFILES: OnceLock<HashMap<Dimension, Profile>> = OnceLock::new();
    PROFILES.get_or_init(|| {
        Dimension::ALL
            .into_iter()
            .map(|dimension| {
                let mut biomes = HashMap::<String, HashSet<String>>::new();
                let mut sets = HashMap::<String, Vec<String>>::new();
                let mut registered = HashSet::<String>::new();
                for set in source()["sets"].as_array().expect("structure sets") {
                    let set_id = set["id"].as_str().expect("structure set id");
                    for child in set["children"].as_array().expect("structure children") {
                        let id = child["id"].as_str().expect("structure id");
                        if explicitly_disabled(child) {
                            continue;
                        }
                        if plausible_dimension(child, dimension) {
                            registered.insert(id.to_owned());
                            sets.entry(set_id.to_owned())
                                .or_default()
                                .push(id.to_owned());
                        }
                        let allowed: HashSet<String> = child["dimension_biomes"][dimension.id()]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect();
                        if allowed.is_empty() {
                            continue;
                        }
                        biomes.entry(id.to_owned()).or_default().extend(allowed);
                    }
                }
                let catalog = registered
                    .iter()
                    .map(|id| {
                        (
                            id.clone(),
                            id.rsplit(':').next().unwrap_or(id).replace(['_', '/'], " "),
                        )
                    })
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .collect();
                (
                    dimension,
                    Profile {
                        biomes,
                        sets,
                        catalog,
                        registered,
                    },
                )
            })
            .collect()
    })
}

pub fn catalog(dimension: Dimension) -> &'static Vec<(String, String)> {
    &profiles()[&dimension].catalog
}

/// Registered, dimension-appropriate controls and set aliases, independent of sample hits.
/// This is catalog availability, not a promise that the seed preview can place the type.
pub fn enabled(dimension: Dimension, id: &str) -> bool {
    let id = id.split('@').next().unwrap_or(id);
    let profile = &profiles()[&dimension];
    profile.registered.contains(id)
        || profile
            .sets
            .get(id)
            .is_some_and(|children| !children.is_empty())
}

/// Literal current-profile biome intersection for the seed placement scanner.
/// A missing intersection limits this scanner; it does not blacklist the catalog entry.
pub fn biome_compatible(dimension: Dimension, id: &str) -> bool {
    allowed_biomes(dimension, id).is_some_and(|biomes| !biomes.is_empty())
}

pub fn allowed_biomes(dimension: Dimension, id: &str) -> Option<&'static HashSet<String>> {
    profiles()[&dimension].biomes.get(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_biome_requirements_are_not_aliased_to_tfc_worldgen() {
        for id in [
            "minecraft:stronghold",
            "minecraft:strongholds",
            "minecraft:ancient_city",
            "minecraft:buried_treasure",
            "minecraft:village_plains",
            "wan_ancient_beasts:ruined_castle",
        ] {
            assert!(!biome_compatible(Dimension::Overworld, id), "{id}");
        }
        assert!(enabled(
            Dimension::Overworld,
            "tfc_ruined_world:towerhouse_1"
        ));
        assert!(enabled(
            Dimension::Overworld,
            "tfg:illagers/norse_village_living"
        ));
    }

    #[test]
    fn explicit_empty_and_wrong_dimension_profiles_stay_disabled() {
        assert!(enabled(Dimension::Beneath, "minecraft:fortress"));
        assert!(!biome_compatible(Dimension::Beneath, "minecraft:fortress"));
        assert!(!enabled(Dimension::Beneath, "tfg:beneath/outpost"));
        assert!(enabled(Dimension::Beneath, "minecraft:bastion_remnant"));
        assert!(enabled(Dimension::Beneath, "minecraft:nether_fossil"));
        assert!(enabled(Dimension::Moon, "tfg:moon/moonbase@-31,17"));
        assert!(enabled(Dimension::Moon, "ad_astra:moon_dungeon"));
        assert!(enabled(Dimension::Mars, "ad_astra:mars_temple"));
        assert!(!biome_compatible(Dimension::Moon, "ad_astra:moon_dungeon"));
        assert!(!biome_compatible(Dimension::Mars, "ad_astra:mars_temple"));
        assert!(!enabled(Dimension::Overworld, "tfg:moon/moonbase"));
    }

    #[test]
    fn pinned_inventory_and_audit_cover_every_type_in_each_dimension() {
        let source = source();
        assert_eq!(source["sets"].as_array().unwrap().len(), 66);
        let all_ids = source["structures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect::<HashSet<_>>();
        assert_eq!(all_ids.len(), 131);
        assert!(
            source["unresolved_references"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let overworld = source["supported_dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == Dimension::Overworld.id())
            .unwrap();
        let biomes = overworld["biomes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry.as_str().unwrap())
            .collect::<HashSet<_>>();
        assert_eq!(biomes, crate::biome::ids::BIOME_IDS.into_iter().collect());

        let report: Value = serde_json::from_str(include_str!(
            "../../../docs/data/structure-audit-seed0-100km.json"
        ))
        .unwrap();
        assert_eq!(report["seed"], 0);
        assert_eq!(report["center"], serde_json::json!([0, 0]));
        assert_eq!(report["radius_blocks"], 100_000);
        assert_eq!(report["dimensions"].as_array().unwrap().len(), 4);
        for (dimension, count) in [
            (Dimension::Overworld, 30),
            (Dimension::Beneath, 3),
            (Dimension::Moon, 4),
            (Dimension::Mars, 0),
        ] {
            let audit = report["dimensions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["dimension"] == dimension.id())
                .unwrap();
            let results = audit["structures"].as_array().unwrap();
            assert_eq!(results.len(), all_ids.len(), "{dimension:?}");
            let audited_ids = results
                .iter()
                .map(|entry| entry["structure_id"].as_str().unwrap())
                .collect::<HashSet<_>>();
            assert_eq!(audited_ids, all_ids, "{dimension:?}");
            assert_eq!(profiles()[&dimension].biomes.len(), count);
            for result in results {
                let id = result["structure_id"].as_str().unwrap();
                let found = result["status"] == "candidate_found";
                assert_eq!(
                    biome_compatible(dimension, id),
                    found,
                    "{dimension:?}: {id}"
                );
                if found {
                    assert!(enabled(dimension, id), "{dimension:?}: {id}");
                }
                if found {
                    let x = result["x"].as_f64().unwrap();
                    let z = result["z"].as_f64().unwrap();
                    assert!(x.hypot(z) <= 100_000., "{dimension:?}: {id}");
                    assert!(result["candidates_checked"].as_u64().unwrap() > 0);
                } else {
                    assert_eq!(
                        result["status"], "no_matching_sampler_biome",
                        "{dimension:?}: {id}"
                    );
                    assert_eq!(result["candidates_checked"], 0);
                }
            }
        }
    }

    #[test]
    fn independently_recorded_saved_starts_are_kept_in_the_source_catalog() {
        let report: Value = serde_json::from_str(include_str!(
            "../../../docs/data/structure-audit-seed0-100km.json"
        ))
        .unwrap();
        let mut recorded_types = 0;
        for dimension in Dimension::ALL {
            let audit = report["dimensions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["dimension"] == dimension.id())
                .unwrap();
            if audit["saved_start_evidence"].is_null() {
                continue;
            }
            assert_eq!(audit["saved_start_evidence"]["decode_errors"], 0);
            for start in audit["saved_start_evidence"]["nearest_starts"]
                .as_array()
                .unwrap()
            {
                let id = start["structure_id"].as_str().unwrap();
                assert!(enabled(dimension, id), "recorded {dimension:?}: {id}");
                recorded_types += 1;
            }
        }
        // Independent saved Anvil starts, not 37 claims of full structure generation.
        assert_eq!(recorded_types, 29);
    }

    #[test]
    fn negative_sample_or_static_biome_mismatch_cannot_blacklist_registered_settlements() {
        for id in [
            "minecraft:village_plains",
            "minecraft:village_desert",
            "minecraft:village_savanna",
            "minecraft:village_snowy",
            "minecraft:village_taiga",
            "wan_ancient_beasts:abandoned_village",
        ] {
            assert!(enabled(Dimension::Overworld, id), "{id}");
            assert!(
                catalog(Dimension::Overworld)
                    .iter()
                    .any(|(candidate, _)| candidate == id),
                "{id}"
            );
            assert!(
                !biome_compatible(Dimension::Overworld, id),
                "no fabricated biome alias: {id}"
            );
            assert!(!enabled(Dimension::Moon, id), "wrong dimension: {id}");
        }
        for id in [
            "tfg:illagers/arabic_village_living",
            "tfg:illagers/malay_village_living",
            "tfg:illagers/norse_village_living",
            "tfg:illagers/norse_village_ruined",
            "tfg:illagers/yurts_village_living",
            "tfg:temperate/plains_temperate_house_0",
            "tfc_ruined_world:towerhouse_1",
            "tfc_ruined_world:towerhouse_2",
        ] {
            assert!(enabled(Dimension::Overworld, id), "{id}");
            assert!(biome_compatible(Dimension::Overworld, id), "{id}");
        }
        assert!(
            !enabled(Dimension::Beneath, "tfg:beneath/outpost"),
            "explicit pack disable"
        );
    }
}
