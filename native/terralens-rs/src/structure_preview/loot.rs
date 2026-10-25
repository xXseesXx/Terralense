//! Loot table summaries and Lootr per-player saved inventory lookup.
//!
//! Table summaries deliberately enumerate possibilities; they never roll loot.
use super::{Container, PlayerInventory, PossibleDrop, StructurePreview};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::OnceLock,
};

const TABLE_DATA: &str = include_str!("../../assets/data/tfg/loot-tables.json");
const MAX_LOOTR_FILE: u64 = 64 * 1024 * 1024;
static TABLES: OnceLock<Option<TablePack>> = OnceLock::new();

#[derive(Deserialize)]
struct TablePack {
    tables: BTreeMap<String, TableRecord>,
    #[serde(default)]
    block_loot: BTreeMap<String, VaseSummary>,
}
#[derive(Deserialize)]
struct VaseSummary {
    #[serde(default)]
    possible_drops: Vec<PossibleDrop>,
    #[serde(default)]
    notes: Vec<String>,
}
#[derive(Deserialize)]
struct TableRecord {
    #[serde(default)]
    table: Option<serde_json::Value>,
    #[serde(default)]
    ambiguous_sources: Vec<String>,
}

/// Enrich containers from the static installed-pack table catalogue and, when available,
/// read-only Lootr SavedData. This runs on the structure engine thread, never the UI thread.
pub fn enrich(preview: &mut StructurePreview, path: Option<&Path>, cancel: &impl Fn() -> bool) {
    let Some(tables) = TABLES
        .get_or_init(|| serde_json::from_str(TABLE_DATA).ok())
        .as_ref()
    else {
        return;
    };
    let lootr_config = read_lootr_config(path);
    let randomizes_loot = apply_pack_lootr_policy(preview, lootr_config.as_deref());
    for container in &mut preview.containers {
        if cancel() {
            return;
        }
        enrich_block_loot(container, tables);
        enrich_table(container, tables);
        if container.lootr {
            if let Some(path) = path {
                load_lootr_inventories(container, path, cancel);
            }
            if !container
                .notes
                .iter()
                .any(|x| x.contains("Lootr inventory is per-player"))
            {
                let note = match randomizes_loot {
                    Some(true) => {
                        "Lootr inventory is per-player. Unopened contents are unknown until saved after first open; this profile randomizes loot seeds, so world seed and chest position cannot determine exact items."
                    }
                    Some(false) => {
                        "Lootr inventory is per-player. Unopened contents remain unknown until saved after first open; TerraLens does not roll exact Minecraft loot results."
                    }
                    None => {
                        "Lootr inventory is per-player. Unopened contents remain unknown until saved after first open; the Lootr seed setting could not be confirmed."
                    }
                };
                container.notes.push(note.into());
            }
        }
    }
}

fn enrich_block_loot(container: &mut Container, pack: &TablePack) {
    if !container
        .block
        .starts_with("tfg:decorative_vase/generated/")
    {
        return;
    }
    let Some(summary) = pack.block_loot.get("generated_vase") else {
        container.notes.push(
            "The generated-vase loot modifier was not present in the extracted pack data.".into(),
        );
        return;
    };
    container.possible_drops = summary.possible_drops.clone();
    for note in &summary.notes {
        if !container.notes.contains(note) {
            container.notes.push(note.clone());
        }
    }
}

fn enrich_table(container: &mut Container, pack: &TablePack) {
    let Some(id) = container.loot_table.as_deref() else {
        return;
    };
    let mut visited = BTreeSet::new();
    let mut drops = BTreeMap::<String, (u32, u32)>::new();
    let mut caveats = BTreeSet::new();
    collect_table(id, pack, &mut visited, &mut drops, &mut caveats, 0);
    if !drops.is_empty() {
        container.possible_drops = drops
            .into_iter()
            .map(|(id, (min, max))| PossibleDrop { id, min, max })
            .collect();
    }
    if !container.contents_known && !container.lootr && container.loot_seed.is_none() {
        caveats.insert("The table has not been rolled; these are possible items and stack sizes, not an exact inventory.".into());
    }
    if container.loot_seed.is_some() && !container.lootr && !container.contents_known {
        caveats.insert("A saved table seed exists, but TerraLens does not emulate Minecraft's complete loot context and roll engine; exact contents are not claimed.".into());
    }
    caveats.insert("Conditional entries, tags, and context-sensitive functions can change the actual result; this list is a conservative preview of directly resolved item entries.".into());
    for note in caveats {
        if !container.notes.contains(&note) {
            container.notes.push(note);
        }
    }
}

