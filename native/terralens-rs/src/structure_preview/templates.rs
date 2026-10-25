//! Static template geometry extracted at maintenance time; never reads mod jars at runtime.
use super::*;
use crate::random::JavaRandom;
use serde::Deserialize;
use std::{collections::HashMap, io::Read, sync::OnceLock};

const DATA: &[u8] = include_bytes!("../../assets/data/tfg/structure-previews.json.gz");
#[path = "jigsaw.rs"]
mod jigsaw;

#[derive(Deserialize)]
pub(super) struct Database {
    pub(super) structures: HashMap<String, StructureDef>,
    pub(super) templates: HashMap<String, Template>,
    pub(super) pools: HashMap<String, Pool>,
    pub(super) processors: HashMap<String, Vec<String>>,
}
#[derive(Deserialize)]
pub(super) struct StructureDef {
    pub(super) start_pool: Option<String>,
    pub(super) size: u8,
    pub(super) templates: Vec<String>,
    #[serde(default)]
    pub(super) start_height: serde_json::Value,
    #[serde(default)]
    pub(super) projection: Option<String>,
}
#[derive(Deserialize)]
pub(super) struct Template {
    pub(super) size: [i32; 3],
    pub(super) palette: Vec<String>,
    pub(super) blocks: Vec<[i32; 4]>,
    pub(super) containers: Vec<RawContainer>,
    #[serde(default)]
    pub(super) jigsaws: Vec<JigsawConnector>,
}
#[derive(Clone, Debug, Deserialize)]
pub(super) struct JigsawConnector {
    pub(super) pos: [i32; 3],
    pub(super) name: String,
    pub(super) target: String,
    pub(super) pool: String,
    pub(super) joint: String,
    pub(super) orientation: String,
    #[serde(default)]
    pub(super) final_state: Option<String>,
}
#[derive(Deserialize)]
pub(super) struct RawContainer {
    pub(super) pos: [i32; 3],
    pub(super) block: String,
    pub(super) loot_table: Option<String>,
    pub(super) loot_seed: Option<i64>,
    pub(super) items: Vec<RawItem>,
}
#[derive(Deserialize)]
pub(super) struct RawItem {
    pub(super) id: String,
    pub(super) count: u32,
    pub(super) slot: Option<u16>,
}
#[derive(Deserialize)]
pub(super) struct Pool {
    #[serde(default)]
    pub(super) fallback: Option<String>,
    pub(super) entries: Vec<PoolEntry>,
}
#[derive(Deserialize)]
pub(super) struct PoolEntry {
    pub(super) weight: u32,
    pub(super) element: Element,
}
#[derive(Clone, Deserialize)]
pub(super) struct Element {
    pub(super) kind: String,
    pub(super) template: Option<String>,
    pub(super) processors: Option<String>,
    #[serde(default)]
    pub(super) elements: Vec<Element>,
    #[serde(default)]
    pub(super) delegate: Option<Box<Element>>,
    #[serde(default)]
    pub(super) limit: Option<u32>,
    #[serde(default)]
    pub(super) projection: Option<String>,
}

fn database() -> &'static Database {
    static DB: OnceLock<Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(DATA)
            .read_to_end(&mut bytes)
            .expect("embedded structure preview archive");
        serde_json::from_slice(&bytes).expect("valid structure preview archive")
    })
}

/// Horizontal GenerationStub position used by vanilla jigsaw biome checks and
/// Lithostitched spawn conditions. It is the rotated starting piece's integer
/// bounding-box center, independent of terrain projection or later child pieces.
pub(crate) fn generation_stub_xz(
    seed: i64,
    id: &str,
    chunk_x: i32,
    chunk_z: i32,
) -> Option<(i32, i32)> {
    let db = database();
    let def = db.structures.get(id)?;
    let selected = select_start(db, def, seed, chunk_x, chunk_z)?;
    let template = db.templates.get(&selected.template)?;
    let [sx, _, sz] = template.size;
    if sx <= 0 || sz <= 0 {
        return None;
    }
    let corners = [(0, 0), (sx - 1, 0), (0, sz - 1), (sx - 1, sz - 1)]
        .map(|(x, z)| rotate(x, z, selected.rotation));
    let origin_x = chunk_x.saturating_mul(16);
    let origin_z = chunk_z.saturating_mul(16);
    let min_x = origin_x.saturating_add(corners.iter().map(|p| p.0).min()?);
    let max_x = origin_x.saturating_add(corners.iter().map(|p| p.0).max()?);
    let min_z = origin_z.saturating_add(corners.iter().map(|p| p.1).min()?);
    let max_z = origin_z.saturating_add(corners.iter().map(|p| p.1).max()?);
    Some((
        ((min_x as i64 + max_x as i64) / 2) as i32,
        ((min_z as i64 + max_z as i64) / 2) as i32,
    ))
}

