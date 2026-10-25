//! Seed-ordered start-piece and bounded jigsaw layout preview.
use super::templates::{Database, PoolEntry, Template};
use super::*;
use crate::random::JavaRandom;
use std::collections::{HashMap, HashSet};

const MAX_PIECES: usize = 512;

#[derive(Clone)]
struct PlacedPiece {
    template_id: String,
    origin: BlockPos,
    rotation: i32,
    bounds: StructureBounds,
    depth: u8,
    processor: Option<String>,
}

#[derive(Clone)]
struct Candidate {
    element: super::templates::Element,
}

/// Assemble the selected start template and compatible jigsaw children. Candidate pool
/// ordering and connector transforms use the same Java random stream as the start piece.
/// Terrain-matching projections and nonempty processors remain explicitly approximate.
pub(super) fn expand(
    db: &Database,
    start_template: &str,
    start_pool: &str,
    start_processor: Option<&str>,
    anchor: BlockPos,
    rotation: i32,
    max_depth: u8,
    rng: &mut JavaRandom,
    cancel: &impl Fn() -> bool,
) -> Result<StructurePreview, String> {
    let Some(template) = db.templates.get(start_template) else {
        return Err(format!("Missing extracted start template {start_template}"));
    };
    let mut preview = StructurePreview::empty(start_pool, PreviewSource::TemplateLayout);
    let first = make_piece(
        start_template,
        template,
        anchor,
        rotation,
        0,
        start_processor.map(str::to_owned),
    );
    add_piece(db, template, first.clone(), &mut preview, cancel)?;
    let mut placed = vec![first];
    let mut occupied = occupied_blocks(template, &placed[0]);

    let mut cursor = 0;
    while cursor < placed.len() {
        if cancel() {
            return Err("Structure preview cancelled".into());
        }
        if placed.len() >= MAX_PIECES {
            preview.complete = false;
            preview
                .notes
                .push(format!("Preview stopped at {MAX_PIECES} pieces."));
            break;
        }
        let parent = placed[cursor].clone();
        cursor += 1;
        if parent.depth >= max_depth {
            continue;
        }
        let Some(parent_template) = db.templates.get(&parent.template_id) else {
            preview.complete = false;
            continue;
        };
        let mut connectors = parent_template.jigsaws.clone();
        shuffle(&mut connectors, rng);
        for connector in connectors {
            if cancel() {
                return Err("Structure preview cancelled".into());
            }
            if connector.pool == "minecraft:empty" || connector.pool.is_empty() {
                continue;
            }
            let Some(pool) = db.pools.get(&connector.pool) else {
                preview.complete = false;
                preview
                    .notes
                    .push(format!("Missing jigsaw pool {}.", connector.pool));
                continue;
            };
            let mut candidates = Vec::new();
            if pool
                .entries
                .iter()
                .any(|entry| contains_limited(&entry.element))
            {
                preview.complete = false;
                preview.notes.push(format!(
                    "Limited element rules in {} are approximated by per-template placement counts.",
                    connector.pool
                ));
            }
            if !weighted_pool(&pool.entries, &mut candidates, rng) {
                preview.complete = false;
                preview.notes.push(format!(
                    "Pool {} exceeded the candidate limit.",
                    connector.pool
                ));
                continue;
            }
            if let Some(fallback_id) = pool
                .fallback
                .as_deref()
                .filter(|id| *id != "minecraft:empty")
            {
                if let Some(fallback) = db.pools.get(fallback_id) {
                    if !weighted_pool(&fallback.entries, &mut candidates, rng) {
                        preview.complete = false;
                        preview.notes.push(format!(
                            "Fallback pool {fallback_id} exceeded the candidate limit."
                        ));
                    }
                } else {
                    preview.complete = false;
                    preview
                        .notes
                        .push(format!("Missing fallback pool {fallback_id}."));
                }
            }
            let source_pos = world_pos(parent.origin, connector.pos, parent.rotation);
            let source_front = world_direction(&connector.orientation, parent.rotation);
            let source_top = world_top(&connector.orientation, parent.rotation);
            if source_front == [0, 0, 0] {
                preview.complete = false;
                preview.notes.push(format!(
                    "Unsupported jigsaw orientation {}.",
                    connector.orientation
                ));
                continue;
            }

            let mut attached = None;
            'candidates: for candidate in candidates {
                let mut elements = Vec::new();
                flatten_composite(&candidate.element, None, &mut elements);
                if elements.is_empty() {
                    continue;
                }
                let Some((candidate_id, _, _)) = elements.first() else {
                    continue;
                };
                let Some(child_template) = db.templates.get(candidate_id) else {
                    preview.complete = false;
                    preview
                        .notes
                        .push(format!("Missing template {candidate_id}."));
                    continue;
                };
                let mut rotations = vec![0, 1, 2, 3];
                shuffle(&mut rotations, rng);
                for child_rotation in rotations {
                    let mut targets = child_template.jigsaws.iter().collect::<Vec<_>>();
                    shuffle(&mut targets, rng);
                    for target in targets {
                        if target.name != connector.target {
                            continue;
                        }
                        if world_direction(&target.orientation, child_rotation)
                            != source_front.map(|v| -v)
                        {
                            continue;
                        }
                        if connector.joint == "aligned"
                            && world_top(&target.orientation, child_rotation) != source_top
                        {
                            continue;
                        }
                        let target_pos = rotate_local(target.pos, child_rotation);
                        // Vanilla offsets the child connector one block outward from the
                        // source connector, so the two jigsaw blocks occupy adjacent cells.
                        let origin = BlockPos {
                            x: source_pos.x + source_front[0] - target_pos[0],
                            y: source_pos.y + source_front[1] - target_pos[1],
                            z: source_pos.z + source_front[2] - target_pos[2],
                        };
                        let mut group = Vec::new();
                        let mut valid = true;
                        for (id, processor, limit) in &elements {
                            if id.is_empty() {
                                preview.complete = false;
                                preview.notes.push(format!(
                                    "Unsupported pool element in {} was skipped.",
                                    connector.pool
                                ));
                                valid = false;
                                break;
                            }
                            let Some(part_template) = db.templates.get(id.as_str()) else {
                                preview.complete = false;
                                preview.notes.push(format!("Missing template {id}."));
                                valid = false;
                                break;
                            };
                            if limit.is_some_and(|limit| {
                                placed.iter().filter(|p| p.template_id == *id).count() as u32
                                    >= limit
                            }) {
                                valid = false;
                                break;
                            }
                            let part = make_piece(
                                id,
                                part_template,
                                origin,
                                child_rotation,
                                parent.depth.saturating_add(1),
                                processor.clone(),
                            );
                            if intersects_occupied(part_template, &part, &occupied) {
                                valid = false;
                                break;
                            }
                            group.push(part);
                        }
                        if !valid || group.is_empty() {
                            continue;
                        }
                        if candidate.element.projection.as_deref() == Some("terrain_matching") {
                            preview.complete = false;
                            preview.notes.push(format!(
                                "Terrain-matching piece {candidate_id} keeps template height; terrain offset is unknown."
                            ));
                        }
                        attached = Some(group);
                        break 'candidates;
                    }
                }
            }
            if let Some(children) = attached {
                if placed.len().saturating_add(children.len()) > MAX_PIECES {
                    preview.complete = false;
                    preview
                        .notes
                        .push(format!("Preview stopped at {MAX_PIECES} pieces."));
                    break;
                }
                for child in children {
                    if !processor_preserves_geometry(db, child.processor.as_deref()) {
                        preview.complete = false;
                        preview.notes.push(format!(
                            "Processor on {} can change block and container placement.",
                            child.template_id
                        ));
                    }
                    let child_template = db.templates.get(&child.template_id).expect("checked");
                    occupied.extend(occupied_blocks(child_template, &child));
                    add_piece(db, child_template, child.clone(), &mut preview, cancel)?;
                    placed.push(child);
                }
            }
        }
    }

    preview.bounds = preview.pieces.iter().map(|p| p.bounds).reduce(union_bounds);
    // Composite pool elements intentionally overlap: Minecraft places each member
    // at the same transform, with later child blocks replacing earlier ones.
    let mut last_at = HashMap::with_capacity(preview.blocks.len());
    let mut deduplicated = Vec::with_capacity(preview.blocks.len());
    for block in preview.blocks.drain(..) {
        if let Some(index) = last_at.get(&block.pos).copied() {
            deduplicated[index] = block;
        } else {
            last_at.insert(block.pos, deduplicated.len());
            deduplicated.push(block);
        }
    }
    preview.blocks = deduplicated;
    preview.notes.push(format!(
        "Seed-selected start piece: {start_template}; expanded to depth {max_depth}."
    ));
    preview.notes.push(
        "Template layout · terrain matching, processors, and code-defined features may alter the generated result.".into(),
    );
    preview.complete = false;
    Ok(preview)
}