fn collect_table(
    id: &str,
    pack: &TablePack,
    visited: &mut BTreeSet<String>,
    drops: &mut BTreeMap<String, (u32, u32)>,
    caveats: &mut BTreeSet<String>,
    depth: usize,
) {
    if depth >= 32 || !visited.insert(id.to_string()) {
        return;
    }
    let Some(record) = pack.tables.get(id) else {
        caveats.insert(format!(
            "Loot table {id} is not available in the extracted installed-pack resources."
        ));
        return;
    };
    let Some(table) = record.table.as_ref() else {
        if record.ambiguous_sources.is_empty() {
            caveats.insert(format!("Loot table {id} has an invalid source resource."));
        } else {
            caveats.insert(format!(
                "Loot table {id} has conflicting sources: {}.",
                record.ambiguous_sources.join(", ")
            ));
        }
        return;
    };
    if let Some(pools) = table.get("pools").and_then(serde_json::Value::as_array) {
        for pool in pools {
            if let Some(entries) = pool.get("entries").and_then(serde_json::Value::as_array) {
                for entry in entries {
                    collect_entry(entry, pack, visited, drops, caveats, depth + 1);
                }
            }
        }
    }
    visited.remove(id);
}

fn collect_entry(
    entry: &serde_json::Value,
    pack: &TablePack,
    visited: &mut BTreeSet<String>,
    drops: &mut BTreeMap<String, (u32, u32)>,
    caveats: &mut BTreeSet<String>,
    depth: usize,
) {
    if depth >= 32 {
        return;
    }
    let kind = entry
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    match kind {
        "minecraft:item" | "item" => {
            if let Some(id) = entry.get("name").and_then(serde_json::Value::as_str) {
                let (min, max, known) = stack_range(entry);
                let range = drops.entry(id.to_string()).or_insert((u32::MAX, 0));
                range.0 = range.0.min(min);
                range.1 = range.1.max(max);
                if !known {
                    caveats.insert(format!(
                        "Stack size for {id} uses an unsupported or contextual loot function."
                    ));
                }
                if has_contextual_function(entry) {
                    caveats.insert(format!(
                        "{id} has a data, condition, or context-dependent item function."
                    ));
                }
            }
        }
        "minecraft:loot_table" | "loot_table" => {
            if let Some(ref_id) = entry.get("name").and_then(serde_json::Value::as_str) {
                collect_table(ref_id, pack, visited, drops, caveats, depth + 1);
            }
        }
        "minecraft:alternatives"
        | "minecraft:group"
        | "minecraft:sequence"
        | "alternatives"
        | "group"
        | "sequence" => {
            if let Some(children) = entry.get("children").and_then(serde_json::Value::as_array) {
                for child in children {
                    collect_entry(child, pack, visited, drops, caveats, depth + 1);
                }
            }
        }
        "minecraft:tag" | "tag" => {
            let id = entry
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown tag");
            caveats.insert(format!("Item tag {id} is unresolved in this preview."));
        }
        "minecraft:empty" | "empty" => {}
        _ => {
            if !kind.is_empty() {
                caveats.insert(format!("Loot entry type {kind} is not expanded."));
            }
        }
    }
}

fn stack_range(entry: &serde_json::Value) -> (u32, u32, bool) {
    let mut min = 1;
    let mut max = 1;
    let mut known = true;
    if let Some(functions) = entry.get("functions").and_then(serde_json::Value::as_array) {
        for function in functions {
            let kind = function
                .get("function")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if kind == "minecraft:set_count" || kind == "set_count" {
                let (a, b, ok) = number_range(function.get("count"));
                min = a;
                max = b;
                known &= ok;
            } else if kind == "minecraft:limit_count" || kind == "limit_count" {
                if let Some((a, b)) = function.get("limit").and_then(|v| range_pair(Some(v))) {
                    min = min.max(a);
                    max = max.min(b).max(min);
                }
            } else if !kind.is_empty()
                && kind != "minecraft:looting_enchant"
                && kind != "minecraft:explosion_decay"
            {
                known = false;
            }
        }
    }
    (min, max, known)
}