/// Predicts and draws the actual start-pool template selected by the Java large-feature
/// random stream. Further jigsaw expansion is deliberately surfaced as an incomplete preview.
pub fn predict(
    world: &mut crate::world::World,
    marker: &Marker,
    cancel: &impl Fn() -> bool,
) -> Result<StructurePreview, String> {
    if cancel() {
        return Err("Inspection cancelled".into());
    }
    let db = database();
    let Some(def) = db
        .structures
        .get(&marker.label)
        .or_else(|| db.structures.get(&marker.id))
    else {
        let mut out = StructurePreview::empty(&marker.label, PreviewSource::TemplateLayout);
        out.notes
            .push("No extracted template data is available for this structure type.".into());
        out.complete = false;
        return Ok(out);
    };
    let (chunk_x, chunk_z) = (
        (marker.x.floor() as i32).div_euclid(16),
        (marker.z.floor() as i32).div_euclid(16),
    );
    let Some(selection) = select_start(db, def, world.seed, chunk_x, chunk_z) else {
        let mut out = StructurePreview::empty(&marker.label, PreviewSource::TemplateLayout);
        out.notes.push(
            "The start pool uses a feature or unresolved element, so no template was guessed."
                .into(),
        );
        out.complete = false;
        return Ok(out);
    };
    let StartSelection {
        template: template_id,
        processor,
        rotation,
        provider_y,
        mut random,
    } = selection;
    let Some(template) = db.templates.get(&template_id) else {
        let mut out = StructurePreview::empty(&marker.label, PreviewSource::TemplateLayout);
        out.notes.push(format!(
            "The selected start template {template_id} is unavailable."
        ));
        out.complete = false;
        return Ok(out);
    };
    let x0 = chunk_x.saturating_mul(16);
    let z0 = chunk_z.saturating_mul(16);
    let (sx, sz) = (template.size[0].max(1), template.size[2].max(1));
    let corners = [(0, 0), (sx - 1, 0), (0, sz - 1), (sx - 1, sz - 1)];
    let rotated_corners: Vec<_> = corners
        .iter()
        .map(|&(x, z)| rotate(x, z, rotation))
        .collect();
    let min_x = rotated_corners.iter().map(|p| p.0).min().unwrap_or(0);
    let max_x = rotated_corners.iter().map(|p| p.0).max().unwrap_or(0);
    let min_z = rotated_corners.iter().map(|p| p.1).min().unwrap_or(0);
    let max_z = rotated_corners.iter().map(|p| p.1).max().unwrap_or(0);
    let center_x = x0.saturating_add((min_x + max_x) / 2);
    let center_z = z0.saturating_add((min_z + max_z) / 2);
    let y0 = marker.y.map(|v| v.floor() as i32).unwrap_or_else(|| {
        if def.projection.is_some() {
            world
                .height(center_x, center_z)
                .unwrap_or(provider_y.unwrap_or(64) as f64)
                .floor() as i32
        } else {
            provider_y
                .unwrap_or_else(|| world.height(center_x, center_z).unwrap_or(64.0).floor() as i32)
        }
    });
    let bounds_center = BlockPos {
        x: x0.saturating_add((min_x + max_x) / 2),
        y: y0,
        z: z0.saturating_add((min_z + max_z) / 2),
    };
    let anchor = BlockPos {
        x: x0,
        y: y0,
        z: z0,
    };
    let mut preview = jigsaw::expand(
        db,
        &template_id,
        def.start_pool.as_deref().unwrap_or(&marker.label),
        processor.as_deref(),
        anchor,
        rotation,
        def.size,
        &mut random,
        cancel,
    )?;
    preview.structure_id = marker.label.clone();
    preview.source = PreviewSource::TemplateLayout;
    preview.notes.push(format!(
        "Seed-selected start piece: {template_id} · rotation {rotation}"
    ));
    preview.notes.push(format!(
        "Placement Y uses the sampled {} model at {}, {}.",
        def.projection.as_deref().unwrap_or("start-height"),
        bounds_center.x,
        bounds_center.z
    ));
    Ok(preview)
}

struct StartSelection {
    template: String,
    processor: Option<String>,
    rotation: i32,
    provider_y: Option<i32>,
    random: JavaRandom,
}