fn weighted_pool(entries: &[PoolEntry], out: &mut Vec<Candidate>, rng: &mut JavaRandom) -> bool {
    let mut local = Vec::new();
    for entry in entries {
        for _ in 0..entry.weight.max(1).min(8192) {
            if local.len() >= 8192 || out.len().saturating_add(local.len()) >= 8192 {
                return false;
            }
            local.push(Candidate {
                element: entry.element.clone(),
            });
        }
    }
    shuffle(&mut local, rng);
    out.extend(local);
    true
}

/// A ListPoolElement composes every child at the same origin and rotation.
/// LimitedPoolElement constrains how often its delegate may be selected.
fn flatten_composite(
    element: &super::templates::Element,
    inherited_limit: Option<u32>,
    out: &mut Vec<(String, Option<String>, Option<u32>)>,
) {
    let limit = element.limit.or(inherited_limit);
    match element.kind.as_str() {
        "single" => {
            let id = element
                .template
                .clone()
                .filter(|id| id != "minecraft:empty")
                .unwrap_or_default();
            out.push((id, element.processors.clone(), limit));
        }
        "list" => {
            for child in &element.elements {
                flatten_composite(child, limit, out);
            }
        }
        "limited" => {
            if let Some(delegate) = element.delegate.as_deref() {
                flatten_composite(delegate, limit, out);
            } else {
                out.push((String::new(), None, limit));
            }
        }
        "empty" => {}
        _ => out.push((String::new(), None, limit)),
    }
}