fn number_range(v: Option<&serde_json::Value>) -> (u32, u32, bool) {
    let Some(v) = v else {
        return (1, 1, false);
    };
    if let Some(n) = v.as_u64() {
        let n = n.min(u32::MAX as u64) as u32;
        return (n, n, true);
    }
    let ty = v
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if (ty.ends_with("uniform") || ty == "uniform")
        && let Some((a, b)) = range_pair(Some(v))
    {
        return (a, b, true);
    }
    if (ty.ends_with("constant") || ty == "constant")
        && let Some(n) = v.get("value").and_then(serde_json::Value::as_u64)
    {
        let n = n.min(u32::MAX as u64) as u32;
        return (n, n, true);
    }
    (1, 64, false)
}
fn range_pair(v: Option<&serde_json::Value>) -> Option<(u32, u32)> {
    let v = v?;
    let min = v.get("min")?.as_u64()?.min(u32::MAX as u64) as u32;
    let max = v.get("max")?.as_u64()?.min(u32::MAX as u64) as u32;
    Some((min.min(max), min.max(max)))
}
fn has_contextual_function(entry: &serde_json::Value) -> bool {
    entry
        .get("conditions")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|a| !a.is_empty())
        || entry
            .get("functions")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|a| {
                a.iter().any(|f| {
                    f.get("function")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|k| {
                            !matches!(
                                k,
                                "minecraft:set_count"
                                    | "set_count"
                                    | "minecraft:limit_count"
                                    | "limit_count"
                            )
                        })
                })
            })
}

fn load_lootr_inventories(
    container: &mut Container,
    world_path: &Path,
    cancel: &impl Fn() -> bool,
) {
    // The saved-world reader supplies the block entity UUID. Read only its exact Lootr record;
    // never scan unrelated records or guess identity from nearby containers.
    let Some(lootr_id) = container.lootr_id.as_deref() else {
        return;
    };
    if cancel() {
        return;
    }
    let base = world_path.join("data").join("lootr");
    let Some(file) = lootr_saved_path(&base, lootr_id) else {
        return;
    };
    if cancel() {
        return;
    }
    let Ok(meta) = fs::metadata(&file) else {
        return;
    };
    if meta.len() > MAX_LOOTR_FILE {
        container
            .notes
            .push("Lootr saved data exceeds the read limit.".into());
        return;
    }
    let Ok(root) = crate::prism::read_nbt_file(&file) else {
        return;
    };
    let data = root.get("data").unwrap_or(&root);
    let Some(pos) = data.get("position").and_then(NbtPos::from_nbt) else {
        container
            .notes
            .push("Lootr SavedData did not contain a readable position.".into());
        return;
    };
    if pos.x != container.pos.x || pos.y != container.pos.y || pos.z != container.pos.z {
        return;
    }
    let Some(expected_dimension) = container.dimension.as_deref() else {
        return;
    };
    let Some(saved_dimension) = data.get("dimension").and_then(crate::prism::Nbt::string) else {
        container.notes.push("Lootr SavedData has no dimension metadata; cross-dimension identity cannot be verified.".into());
        return;
    };
    if !dimension_matches(saved_dimension, expected_dimension) {
        return;
    }
    let Some(expected_id) = container.lootr_id.as_deref().and_then(normalized_uuid) else {
        return;
    };
    let Some(saved_id) = data.get("uuid").and_then(crate::prism::Nbt::uuid_string) else {
        container.notes.push("Lootr SavedData has no container UUID metadata; inventory identity cannot be verified.".into());
        return;
    };
    if saved_id != expected_id {
        return;
    }
    let Some(rows) = data.get("inventories").and_then(crate::prism::Nbt::list) else {
        return;
    };
    container.player_inventories = parse_player_inventories(rows);
    if !container.player_inventories.is_empty() {
        container.contents_known = true;
    } else {
        container
            .notes
            .push("Lootr SavedData exists, but no saved player inventory is present yet.".into());
    }
}

fn read_lootr_config(world_path: Option<&Path>) -> Option<String> {
    let game_dir = world_path
        .and_then(|p| p.parent().and_then(Path::parent))
        .map(Path::to_path_buf)
        .unwrap_or_else(|| crate::prism::default_instance().join("minecraft"));
    let config_path = game_dir.join("config/lootr-common.toml");
    fs::read_to_string(config_path).ok()
}

