//! Effective structure-template loot evidence, independent of a structure's registry name.
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LootStatus {
    Loot,
    NoLoot,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
pub struct Evidence {
    pub status: LootStatus,
    pub has_containers: bool,
    #[serde(default)]
    pub container_blocks: Vec<String>,
    #[serde(default)]
    pub loot_tables: Vec<String>,
    #[serde(default)]
    pub template_count: usize,
    #[serde(default)]
    pub unknowns: Vec<String>,
    #[serde(skip)]
    search_terms: String,
}

#[derive(Deserialize)]
struct Manifest {
    structures: HashMap<String, Evidence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuinGroup {
    Lootless,
    Building,
}

/// Keep registry identities intact while naming useful ruins as buildings in the UI.
pub fn display_name(id: &str) -> Option<&'static str> {
    match id.split('@').next().unwrap_or(id) {
        "tfc_ruins:ruin_small" => Some("Small ruined building"),
        "tfc_ruins:ruin_beach" => Some("Coastal ruined building"),
        "tfc_ruins:ruin_rich" => Some("Rich ruined building"),
        "tfg:illagers/norse_village_ruined" => Some("Dead Norse village"),
        _ => None,
    }
}

fn manifest() -> &'static Manifest {
    static DATA: OnceLock<Manifest> = OnceLock::new();
    DATA.get_or_init(|| {
        let mut data: Manifest = serde_json::from_str(include_str!(
            "../assets/data/tfg/structure-loot-runtime.json"
        ))
        .expect("installed structure loot evidence");
        for row in data.structures.values_mut() {
            if row.status == LootStatus::Loot || row.has_containers {
                row.search_terms = format!(
                    "loot container {} {}",
                    row.container_blocks.join(" "),
                    row.loot_tables.join(" ")
                );
            }
        }
        data
    })
}

pub fn evidence(id: &str) -> Option<&'static Evidence> {
    manifest()
        .structures
        .get(id.split('@').next().unwrap_or(id))
}

pub fn has_loot(id: &str) -> bool {
    evidence(id).is_some_and(|row| row.status == LootStatus::Loot || row.has_containers)
}

/// Unknown or unresolved templates cannot be called useless, loot-free ruins.
pub fn ruin_group(id: &str) -> Option<RuinGroup> {
    crate::structure_icons::ruin_topic(id)?;
    Some(
        if evidence(id).is_some_and(|row| {
            row.status == LootStatus::NoLoot && !row.has_containers && row.unknowns.is_empty()
        }) {
            RuinGroup::Lootless
        } else {
            RuinGroup::Building
        },
    )
}

pub fn search_terms(id: &str) -> &'static str {
    evidence(id).map_or("", |row| row.search_terms.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mudbrick_site_with_saved_populated_vessels_is_a_visible_building() {
        let saved: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/loot-sites/world0-1231--204.json"
        ))
        .unwrap();
        let id = saved["structure"]["registry_id"].as_str().unwrap();
        assert_eq!(
            saved["provenance"]["chunk"],
            serde_json::json!([1231, -204])
        );
        let containers = saved["container_block_entities"].as_array().unwrap();
        assert_eq!(containers.len(), 2);
        assert!(containers.iter().all(|container| {
            !container["inventory_items_summary"]
                .as_array()
                .unwrap()
                .is_empty()
        }));
        let row = evidence(id).expect("all registered structure IDs audited");
        assert!(has_loot(id));
        assert!(row.has_containers);
        assert!(
            row.container_blocks
                .iter()
                .any(|block| block.contains("large_vessel"))
        );
        assert_eq!(ruin_group(id), Some(RuinGroup::Building));
        assert_eq!(display_name(id), Some("Small ruined building"));
        let prefs = crate::display::DisplayPrefs::default();
        assert!(!prefs.hidden_structure_ids.iter().any(|hidden| hidden == id));
    }

    #[test]
    fn registered_types_are_covered_and_unknown_ruins_are_not_declared_lootless() {
        for definition in crate::structure_support::source()["structures"]
            .as_array()
            .unwrap()
        {
            let id = definition["id"].as_str().unwrap();
            assert!(evidence(id).is_some(), "missing loot audit: {id}");
        }
        assert_eq!(
            ruin_group("custom:abandoned_house"),
            Some(RuinGroup::Building)
        );
        let prefs = crate::display::DisplayPrefs::default();
        for (id, _) in crate::structure_support::catalog(crate::dimension::Dimension::Overworld) {
            if evidence(id).is_some_and(|row| row.status == LootStatus::Loot) {
                assert!(
                    !prefs.hidden_structure_ids.iter().any(|hidden| hidden == id),
                    "loot-bearing Overworld structure must not inherit Ruins hiding: {id}"
                );
            }
        }
        for (id, row) in &manifest().structures {
            if ruin_group(id) == Some(RuinGroup::Lootless) {
                assert_eq!(row.status, LootStatus::NoLoot, "{id}");
                assert!(!row.has_containers && row.unknowns.is_empty(), "{id}");
            }
        }
    }

    #[test]
    fn compact_runtime_evidence_agrees_with_the_complete_template_audit() {
        let full: serde_json::Value =
            serde_json::from_str(include_str!("../assets/data/tfg/structure-loot.json")).unwrap();
        let rows = full["structures"].as_object().unwrap();
        assert_eq!(rows.len(), manifest().structures.len());
        for (id, row) in rows {
            let compact = evidence(id).unwrap();
            assert_eq!(
                compact.status,
                serde_json::from_value::<LootStatus>(row["status"].clone()).unwrap(),
                "{id}"
            );
            assert_eq!(
                compact.has_containers,
                row["has_containers"].as_bool().unwrap(),
                "{id}"
            );
            assert_eq!(
                compact.container_blocks,
                serde_json::from_value::<Vec<String>>(row["container_blocks"].clone()).unwrap(),
                "{id}"
            );
            assert_eq!(
                compact.loot_tables,
                serde_json::from_value::<Vec<String>>(row["loot_tables"].clone()).unwrap(),
                "{id}"
            );
            assert_eq!(
                compact.unknowns,
                serde_json::from_value::<Vec<String>>(row["unknowns"].clone()).unwrap(),
                "{id}"
            );
            assert_eq!(
                compact.template_count,
                row["templates"].as_array().unwrap().len(),
                "{id}"
            );
        }
    }
}