fn contains_limited(element: &super::templates::Element) -> bool {
    element.kind == "limited"
        || element.delegate.as_deref().is_some_and(contains_limited)
        || element.elements.iter().any(contains_limited)
}

fn shuffle<T>(items: &mut [T], rng: &mut JavaRandom) {
    for i in (1..items.len()).rev() {
        let j = rng.next_int_bound((i + 1) as i32) as usize;
        items.swap(i, j);
    }
}

fn make_piece(
    id: &str,
    template: &Template,
    origin: BlockPos,
    rotation: i32,
    depth: u8,
    processor: Option<String>,
) -> PlacedPiece {
    let [sx, sy, sz] = template.size;
    let corners = [
        [0, 0, 0],
        [sx - 1, 0, 0],
        [0, 0, sz - 1],
        [sx - 1, 0, sz - 1],
    ];
    let points = corners.map(|p| rotate_local(p, rotation));
    let min_x = points.iter().map(|p| p[0]).min().unwrap_or(0);
    let max_x = points.iter().map(|p| p[0]).max().unwrap_or(0);
    let min_z = points.iter().map(|p| p[2]).min().unwrap_or(0);
    let max_z = points.iter().map(|p| p[2]).max().unwrap_or(0);
    PlacedPiece {
        template_id: id.to_owned(),
        origin,
        rotation,
        depth,
        processor,
        bounds: StructureBounds {
            min_x: origin.x + min_x,
            max_x: origin.x + max_x,
            min_y: origin.y,
            max_y: origin.y + sy.saturating_sub(1),
            min_z: origin.z + min_z,
            max_z: origin.z + max_z,
        },
    }
}

fn add_piece(
    db: &Database,
    template: &Template,
    piece: PlacedPiece,
    preview: &mut StructurePreview,
    cancel: &impl Fn() -> bool,
) -> Result<(), String> {
    if preview.blocks.len() >= crate::structure_preview::MAX_SHAPE_BLOCKS {
        preview.complete = false;
        preview
            .notes
            .push("Block preview reached its memory limit; remaining geometry was omitted.".into());
        return Ok(());
    }
    let preserves_items = processor_is_empty(piece.processor.as_deref());
    if !processor_preserves_geometry(db, piece.processor.as_deref()) {
        preview.complete = false;
    }
    preview.pieces.push(Piece {
        template: Some(piece.template_id.clone()),
        bounds: piece.bounds,
    });
    for block in &template.blocks {
        if cancel() {
            return Err("Structure preview cancelled".into());
        }
        if preview.blocks.len() >= crate::structure_preview::MAX_SHAPE_BLOCKS {
            preview.complete = false;
            preview.notes.push(
                "Block preview reached its memory limit; remaining geometry was omitted.".into(),
            );
            break;
        }
        let local = rotate_local([block[0], block[1], block[2]], piece.rotation);
        let Some(name) = template.palette.get(block[3] as usize) else {
            continue;
        };
        let name = if name == "minecraft:jigsaw" {
            let final_state = template
                .jigsaws
                .iter()
                .find(|j| j.pos == [block[0], block[1], block[2]])
                .and_then(|j| j.final_state.as_deref())
                .unwrap_or("minecraft:air");
            let block_id = final_state.split(['[', '{']).next().unwrap_or(final_state);
            if block_id.ends_with(":air") || block_id == "minecraft:air" {
                continue;
            }
            block_id
        } else {
            name.as_str()
        };
        preview.blocks.push(ShapeBlock {
            pos: BlockPos {
                x: piece.origin.x + local[0],
                y: piece.origin.y + local[1],
                z: piece.origin.z + local[2],
            },
            block: name.to_owned(),
            color: block_color(name),
        });
    }
    for raw in &template.containers {
        let local = rotate_local(raw.pos, piece.rotation);
        let mut container = Container::new(
            BlockPos {
                x: piece.origin.x + local[0],
                y: piece.origin.y + local[1],
                z: piece.origin.z + local[2],
            },
            raw.block.clone(),
        );
        container.loot_table = raw.loot_table.clone();
        container.loot_seed = raw.loot_seed;
        container.items = raw
            .items
            .iter()
            .map(|item| ItemStack {
                id: item.id.clone(),
                count: item.count,
                slot: item.slot,
                description: String::new(),
            })
            .collect();
        container.contents_known =
            raw.loot_table.is_none() && !raw.items.is_empty() && preserves_items;
        preview.containers.push(container);
    }
    Ok(())
}