fn apply_pack_lootr_policy(preview: &mut StructurePreview, config: Option<&str>) -> Option<bool> {
    let Some(config) = config else {
        for c in &mut preview.containers {
            if c.loot_table.is_some()
                && (c.block.ends_with(":chest") || c.block.ends_with(":trapped_chest"))
            {
                c.notes.push(
                    "Whether Lootr converts this chest depends on the instance configuration."
                        .into(),
                );
            }
        }
        return None;
    };
    let value = |key: &str| {
        config.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').to_owned())
        })
    };
    let bool_value = |key: &str, default: bool| {
        value(key)
            .map(|v| v.parse::<bool>().unwrap_or(default))
            .unwrap_or(default)
    };
    let disabled = bool_value("disable", false);
    let randomise = value("randomise_seed").and_then(|v| v.parse::<bool>().ok());
    if preview.source == super::PreviewSource::Saved {
        return randomise;
    }
    let dim = preview
        .containers
        .first()
        .and_then(|c| c.dimension.as_deref())
        .unwrap_or("minecraft:overworld");
    let dimensions = value("dimension_whitelist").unwrap_or_default();
    let dim_blacklist = value("dimension_blacklist").unwrap_or_default();
    let mod_dim_whitelist = value("modid_dimension_whitelist").unwrap_or_default();
    let mod_dim_blacklist = value("modid_dimension_blacklist").unwrap_or_default();
    let mods_blacklist = value("loot_modid_blacklist").unwrap_or_default();
    let tables_blacklist = value("loot_table_blacklist").unwrap_or_default();
    let whitelisted = dimensions.contains('[') && !dimensions.contains("[]");
    let dim_mod = dim.split_once(':').map(|x| x.0).unwrap_or("minecraft");
    let mod_whitelisted = mod_dim_whitelist.contains('[') && !mod_dim_whitelist.contains("[]");
    let allowed_dimension = (!whitelisted || dimensions.contains(dim))
        && !dim_blacklist.contains(dim)
        && (!mod_whitelisted || mod_dim_whitelist.contains(&format!("\"{dim_mod}\"")))
        && !mod_dim_blacklist.contains(&format!("\"{dim_mod}\""));
    for container in &mut preview.containers {
        if container.loot_table.is_none() || !allowed_dimension || disabled {
            continue;
        }
        let block = container.block.as_str();
        let wooden_chest = block.ends_with(":chest") && !block.ends_with(":trapped_chest");
        let trapped = block.ends_with(":trapped_chest");
        let enabled = (wooden_chest && bool_value("convert_wooden_chests", true))
            || (trapped && bool_value("convert_trapped_chests", true))
            || value("additional_chests")
                .as_deref()
                .is_some_and(|list| list.contains(block))
            || value("additional_trapped_chests")
                .as_deref()
                .is_some_and(|list| list.contains(block));
        if !enabled {
            continue;
        }
        if container
            .loot_table
            .as_deref()
            .is_some_and(|t| tables_blacklist.contains(t))
        {
            continue;
        }
        if container
            .loot_table
            .as_deref()
            .and_then(|t| t.split_once(':').map(|x| x.0))
            .is_some_and(|ns| mods_blacklist.contains(&format!("\"{ns}\"")))
        {
            continue;
        }
        container.lootr = true;
        match randomise {
            Some(true) => container.notes.push("Lootr inventory is per-player and uses randomized seeds; each player's unopened contents differ and cannot be calculated from the world seed.".into()),
            Some(false) => container.notes.push("Lootr inventory is per-player; unopened contents remain unknown until saved after opening.".into()),
            None => container.notes.push("Lootr inventory is per-player; the random seed setting is unavailable, so unopened contents remain unknown.".into()),
        }
    }
    randomise
}

fn lootr_saved_path(base: &Path, id: &str) -> Option<std::path::PathBuf> {
    let id = id.trim().to_ascii_lowercase();
    let compact = id.replace('-', "");
    if compact.len() != 32 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let uuid = format!(
        "{}-{}-{}-{}-{}",
        &compact[..8],
        &compact[8..12],
        &compact[12..16],
        &compact[16..20],
        &compact[20..]
    );
    Some(
        base.join(&uuid[..1])
            .join(&uuid[..2])
            .join(format!("{uuid}.dat")),
    )
}

