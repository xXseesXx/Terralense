//! On-demand structure geometry and inventories. All resource/save work runs in the engine.
pub mod loot;
pub mod saved;
pub mod templates;

use crate::model::{Marker, StructureBounds};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX_SHAPE_BLOCKS: usize = 160_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemStack {
    pub id: String,
    pub count: u32,
    pub slot: Option<u16>,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerInventory {
    pub uuid: String,
    pub items: Vec<ItemStack>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PossibleDrop {
    pub id: String,
    /// Count range for one selected entry, before unknown context-dependent modifiers.
    pub min: u32,
    pub max: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Container {
    pub pos: BlockPos,
    pub block: String,
    #[serde(default)]
    pub dimension: Option<String>,
    pub items: Vec<ItemStack>,
    pub contents_known: bool,
    pub loot_table: Option<String>,
    pub loot_seed: Option<i64>,
    pub lootr: bool,
    #[serde(default)]
    pub lootr_id: Option<String>,
    pub player_inventories: Vec<PlayerInventory>,
    pub possible_drops: Vec<PossibleDrop>,
    pub notes: Vec<String>,
}

impl Container {
    pub fn new(pos: BlockPos, block: String) -> Self {
        Self {
            pos,
            block,
            dimension: None,
            items: Vec::new(),
            contents_known: false,
            loot_table: None,
            loot_seed: None,
            lootr: false,
            lootr_id: None,
            player_inventories: Vec::new(),
            possible_drops: Vec::new(),
            notes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Piece {
    pub template: Option<String>,
    pub bounds: StructureBounds,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShapeBlock {
    pub pos: BlockPos,
    pub block: String,
    pub color: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewSource {
    Saved,
    SeedLayout,
    TemplateLayout,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructurePreview {
    pub structure_id: String,
    pub bounds: Option<StructureBounds>,
    pub source: PreviewSource,
    pub pieces: Vec<Piece>,
    /// Sparse visible structure blocks, not a fabricated filled bounding rectangle.
    pub blocks: Vec<ShapeBlock>,
    pub containers: Vec<Container>,
    pub notes: Vec<String>,
    pub complete: bool,
}

impl StructurePreview {
    pub fn empty(id: &str, source: PreviewSource) -> Self {
        Self {
            structure_id: id.into(),
            bounds: None,
            source,
            pieces: Vec::new(),
            blocks: Vec::new(),
            containers: Vec::new(),
            notes: Vec::new(),
            complete: true,
        }
    }
}

pub fn inspect(
    world: &mut crate::world::World,
    marker: &Marker,
    path: Option<&Path>,
    cancel: &impl Fn() -> bool,
) -> Result<StructurePreview, String> {
    if cancel() {
        return Err("Inspection cancelled".into());
    }
    let mut preview = if let Some(path) = path {
        match saved::read(path, marker, cancel)? {
            Some(preview) => preview,
            None => templates::predict(world, marker, cancel)?,
        }
    } else {
        templates::predict(world, marker, cancel)?
    };
    if preview.blocks.len() > MAX_SHAPE_BLOCKS {
        preview.blocks.truncate(MAX_SHAPE_BLOCKS);
        preview.complete = false;
        preview
            .notes
            .push("Block display limited for this large structure".into());
    }
    for container in &mut preview.containers {
        container.dimension = Some(world.dimension.id().to_owned());
    }
    let save_root = path.and_then(|path| match world.dimension {
        crate::dimension::Dimension::Overworld => Some(path),
        crate::dimension::Dimension::Beneath => path.parent(),
        crate::dimension::Dimension::Moon | crate::dimension::Dimension::Mars => {
            path.ancestors().nth(3)
        }
    });
    loot::enrich(&mut preview, save_root, cancel);
    Ok(preview)
}