fn occupied_blocks(template: &Template, piece: &PlacedPiece) -> HashSet<BlockPos> {
    template
        .blocks
        .iter()
        .filter_map(|block| {
            let state = template.palette.get(block[3] as usize)?;
            if state == "minecraft:jigsaw"
                || state.ends_with(":air")
                || state.ends_with(":cave_air")
                || state.ends_with(":structure_void")
            {
                return None;
            }
            let p = rotate_local([block[0], block[1], block[2]], piece.rotation);
            Some(BlockPos {
                x: piece.origin.x + p[0],
                y: piece.origin.y + p[1],
                z: piece.origin.z + p[2],
            })
        })
        .collect()
}

fn intersects_occupied(
    template: &Template,
    piece: &PlacedPiece,
    occupied: &HashSet<BlockPos>,
) -> bool {
    template.blocks.iter().any(|block| {
        let Some(state) = template.palette.get(block[3] as usize) else {
            return false;
        };
        if state == "minecraft:jigsaw"
            || state.ends_with(":air")
            || state.ends_with(":cave_air")
            || state.ends_with(":structure_void")
        {
            return false;
        }
        let p = rotate_local([block[0], block[1], block[2]], piece.rotation);
        occupied.contains(&BlockPos {
            x: piece.origin.x + p[0],
            y: piece.origin.y + p[1],
            z: piece.origin.z + p[2],
        })
    })
}

fn processor_is_empty(id: Option<&str>) -> bool {
    id.is_none_or(|id| matches!(id, "minecraft:empty" | "minecraft:empty_processor_list"))
}

fn processor_preserves_geometry(db: &Database, id: Option<&str>) -> bool {
    let Some(id) = id else {
        return true;
    };
    processor_is_empty(Some(id)) || db.processors.get(id).is_some_and(|types| types.is_empty())
}

fn world_pos(origin: BlockPos, local: [i32; 3], rotation: i32) -> BlockPos {
    let p = rotate_local(local, rotation);
    BlockPos {
        x: origin.x + p[0],
        y: origin.y + p[1],
        z: origin.z + p[2],
    }
}

fn rotate_local(pos: [i32; 3], rotation: i32) -> [i32; 3] {
    match rotation.rem_euclid(4) {
        1 => [-pos[2], pos[1], pos[0]],
        2 => [-pos[0], pos[1], -pos[2]],
        3 => [pos[2], pos[1], -pos[0]],
        _ => pos,
    }
}

fn world_direction(orientation: &str, rotation: i32) -> [i32; 3] {
    let face = orientation.split('_').next().unwrap_or(orientation);
    let direction = match face {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "east" => [1, 0, 0],
        "west" => [-1, 0, 0],
        "up" => [0, 1, 0],
        "down" => [0, -1, 0],
        _ => [0, 0, 0],
    };
    rotate_local(direction, rotation)
}

fn world_top(orientation: &str, rotation: i32) -> [i32; 3] {
    let top = orientation.split('_').nth(1).unwrap_or("up");
    let direction = match top {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "east" => [1, 0, 0],
        "west" => [-1, 0, 0],
        "down" => [0, -1, 0],
        _ => [0, 1, 0],
    };
    rotate_local(direction, rotation)
}

fn union_bounds(a: StructureBounds, b: StructureBounds) -> StructureBounds {
    StructureBounds {
        min_x: a.min_x.min(b.min_x),
        max_x: a.max_x.max(b.max_x),
        min_y: a.min_y.min(b.min_y),
        max_y: a.max_y.max(b.max_y),
        min_z: a.min_z.min(b.min_z),
        max_z: a.max_z.max(b.max_z),
    }
}

fn block_color(id: &str) -> [u8; 3] {
    let path = id.rsplit(':').next().unwrap_or(id);
    if path.contains("water") || path.contains("ice") {
        [76, 123, 158]
    } else if path.contains("wood") || path.contains("log") || path.contains("planks") {
        [137, 100, 64]
    } else if path.contains("brick") || path.contains("stone") || path.contains("cobble") {
        [137, 134, 126]
    } else if path.contains("glass") {
        [126, 175, 179]
    } else if path.contains("chest") || path.contains("vessel") || path.contains("vase") {
        [182, 139, 82]
    } else {
        [115, 126, 119]
    }
}