fn normalized_uuid(id: &str) -> Option<String> {
    let compact = id.trim().replace('-', "").to_ascii_lowercase();
    if compact.len() != 32 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!(
        "{}-{}-{}-{}-{}",
        &compact[..8],
        &compact[8..12],
        &compact[12..16],
        &compact[16..20],
        &compact[20..]
    ))
}

#[derive(PartialEq, Eq)]
struct NbtPos {
    x: i32,
    y: i32,
    z: i32,
}
impl NbtPos {
    fn from_nbt(n: &crate::prism::Nbt) -> Option<Self> {
        if let crate::prism::Nbt::Long(value) = n {
            let x = (value >> 38) as i32;
            let y = ((value & 0xfff) as i32) << 20 >> 20;
            let z = ((value << 26) >> 38) as i32;
            return Some(Self { x, y, z });
        }
        if let Some(v) = n.int_array().filter(|v| v.len() == 3) {
            return Some(Self {
                x: v[0],
                y: v[1],
                z: v[2],
            });
        }
        let (x, y, z) = (
            n.get("X").or_else(|| n.get("x"))?.i64()?,
            n.get("Y").or_else(|| n.get("y"))?.i64()?,
            n.get("Z").or_else(|| n.get("z"))?.i64()?,
        );
        Some(Self {
            x: x.try_into().ok()?,
            y: y.try_into().ok()?,
            z: z.try_into().ok()?,
        })
    }
}

// Lootr's DataStorage implementation uses the overworld data storage even for other dimensions.
// We still require exact NBT dimension metadata and position after reading that one UUID file.
fn dimension_matches(saved: &str, expected: &str) -> bool {
    saved == expected || (saved == "overworld" && expected == "minecraft:overworld")
}