fn select_start(
    db: &Database,
    def: &StructureDef,
    seed: i64,
    chunk_x: i32,
    chunk_z: i32,
) -> Option<StartSelection> {
    let mut random = JavaRandom::new(large_feature_seed(seed, chunk_x, chunk_z));
    // The structure start-height provider is sampled before JigsawPlacement.addPieces.
    let provider_y = sample_start_height(&def.start_height, &mut random);
    // addPieces draws Rotation before its weighted start-pool element.
    let rotation = random.next_int_bound(4);
    let selected = def
        .start_pool
        .as_deref()
        .and_then(|id| choose_pool_template(db, id, &mut random, &mut Vec::new()))
        .or_else(|| {
            (def.start_pool.is_none() && def.templates.len() == 1)
                .then(|| (def.templates[0].clone(), None))
        })?;
    Some(StartSelection {
        template: selected.0,
        processor: selected.1,
        rotation,
        provider_y,
        random,
    })
}

fn sample_start_height(provider: &serde_json::Value, random: &mut JavaRandom) -> Option<i32> {
    if let Some(value) = provider.get("absolute").and_then(serde_json::Value::as_i64) {
        return Some(value.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
    }
    if provider.get("type").and_then(serde_json::Value::as_str) == Some("minecraft:uniform")
        || provider.get("type").and_then(serde_json::Value::as_str) == Some("uniform")
    {
        let bound = |v: &serde_json::Value| {
            v.get("absolute")
                .and_then(serde_json::Value::as_i64)
                .or_else(|| v.get("above_bottom").and_then(serde_json::Value::as_i64))
                .or_else(|| v.get("below_top").and_then(serde_json::Value::as_i64))
                .map(|v| v.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
        };
        let min = bound(&provider["min_inclusive"])?;
        let max = bound(&provider["max_inclusive"])?;
        if min > max {
            return None;
        }
        let span = (max as i64 - min as i64 + 1).min(i32::MAX as i64) as i32;
        return Some(min.saturating_add(random.next_int_bound(span)));
    }
    None
}

fn choose_pool_template(
    db: &Database,
    id: &str,
    rng: &mut JavaRandom,
    seen: &mut Vec<String>,
) -> Option<(String, Option<String>)> {
    if seen.len() >= 16 || seen.iter().any(|v| v == id) {
        return None;
    }
    let pool = db.pools.get(id)?;
    seen.push(id.to_owned());
    let mut choices = Vec::<(u32, Option<(String, Option<String>)>)>::new();
    for entry in &pool.entries {
        if entry.element.kind != "single" || entry.element.template.is_none() {
            return None;
        }
        choices.push((
            entry.weight,
            Some((
                entry.element.template.clone()?,
                entry.element.processors.clone(),
            )),
        ));
    }
    if choices.is_empty() {
        return None;
    }
    let total = choices.iter().map(|c| c.0).sum::<u32>();
    if total == 0 {
        return None;
    }
    let mut pick = rng.next_int_bound(total.min(i32::MAX as u32) as i32) as u32;
    let mut chosen = None;
    for (weight, template) in choices {
        if pick < weight {
            chosen = template;
            break;
        }
        pick -= weight;
    }
    chosen
}

fn large_feature_seed(seed: i64, x: i32, z: i32) -> i64 {
    let mut random = JavaRandom::new(seed);
    let a = random.next_long();
    let b = random.next_long();
    (x as i64).wrapping_mul(a) ^ (z as i64).wrapping_mul(b) ^ seed
}
fn rotate(x: i32, z: i32, rotation: i32) -> (i32, i32) {
    match rotation {
        1 => (-z, x),
        2 => (-x, -z),
        3 => (z, -x),
        _ => (x, z),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn malay_biome_gate_uses_rotated_start_stub_not_chunk_origin_or_marker() {
        assert_eq!(
            generation_stub_xz(0, "tfg:illagers/malay_village_living", 991, 990),
            Some((15851, 15845))
        );
        assert_ne!(
            generation_stub_xz(0, "tfg:illagers/malay_village_living", 991, 990),
            Some((15856, 15840))
        );
    }

    #[test]
    fn saved_world_zero_start_templates_rotations_and_footprints_match() {
        // Read-only starts decoded from saves/0, region r.38.-7.mca. Minecraft rotation
        // order: NONE, CLOCKWISE_90, CLOCKWISE_180, COUNTERCLOCKWISE_90.
        let fixtures = [
            (
                "tfc_ruins:ruin_small",
                1231,
                -204,
                "tfc_ruins:ruin_small/ruin_8",
                0,
                19696,
                19703,
                -3264,
                -3258,
                69,
                75,
            ),
            (
                "tfc_ruins:ruin_small",
                1244,
                -222,
                "tfc_ruins:ruin_small/ruin_3",
                1,
                19899,
                19904,
                -3552,
                -3547,
                68,
                73,
            ),
            (
                "tfc_ruined_world:limestone_church",
                1225,
                -215,
                "tfc_ruined_world:limestone_church",
                2,
                19581,
                19600,
                -3446,
                -3440,
                70,
                80,
            ),
            (
                "tfg:illagers/forest_roaming",
                1234,
                -210,
                "tfg:illager_generic/gatherers",
                0,
                19744,
                19745,
                -3360,
                -3359,
                68,
                71,
            ),
            (
                "tfg:illagers/camps",
                1230,
                -209,
                "tfg:illager_generic/small_camp_2",
                3,
                19680,
                19684,
                -3348,
                -3344,
                67,
                71,
            ),
            (
                "tfc_ruined_world:tower_5",
                1234,
                -206,
                "tfc_ruined_world:tower_5",
                1,
                19738,
                19744,
                -3296,
                -3290,
                68,
                80,
            ),
        ];
        let db = database();
        for (id, cx, cz, expected, rotation, min_x, max_x, min_z, max_z, min_y, max_y) in fixtures {
            let def = db.structures.get(id).unwrap();
            let mut selected = select_start(db, def, 0, cx, cz).unwrap();
            assert_eq!(selected.template, expected, "{id} at {cx},{cz}");
            assert_eq!(selected.rotation, rotation, "{id} at {cx},{cz}");
            let result = jigsaw::expand(
                db,
                &selected.template,
                def.start_pool.as_deref().unwrap_or(id),
                selected.processor.as_deref(),
                BlockPos {
                    x: cx * 16,
                    y: min_y,
                    z: cz * 16,
                },
                selected.rotation,
                0,
                &mut selected.random,
                &|| false,
            )
            .unwrap();
            let bb = result.bounds.unwrap();
            assert_eq!(
                (bb.min_x, bb.max_x, bb.min_z, bb.max_z, bb.min_y, bb.max_y),
                (min_x, max_x, min_z, max_z, min_y, max_y),
                "{id} at {cx},{cz}"
            );
        }
    }

    #[test]
    fn village_and_moonbase_start_pieces_match_saved_generation() {
        let db = database();
        for (id, cx, cz, template, rotation) in [
            (
                "tfg:illagers/arabic_village_living",
                1542,
                702,
                "tfg:arabic/baseline/well",
                2,
            ),
            (
                "tfg:moon/moonbase",
                826,
                862,
                "tfg:moon/moonbase/entrance/moonbase_entrance2",
                1,
            ),
        ] {
            let selected = select_start(db, db.structures.get(id).unwrap(), 0, cx, cz).unwrap();
            assert_eq!(selected.template, template, "{id}");
            assert_eq!(selected.rotation, rotation, "{id}");
        }
    }

    #[test]
    fn village_and_moonbase_layouts_expand_to_loot_bearing_rooms() {
        let db = database();
        for (id, cx, cz, y, saved_pieces) in [
            ("tfg:illagers/arabic_village_living", 1542, 702, 89, 90usize),
            ("tfg:moon/moonbase", 826, 862, 103, 24usize),
        ] {
            let def = db.structures.get(id).unwrap();
            let selected = select_start(db, def, 0, cx, cz).unwrap();
            let mut random = selected.random;
            let preview = jigsaw::expand(
                db,
                &selected.template,
                def.start_pool.as_deref().unwrap_or(id),
                selected.processor.as_deref(),
                BlockPos {
                    x: cx * 16,
                    y,
                    z: cz * 16,
                },
                selected.rotation,
                def.size,
                &mut random,
                &|| false,
            )
            .unwrap();
            eprintln!(
                "{id}: preview {}/{} saved pieces, {} blocks, {} containers, bounds {:?}",
                preview.pieces.len(),
                saved_pieces,
                preview.blocks.len(),
                preview.containers.len(),
                preview.bounds
            );
            assert!(!preview.complete);
            assert!(preview.bounds.is_some());
            assert!(preview.blocks.len() <= crate::structure_preview::MAX_SHAPE_BLOCKS);
            assert!(
                preview.pieces.len() > 1,
                "{id} must extend beyond its entrance"
            );
            assert!(
                preview
                    .containers
                    .iter()
                    .any(|c| c.loot_table.is_some() || !c.items.is_empty()),
                "{id} must reach loot-bearing rooms rather than only drawing roads or an entrance"
            );
        }
    }

    #[test]
    fn jigsaw_connector_places_child_and_keeps_unknown_processor_inventory_unconfirmed() {
        let connector =
            |pos, name: &str, target: &str, pool: &str, orientation: &str| JigsawConnector {
                pos,
                name: name.into(),
                target: target.into(),
                pool: pool.into(),
                joint: "aligned".into(),
                orientation: orientation.into(),
                final_state: Some("minecraft:air".into()),
            };
        let template = |block: &str, connectors: Vec<JigsawConnector>, containers| Template {
            size: [1, 1, 1],
            palette: vec![block.into()],
            blocks: vec![[0, 0, 0, 0]],
            containers,
            jigsaws: connectors,
        };
        let item = RawItem {
            id: "minecraft:diamond".into(),
            count: 1,
            slot: Some(0),
        };
        let mut templates = HashMap::new();
        templates.insert(
            "test:root".into(),
            template(
                "minecraft:jigsaw",
                vec![connector(
                    [0, 0, 0],
                    "test:source",
                    "test:target",
                    "test:child_pool",
                    "east_up",
                )],
                vec![],
            ),
        );
        let mut child = template(
            "minecraft:jigsaw",
            vec![connector(
                [0, 0, 0],
                "test:target",
                "test:unused",
                "minecraft:empty",
                "west_up",
            )],
            vec![RawContainer {
                pos: [1, 0, 0],
                block: "minecraft:chest".into(),
                loot_table: None,
                loot_seed: None,
                items: vec![item],
            }],
        );
        child.size = [2, 1, 1];
        child.palette = vec!["minecraft:jigsaw".into(), "minecraft:gold_block".into()];
        child.blocks = vec![[0, 0, 0, 0], [1, 0, 0, 1]];
        templates.insert("test:child".into(), child);
        let mut pools = HashMap::new();
        pools.insert(
            "test:child_pool".into(),
            Pool {
                fallback: Some("minecraft:empty".into()),
                entries: vec![PoolEntry {
                    weight: 1,
                    element: Element {
                        kind: "single".into(),
                        template: Some("test:child".into()),
                        processors: Some("test:unknown".into()),
                        elements: vec![],
                        delegate: None,
                        limit: None,
                        projection: Some("rigid".into()),
                    },
                }],
            },
        );
        let db = Database {
            structures: HashMap::new(),
            templates,
            pools,
            processors: HashMap::new(),
        };
        let preview = jigsaw::expand(
            &db,
            "test:root",
            "test:start_pool",
            None,
            BlockPos {
                x: 10,
                y: 64,
                z: 30,
            },
            0,
            1,
            &mut JavaRandom::new(17),
            &|| false,
        )
        .unwrap();
        assert_eq!(preview.pieces.len(), 2, "{:?}", preview.notes);
        assert!(
            preview
                .blocks
                .iter()
                .any(|b| b.block == "minecraft:gold_block"
                    && b.pos
                        == BlockPos {
                            x: 12,
                            y: 64,
                            z: 30
                        })
        );
        assert_eq!(preview.containers.len(), 1);
        assert!(!preview.containers[0].contents_known);
        assert!(!preview.complete);
    }

    #[test]
    fn jigsaw_block_geometry_stays_within_preview_cap() {
        let template = Template {
            size: [crate::structure_preview::MAX_SHAPE_BLOCKS as i32 + 17, 1, 1],
            palette: vec!["minecraft:stone".into()],
            blocks: (0..crate::structure_preview::MAX_SHAPE_BLOCKS as i32 + 17)
                .map(|x| [x, 0, 0, 0])
                .collect(),
            containers: vec![],
            jigsaws: vec![],
        };
        let db = Database {
            structures: HashMap::new(),
            templates: HashMap::from([("test:wide".into(), template)]),
            pools: HashMap::new(),
            processors: HashMap::new(),
        };
        let preview = jigsaw::expand(
            &db,
            "test:wide",
            "test:start",
            None,
            BlockPos { x: 0, y: 0, z: 0 },
            0,
            0,
            &mut JavaRandom::new(0),
            &|| false,
        )
        .unwrap();
        assert_eq!(
            preview.blocks.len(),
            crate::structure_preview::MAX_SHAPE_BLOCKS
        );
        assert!(
            preview
                .notes
                .iter()
                .any(|note| note.contains("memory limit"))
        );
    }
}