fn parse_player_inventories(rows: &[crate::prism::Nbt]) -> Vec<PlayerInventory> {
    rows.iter()
        .filter_map(|row| {
            let uuid = row.get("uuid").and_then(crate::prism::Nbt::uuid_string)?;
            let chest = row.get("chest")?;
            let items = chest
                .get("Items")
                .and_then(crate::prism::Nbt::list)
                .map(|list| {
                    list.iter()
                        .filter_map(crate::prism::preview_item)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(PlayerInventory { uuid, items })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::BlockPos;
    use super::*;
    use serde_json::json;
    use std::io::Write;
    fn pack(tables: &[(&str, serde_json::Value)]) -> TablePack {
        TablePack {
            tables: tables
                .iter()
                .map(|(id, table)| {
                    (
                        (*id).into(),
                        TableRecord {
                            table: Some(table.clone()),
                            ambiguous_sources: vec![],
                        },
                    )
                })
                .collect(),
            block_loot: BTreeMap::new(),
        }
    }
    #[test]
    fn nested_tables_alternatives_and_count_ranges_are_possible_not_rolls() {
        let p = pack(&[
            (
                "test:root",
                json!({"pools":[{"entries":[{"type":"minecraft:loot_table","name":"test:nested"}]}]}),
            ),
            (
                "test:nested",
                json!({"pools":[{"entries":[{"type":"minecraft:alternatives","children":[
                    {"type":"minecraft:item","name":"minecraft:bread","functions":[{"function":"minecraft:set_count","count":{"type":"minecraft:uniform","min":2,"max":5}}]},
                    {"type":"minecraft:item","name":"minecraft:apple"}
                ]}]}]}),
            ),
        ]);
        let mut visited = BTreeSet::new();
        let mut drops = BTreeMap::new();
        let mut notes = BTreeSet::new();
        collect_table("test:root", &p, &mut visited, &mut drops, &mut notes, 0);
        assert_eq!(drops.get("minecraft:bread"), Some(&(2, 5)));
        assert_eq!(drops.get("minecraft:apple"), Some(&(1, 1)));
    }
    #[test]
    fn unavailable_or_ambiguous_tables_do_not_fabricate_items() {
        let p = pack(&[(
            "test:root",
            json!({"pools":[{"entries":[{"type":"minecraft:loot_table","name":"test:missing"}]}]}),
        )]);
        let mut visited = BTreeSet::new();
        let mut drops = BTreeMap::new();
        let mut notes = BTreeSet::new();
        collect_table("test:root", &p, &mut visited, &mut drops, &mut notes, 0);
        assert!(drops.is_empty());
        assert!(notes.iter().any(|x| x.contains("test:missing")));
    }
    #[test]
    fn actual_known_empty_inventory_remains_distinct_from_unopened_table() {
        let mut a = Container::new(BlockPos { x: 1, y: 2, z: 3 }, "minecraft:chest".into());
        a.contents_known = true;
        let mut b = Container::new(BlockPos { x: 1, y: 2, z: 3 }, "minecraft:chest".into());
        b.loot_table = Some("test:missing".into());
        assert!(a.contents_known && a.items.is_empty());
        assert!(!b.contents_known && b.items.is_empty());
    }

    #[test]
    fn lootr_saved_data_path_uses_the_uuid_shard_layout() {
        let path = lootr_saved_path(
            Path::new("world/data/lootr"),
            "069a79f4-4e94-4726-a5be-fca90e38aaf5",
        )
        .unwrap();
        assert_eq!(
            path,
            Path::new("world/data/lootr/0/06/069a79f4-4e94-4726-a5be-fca90e38aaf5.dat")
        );
        assert!(lootr_saved_path(Path::new("world/data/lootr"), "bad-uuid").is_none());
    }

    #[test]
    fn saved_player_uuid_records_keep_known_empty_separate_from_missing() {
        use crate::prism::Nbt;
        use std::collections::HashMap;
        let uuid = Nbt::IntArray(vec![
            0x069a79f4,
            0x4e944726,
            0xa5befca9u32 as i32,
            0x0e38aaf5,
        ]);
        assert_eq!(uuid.int_array().map(|a| a.len()), Some(4));
        let stack = Nbt::Compound(HashMap::from([
            ("id".into(), Nbt::String("minecraft:bread".into())),
            ("Count".into(), Nbt::Number(3.)),
            ("Slot".into(), Nbt::Number(7.)),
        ]));
        let empty_inventory =
            Nbt::Compound(HashMap::from([("Items".into(), Nbt::List(vec![stack]))]));
        let row = Nbt::Compound(HashMap::from([
            ("uuid".into(), uuid),
            ("chest".into(), empty_inventory),
        ]));
        let parsed = parse_player_inventories(&[row]);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].uuid, "069a79f4-4e94-4726-a5be-fca90e38aaf5");
        assert_eq!(parsed[0].items.len(), 1);
        assert_eq!(parsed[0].items[0].id, "minecraft:bread");
        assert_eq!(parsed[0].items[0].count, 3);
        assert_eq!(parsed[0].items[0].slot, Some(7));
        let known_empty = Nbt::Compound(HashMap::from([
            (
                "uuid".into(),
                Nbt::IntArray(vec![
                    0x069a79f4,
                    0x4e944726,
                    0xa5befca9u32 as i32,
                    0x0e38aaf5,
                ]),
            ),
            (
                "chest".into(),
                Nbt::Compound(HashMap::from([("Items".into(), Nbt::List(vec![]))])),
            ),
        ]));
        assert_eq!(parse_player_inventories(&[known_empty])[0].items.len(), 0);
        assert!(parse_player_inventories(&[]).is_empty());
    }

    fn uuid_ints(uuid: &str) -> [i32; 4] {
        let compact = uuid.replace('-', "");
        std::array::from_fn(|i| {
            i32::from_be_bytes(
                u32::from_str_radix(&compact[i * 8..i * 8 + 8], 16)
                    .unwrap()
                    .to_be_bytes(),
            )
        })
    }
    fn nbt_text(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(&(text.len() as u16).to_be_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    fn named_string(out: &mut Vec<u8>, key: &str, value: &str) {
        out.push(8);
        nbt_text(out, key);
        nbt_text(out, value);
    }
    fn named_int(out: &mut Vec<u8>, key: &str, value: i32) {
        out.push(3);
        nbt_text(out, key);
        out.extend_from_slice(&value.to_be_bytes());
    }
    fn named_uuid(out: &mut Vec<u8>, key: &str, value: &str) {
        out.push(11);
        nbt_text(out, key);
        out.extend_from_slice(&4i32.to_be_bytes());
        for i in uuid_ints(value) {
            out.extend_from_slice(&i.to_be_bytes());
        }
    }
    fn item_payload(id: &str, count: i8, slot: i8) -> Vec<u8> {
        let mut out = Vec::new();
        named_string(&mut out, "id", id);
        out.push(1);
        nbt_text(&mut out, "Count");
        out.push(count as u8);
        out.push(1);
        nbt_text(&mut out, "Slot");
        out.push(slot as u8);
        out.push(0);
        out
    }
    fn inventory_payload(player: &str, items: &[(&str, i8, i8)]) -> Vec<u8> {
        let mut chest = Vec::new();
        chest.push(9);
        nbt_text(&mut chest, "Items");
        chest.push(10);
        chest.extend_from_slice(&(items.len() as i32).to_be_bytes());
        for (id, count, slot) in items {
            chest.extend(item_payload(id, *count, *slot));
        }
        chest.push(0);
        let mut row = Vec::new();
        named_uuid(&mut row, "uuid", player);
        row.push(10);
        nbt_text(&mut row, "chest");
        row.extend(chest);
        row.push(0);
        row
    }
    fn lootr_nbt(
        saved_id: &str,
        pos: BlockPos,
        dimension: &str,
        players: &[(&str, Vec<(&str, i8, i8)>)],
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(10);
        nbt_text(&mut data, "position");
        named_int(&mut data, "X", pos.x);
        named_int(&mut data, "Y", pos.y);
        named_int(&mut data, "Z", pos.z);
        data.push(0);
        named_string(&mut data, "dimension", dimension);
        named_uuid(&mut data, "uuid", saved_id);
        data.push(9);
        nbt_text(&mut data, "inventories");
        data.push(10);
        data.extend_from_slice(&(players.len() as i32).to_be_bytes());
        for (player, items) in players {
            data.extend(inventory_payload(player, items));
        }
        data.push(0);
        let mut root = vec![10];
        nbt_text(&mut root, "");
        root.push(10);
        nbt_text(&mut root, "data");
        root.extend(data);
        root.push(0);
        root
    }
    fn write_lootr_file(
        world: &std::path::Path,
        tile_id: &str,
        bytes: &[u8],
    ) -> std::path::PathBuf {
        let path = lootr_saved_path(&world.join("data/lootr"), tile_id).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = fs::File::create(&path).unwrap();
        let mut gzip = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        gzip.write_all(bytes).unwrap();
        gzip.finish().unwrap();
        path
    }
    fn temp_world(label: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "terralens-lootr-{label}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn lootr_container(id: &str, pos: BlockPos, dimension: &str) -> Container {
        let mut c = Container::new(pos, "lootr:lootr_chest".into());
        c.dimension = Some(dimension.into());
        c.lootr = true;
        c.lootr_id = Some(id.into());
        c
    }
    fn saved_preview(container: Container) -> StructurePreview {
        let mut preview =
            StructurePreview::empty("test:structure", super::super::PreviewSource::Saved);
        preview.containers.push(container);
        preview
    }

    #[test]
    fn actual_compressed_lootr_record_loads_per_player_items_and_known_empty_inventory() {
        let tile = "0f111111-2222-4333-8444-555555555555";
        let player_a = "12345678-1234-4234-8234-123456789abc";
        let player_b = "87654321-4321-4321-8321-cba987654321";
        let pos = BlockPos {
            x: 19698,
            y: 71,
            z: -3259,
        };
        let world = temp_world("actual");
        let bytes = lootr_nbt(
            tile,
            pos,
            "minecraft:overworld",
            &[
                (
                    player_a,
                    vec![("minecraft:bread", 3, 4), ("minecraft:iron_ingot", 2, 8)],
                ),
                (player_b, vec![]),
            ],
        );
        let file = write_lootr_file(&world, tile, &bytes);
        assert!(file.ends_with("0/0f/0f111111-2222-4333-8444-555555555555.dat"));
        let mut preview = saved_preview(lootr_container(tile, pos, "minecraft:overworld"));
        enrich(&mut preview, Some(&world), &|| false);
        let c = &preview.containers[0];
        assert!(c.contents_known);
        assert_eq!(c.player_inventories.len(), 2);
        assert_eq!(c.player_inventories[0].uuid, player_a);
        assert_eq!(c.player_inventories[0].items.len(), 2);
        assert_eq!(c.player_inventories[0].items[0].id, "minecraft:bread");
        assert_eq!(c.player_inventories[0].items[0].count, 3);
        assert_eq!(c.player_inventories[0].items[0].slot, Some(4));
        assert_eq!(c.player_inventories[1].uuid, player_b);
        assert!(c.player_inventories[1].items.is_empty());
        let _ = fs::remove_dir_all(world);
    }

    #[test]
    fn lootr_records_with_wrong_position_dimension_or_uuid_are_rejected() {
        let tile = "0f111111-2222-4333-8444-555555555555";
        let pos = BlockPos {
            x: 10,
            y: 20,
            z: 30,
        };
        for (label, saved_id, saved_pos, saved_dim) in [
            (
                "position",
                tile,
                BlockPos {
                    x: 11,
                    y: 20,
                    z: 30,
                },
                "minecraft:overworld",
            ),
            ("dimension", tile, pos, "minecraft:the_nether"),
            (
                "uuid",
                "1f111111-2222-4333-8444-555555555555",
                pos,
                "minecraft:overworld",
            ),
        ] {
            let world = temp_world(label);
            let bytes = lootr_nbt(
                saved_id,
                saved_pos,
                saved_dim,
                &[(
                    "12345678-1234-4234-8234-123456789abc",
                    vec![("minecraft:diamond", 1, 0)],
                )],
            );
            write_lootr_file(&world, tile, &bytes);
            let mut preview = saved_preview(lootr_container(tile, pos, "minecraft:overworld"));
            enrich(&mut preview, Some(&world), &|| false);
            assert!(
                !preview.containers[0].contents_known,
                "accepted mismatched {label}"
            );
            assert!(preview.containers[0].player_inventories.is_empty());
            let _ = fs::remove_dir_all(world);
        }
    }

    #[test]
    fn actual_lootr_record_with_no_player_inventory_stays_unknown() {
        let tile = "0f111111-2222-4333-8444-555555555555";
        let pos = BlockPos { x: 1, y: 2, z: 3 };
        let world = temp_world("unopened");
        write_lootr_file(
            &world,
            tile,
            &lootr_nbt(tile, pos, "minecraft:overworld", &[]),
        );
        let mut preview = saved_preview(lootr_container(tile, pos, "minecraft:overworld"));
        enrich(&mut preview, Some(&world), &|| false);
        assert!(!preview.containers[0].contents_known);
        assert!(preview.containers[0].player_inventories.is_empty());
        assert!(
            preview.containers[0]
                .notes
                .iter()
                .any(|n| n.contains("no saved player inventory"))
        );
        let _ = fs::remove_dir_all(world);
    }

    #[test]
    fn loot_seed_note_uses_explicit_setting_and_stays_unconfirmed_without_config() {
        let mut c = Container::new(BlockPos { x: 0, y: 64, z: 0 }, "minecraft:chest".into());
        c.loot_table = Some("test:chest".into());
        let mut preview = StructurePreview::empty(
            "test:structure",
            super::super::PreviewSource::TemplateLayout,
        );
        preview.containers.push(c);
        let randomizes = apply_pack_lootr_policy(
            &mut preview,
            Some("disable = false\nrandomise_seed = false\nconvert_wooden_chests = true\n"),
        );
        assert_eq!(randomizes, Some(false));
        assert!(preview.containers[0].lootr);
        assert!(
            preview.containers[0]
                .notes
                .iter()
                .any(|n| n.contains("remain unknown"))
        );
        assert!(
            !preview.containers[0]
                .notes
                .iter()
                .any(|n| n.contains("randomized"))
        );

        let mut unknown = StructurePreview::empty(
            "test:structure",
            super::super::PreviewSource::TemplateLayout,
        );
        let mut c = Container::new(BlockPos { x: 0, y: 64, z: 0 }, "minecraft:chest".into());
        c.loot_table = Some("test:chest".into());
        unknown.containers.push(c);
        assert_eq!(apply_pack_lootr_policy(&mut unknown, None), None);
        assert!(
            unknown.containers[0]
                .notes
                .iter()
                .any(|n| n.contains("Whether Lootr converts"))
        );
        assert!(
            !unknown.containers[0]
                .notes
                .iter()
                .any(|n| n.contains("predicted"))
        );
    }
}
