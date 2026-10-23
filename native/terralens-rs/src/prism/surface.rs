//! Exact, read-only block surface extraction from generated Minecraft Anvil chunks.
//!
//! This reads saved block states only. It never substitutes seed estimates for an
//! absent or incomplete chunk.
use super::{Nbt, decode_nbt_with_long_arrays};
use crate::model::StructureBounds;
use flate2::read::{GzDecoder, ZlibDecoder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const SECTOR: u64 = 4096;
const MAX_COMPRESSED: usize = 16 * 1024 * 1024;
const MAX_DECOMPRESSED: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SurfaceChunk {
    pub chunk_x: i32,
    pub chunk_z: i32,
    /// Chunk-local columns in z-major order: `z * 16 + x`.
    pub columns: Vec<SurfaceColumn>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SurfaceColumn {
    pub y: i32,
    pub block: String,
    pub tree: Option<String>,
    pub crop: Option<String>,
    /// A compact approximate display tint, not a sampled game texture.
    pub color: [u8; 3],
}

/// Read one generated chunk from `world/region/r.<rx>.<rz>.mca` and extract its
/// highest non-air saved block for each of its 256 columns. Returns `None` when the
/// region slot is empty, points to an incomplete record, or contains no block states.
pub fn read_surface_tile(
    world_path: &Path,
    chunk_x: i32,
    chunk_z: i32,
) -> Result<Option<SurfaceChunk>, String> {
    let Some(root) = read_chunk_nbt(world_path, chunk_x, chunk_z)? else {
        return Ok(None);
    };
    Ok(surface_chunk(&root, chunk_x, chunk_z))
}

/// Read a saved slice suitable for a downward view inside caves. For each column,
/// select the highest saved non-air block at or below `slice_y` whose upper neighbor
/// is air or fluid. This finds floors and exposed fluid surfaces while excluding roofs.
pub fn read_surface_slice(
    world_path: &Path,
    chunk_x: i32,
    chunk_z: i32,
    slice_y: i32,
) -> Result<Option<SurfaceChunk>, String> {
    let Some(root) = read_chunk_nbt(world_path, chunk_x, chunk_z)? else {
        return Ok(None);
    };
    Ok(surface_slice_chunk(&root, chunk_x, chunk_z, slice_y))
}

/// Read the highest saved non-air block in each column inside an inclusive vertical
/// band. Unlike the cave slice, this intentionally includes walls and ceilings in the
/// band, which supports regular-height cutaway maps.
pub fn read_surface_band(
    world_path: &Path,
    chunk_x: i32,
    chunk_z: i32,
    min_y: i32,
    max_y: i32,
) -> Result<Option<SurfaceChunk>, String> {
    let Some(root) = read_chunk_nbt(world_path, chunk_x, chunk_z)? else {
        return Ok(None);
    };
    Ok(surface_band_chunk(&root, chunk_x, chunk_z, min_y, max_y))
}

/// Clear columns outside a saved structure's horizontal bounds. Vertical selection
/// is performed by `read_surface_band` before this mask is applied.
pub fn clip_surface_chunk(mut chunk: SurfaceChunk, bounds: StructureBounds) -> SurfaceChunk {
    for (index, column) in chunk.columns.iter_mut().enumerate() {
        let x = chunk.chunk_x.saturating_mul(16) + (index % 16) as i32;
        let z = chunk.chunk_z.saturating_mul(16) + (index / 16) as i32;
        if !bounds.contains_column(x, z) {
            *column = empty_column();
        }
    }
    chunk
}

/// Read the 4×4 biome sample grid at the quart Y containing `y`, in z-major order.
/// Returns `None` for missing or incomplete chunks and `Some` with 16 names otherwise.
pub fn read_biome_chunk(
    world_path: &Path,
    chunk_x: i32,
    chunk_z: i32,
    y: i32,
) -> Result<Option<Vec<String>>, String> {
    let Some(root) = read_chunk_nbt(world_path, chunk_x, chunk_z)? else {
        return Ok(None);
    };
    Ok(biome_chunk(&root, y))
}

fn biome_chunk(root: &Nbt, y: i32) -> Option<Vec<String>> {
    let data = root.get("Level").unwrap_or(root);
    let status = data.get("Status").and_then(Nbt::string);
    if !matches!(status, Some("minecraft:full" | "full")) {
        return None;
    }
    let Some(sections) = data
        .get("sections")
        .or_else(|| data.get("Sections"))
        .and_then(Nbt::list)
    else {
        return None;
    };
    let target_section = y.div_euclid(16);
    let local_y = (y.rem_euclid(16) / 4) as usize;
    for section in sections {
        if section.get("Y").and_then(Nbt::i64) != Some(target_section as i64) {
            continue;
        }
        let Some(biomes) = section.get("biomes") else {
            return None;
        };
        let Some(palette) = biomes.get("palette").and_then(Nbt::list) else {
            return None;
        };
        let names = palette.iter().map(Nbt::string).collect::<Option<Vec<_>>>();
        let Some(names) = names else { return None };
        if names.is_empty() {
            return None;
        }
        let values = if names.len() == 1 {
            vec![0usize; 64]
        } else {
            let Some(packed) = biomes.get("data").and_then(Nbt::long_array) else {
                return None;
            };
            let bits = (usize::BITS - (names.len() - 1).leading_zeros()).max(1) as usize;
            let per_long = 64 / bits;
            if packed.len() < 64usize.div_ceil(per_long) {
                return None;
            }
            let mask = (1u64 << bits) - 1;
            (0..64)
                .map(|i| ((packed[i / per_long] as u64 >> ((i % per_long) * bits)) & mask) as usize)
                .collect()
        };
        let mut out = Vec::with_capacity(16);
        for z in 0..4 {
            for x in 0..4 {
                let index = (local_y * 4 + z) * 4 + x;
                let Some(name) = names.get(values[index]) else {
                    return None;
                };
                out.push((*name).to_owned());
            }
        }
        return Some(out);
    }
    None
}

fn read_chunk_nbt(world_path: &Path, chunk_x: i32, chunk_z: i32) -> Result<Option<Nbt>, String> {
    let (region_x, region_z, slot) = region_address(chunk_x, chunk_z);
    let path = world_path
        .join("region")
        .join(format!("r.{region_x}.{region_z}.mca"));
    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut header = [0u8; 8192];
    if file.read_exact(&mut header).is_err() {
        return Ok(None);
    }
    let entry = &header[slot * 4..slot * 4 + 4];
    let sector = ((entry[0] as u64) << 16) | ((entry[1] as u64) << 8) | entry[2] as u64;
    let sectors = entry[3] as usize;
    if sector < 2 || sectors == 0 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(sector * SECTOR))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut record_head = [0u8; 5];
    if file.read_exact(&mut record_head).is_err() {
        return Ok(None);
    }
    let len = u32::from_be_bytes(record_head[..4].try_into().unwrap()) as usize;
    if len < 1 || len > MAX_COMPRESSED || len + 4 > sectors * SECTOR as usize {
        return Ok(None);
    }
    let mut compressed = vec![0; len - 1];
    if file.read_exact(&mut compressed).is_err() {
        return Ok(None);
    }
    let mut decoded = Vec::with_capacity((len * 3).min(MAX_DECOMPRESSED));
    let result = match record_head[4] {
        1 => GzDecoder::new(compressed.as_slice())
            .take((MAX_DECOMPRESSED + 1) as u64)
            .read_to_end(&mut decoded),
        2 => ZlibDecoder::new(compressed.as_slice())
            .take((MAX_DECOMPRESSED + 1) as u64)
            .read_to_end(&mut decoded),
        3 => {
            decoded.extend_from_slice(&compressed);
            Ok(decoded.len())
        }
        _ => return Ok(None),
    };
    if result.is_err() || decoded.len() > MAX_DECOMPRESSED {
        return Ok(None);
    }
    let root =
        decode_nbt_with_long_arrays(&decoded).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(root))
}

fn surface_slice_chunk(
    root: &Nbt,
    chunk_x: i32,
    chunk_z: i32,
    slice_y: i32,
) -> Option<SurfaceChunk> {
    surface_projection(root, chunk_x, chunk_z, Some(slice_y), None)
}

fn surface_band_chunk(
    root: &Nbt,
    chunk_x: i32,
    chunk_z: i32,
    min_y: i32,
    max_y: i32,
) -> Option<SurfaceChunk> {
    if min_y > max_y {
        return None;
    }
    surface_projection(root, chunk_x, chunk_z, None, Some((min_y, max_y)))
}

fn surface_projection(
    root: &Nbt,
    chunk_x: i32,
    chunk_z: i32,
    slice_y: Option<i32>,
    band: Option<(i32, i32)>,
) -> Option<SurfaceChunk> {
    let data = root.get("Level").unwrap_or(root);
    if !matches!(
        data.get("Status").and_then(Nbt::string),
        Some("minecraft:full" | "full")
    ) {
        return None;
    }
    let sections = data
        .get("sections")
        .or_else(|| data.get("Sections"))?
        .list()?;
    let mut ordered = sections.iter().collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|section| section.get("Y").and_then(Nbt::i64).unwrap_or(i64::MIN));
    let mut names_by_id = vec!["minecraft:air".to_owned()];
    let mut name_ids = HashMap::from([("minecraft:air".to_owned(), 0u32)]);
    let mut best: Vec<Option<(i32, u32)>> = vec![None; 256];
    let mut above = vec![0u32; 256];
    let mut above_section_y: Option<i32> = None;
    for section in ordered.into_iter().rev() {
        let section_y = section.get("Y")?.i64()? as i32;
        if above_section_y.is_some_and(|higher_y| higher_y != section_y + 1) {
            above.fill(0);
        }
        let (palette, packed) = if let Some(states) = section.get("block_states") {
            (
                states.get("palette")?.list()?,
                states.get("data").and_then(Nbt::long_array),
            )
        } else {
            (
                section.get("Palette")?.list()?,
                section.get("BlockStates").and_then(Nbt::long_array),
            )
        };
        if palette.is_empty() {
            above_section_y = Some(section_y);
            continue;
        }
        let names = palette.iter().map(block_name).collect::<Option<Vec<_>>>()?;
        let palette_ids = names
            .iter()
            .map(|name| {
                if let Some(id) = name_ids.get(name) {
                    *id
                } else {
                    let id = names_by_id.len() as u32;
                    names_by_id.push(name.clone());
                    name_ids.insert(name.clone(), id);
                    id
                }
            })
            .collect::<Vec<_>>();
        let packed = if names.len() == 1 {
            None
        } else {
            Some(packed?)
        };
        let (bits, per_long, mask) = if names.len() == 1 {
            (0usize, 0usize, 0u64)
        } else {
            let bits = (usize::BITS - (names.len() - 1).leading_zeros()).max(4) as usize;
            if bits > 32 {
                return None;
            }
            let per_long = 64 / bits;
            if packed
                .as_ref()
                .is_none_or(|data| data.len() < 4096usize.div_ceil(per_long))
            {
                return None;
            }
            (bits, per_long, (1u64 << bits) - 1)
        };
        for dy in (0..16usize).rev() {
            let y = section_y * 16 + dy as i32;
            for z in 0..16usize {
                for x in 0..16usize {
                    let column = z * 16 + x;
                    let name_index = if names.len() == 1 {
                        0
                    } else {
                        let index = (dy * 16 + z) * 16 + x;
                        let word = packed.as_ref().unwrap()[index / per_long] as u64;
                        ((word >> ((index % per_long) * bits)) & mask) as usize
                    };
                    let name_id = *palette_ids.get(name_index)?;
                    let name = &names_by_id[name_id as usize];
                    let above_name = &names_by_id[above[column] as usize];
                    let in_view = slice_y.is_some_and(|slice_y| y <= slice_y)
                        || band.is_some_and(|(min_y, max_y)| (min_y..=max_y).contains(&y));
                    let exposed = band.is_some() || is_air(above_name) || is_fluid(above_name);
                    if in_view
                        && best[column].is_none()
                        && !is_air(name)
                        && (band.is_some() || exposed)
                    {
                        best[column] = Some((y, name_id));
                    }
                    above[column] = name_id;
                }
            }
        }
        above_section_y = Some(section_y);
    }
    let columns = best
        .into_iter()
        .map(|cell| {
            cell.map(|(y, block_id)| make_column(y, names_by_id[block_id as usize].clone()))
                .unwrap_or_else(empty_column)
        })
        .collect::<Vec<_>>();
    columns
        .iter()
        .any(|c| c.y != i32::MIN)
        .then_some(SurfaceChunk {
            chunk_x,
            chunk_z,
            columns,
        })
}

fn is_fluid(block: &str) -> bool {
    block.ends_with(":water")
        || block.ends_with(":lava")
        || block.ends_with("_water")
        || block.ends_with("_lava")
}
fn empty_column() -> SurfaceColumn {
    SurfaceColumn {
        y: i32::MIN,
        block: String::new(),
        tree: None,
        crop: None,
        color: [0; 3],
    }
}
fn make_column(y: i32, block: String) -> SurfaceColumn {
    let tree = tree_species(&block);
    let crop = is_crop(&block).then(|| block.clone());
    let color = tint(&block);
    SurfaceColumn {
        y,
        block,
        tree,
        crop,
        color,
    }
}

fn surface_chunk(root: &Nbt, chunk_x: i32, chunk_z: i32) -> Option<SurfaceChunk> {
    let data = root.get("Level").unwrap_or(root);
    let status = data.get("Status")?.string()?;
    if status != "minecraft:full" && status != "full" {
        return None;
    }
    let sections = data
        .get("sections")
        .or_else(|| data.get("Sections"))?
        .list()?;
    let mut best: Vec<Option<(i32, String)>> = vec![None; 256];
    for section in sections {
        let section_y = section.get("Y")?.i64()? as i32;
        let (palette, packed) = if let Some(states) = section.get("block_states") {
            (
                states.get("palette")?.list()?,
                states.get("data").and_then(Nbt::long_array),
            )
        } else {
            (
                section.get("Palette")?.list()?,
                section.get("BlockStates").and_then(Nbt::long_array),
            )
        };
        if palette.is_empty() {
            continue;
        }
        let names = palette.iter().map(block_name).collect::<Option<Vec<_>>>()?;
        if names.len() == 1 {
            let name = names[0].clone();
            if !is_air(&name) {
                let y = section_y * 16 + 15;
                for item in &mut best {
                    if item.as_ref().is_none_or(|(old, _)| y > *old) {
                        *item = Some((y, name.clone()));
                    }
                }
            }
            continue;
        }
        let packed = packed?;
        let bits = (usize::BITS - (names.len() - 1).leading_zeros()).max(4) as usize;
        if bits > 32 {
            continue;
        }
        let per_long = 64 / bits;
        let mask = (1u64 << bits) - 1;
        if packed.len() < 4096usize.div_ceil(per_long) {
            continue;
        }
        for y in 0..16usize {
            for z in 0..16usize {
                for x in 0..16usize {
                    let index = (y * 16 + z) * 16 + x;
                    let word = packed[index / per_long] as u64;
                    let palette_index = ((word >> ((index % per_long) * bits)) & mask) as usize;
                    let Some(name) = names.get(palette_index) else {
                        continue;
                    };
                    if is_air(name) {
                        continue;
                    }
                    let column = z * 16 + x;
                    let block_y = section_y * 16 + y as i32;
                    if best[column]
                        .as_ref()
                        .is_none_or(|(old_y, _)| block_y > *old_y)
                    {
                        best[column] = Some((block_y, name.clone()));
                    }
                }
            }
        }
    }
    if best.iter().all(Option::is_none) {
        return None;
    }
    Some(SurfaceChunk {
        chunk_x,
        chunk_z,
        columns: best
            .into_iter()
            .map(|cell| {
                if let Some((y, block)) = cell {
                    let tree = tree_species(&block);
                    let crop = is_crop(&block).then(|| block.clone());
                    let color = tint(&block);
                    SurfaceColumn {
                        y,
                        block,
                        tree,
                        crop,
                        color,
                    }
                } else {
                    SurfaceColumn {
                        y: i32::MIN,
                        block: String::new(),
                        tree: None,
                        crop: None,
                        color: [0; 3],
                    }
                }
            })
            .collect(),
    })
}

fn block_name(tag: &Nbt) -> Option<String> {
    tag.get("Name")?.string().map(str::to_owned)
}
fn is_air(block: &str) -> bool {
    matches!(
        block.rsplit(':').next().unwrap_or(block),
        "air" | "cave_air" | "void_air"
    )
}
fn tree_species(block: &str) -> Option<String> {
    let path = block.rsplit_once(':').map_or(block, |(_, p)| p);
    if path.starts_with("wood/fallen_leaves/") || path.contains("/fallen_leaves/") {
        return None;
    }
    let species = if let Some((before, species)) = path.rsplit_once("/leaves/") {
        if before.ends_with("wood") {
            species
        } else {
            path.rsplit('/').next().unwrap_or(path)
        }
    } else {
        let leaf = path.rsplit('/').next().unwrap_or(path);
        leaf.strip_suffix("_leaves")
            .or_else(|| leaf.strip_suffix("_leaf"))?
    };
    if species.is_empty() || species == "azalea" || species == "flowering_azalea" {
        return None;
    }
    Some(species.replace('_', " "))
}
fn is_crop(block: &str) -> bool {
    let path = block.rsplit_once(':').map_or(block, |(_, p)| p);
    if path.contains("crop/") || path.starts_with("crop/") {
        return true;
    }
    let leaf = path.rsplit('/').next().unwrap_or(path);
    [
        "crop",
        "wheat",
        "carrot",
        "potato",
        "beetroot",
        "rice",
        "maize",
        "corn",
        "tomato",
        "onion",
        "cabbage",
        "soybean",
        "sugarcane",
        "berry_bush",
        "grape",
    ]
    .iter()
    .any(|needle| leaf.contains(needle))
}
fn tint(block: &str) -> [u8; 3] {
    let path = block.rsplit_once(':').map_or(block, |(_, p)| p);
    let s = path.rsplit('/').next().unwrap_or(path);
    if s.contains("lava") {
        [226, 91, 30]
    } else if s.contains("glowstone") || s.contains("shroomlight") {
        [226, 190, 103]
    } else if s.contains("soul_sand") || s.contains("soul_soil") || s.contains("soul_fire") {
        [93, 112, 119]
    } else if s.contains("crimson") || s.contains("nether_wart") {
        [139, 56, 86]
    } else if s.contains("warped") {
        [54, 132, 128]
    } else if s.contains("basalt") || s.contains("blackstone") || s.contains("deepslate") {
        [72, 70, 79]
    } else if s.contains("netherrack") || s.contains("nether_brick") {
        [133, 56, 55]
    } else if s.contains("salt") {
        [220, 212, 190]
    } else if s == "water"
        || s.ends_with("_water")
        || matches!(s, "ice" | "packed_ice" | "blue_ice" | "frosted_ice")
    {
        [76, 137, 176]
    } else if s.contains("pumice") {
        [171, 164, 151]
    } else if path.contains("leaves")
        || path.contains("/crop/")
        || s.contains("leaves")
        || s.contains("grass")
        || s.contains("fern")
        || s.contains("crop")
    {
        [99, 145, 67]
    } else if s.contains("sand") || s.contains("gravel") {
        [194, 177, 128]
    } else if s.contains("snow") {
        [225, 232, 236]
    } else if s.contains("stone") || s.contains("rock") {
        [119, 116, 109]
    } else if s.contains("log") || s.contains("wood") || s.contains("planks") {
        [132, 96, 62]
    } else if s.contains("dirt") || s.contains("soil") || s.contains("mud") {
        [126, 93, 67]
    } else {
        [133, 128, 113]
    }
}

impl Nbt {
    fn long_array(&self) -> Option<&[i64]> {
        if let Self::LongArray(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

fn region_address(chunk_x: i32, chunk_z: i32) -> (i32, i32, usize) {
    let slot = chunk_x.rem_euclid(32) as usize + chunk_z.rem_euclid(32) as usize * 32;
    (chunk_x.div_euclid(32), chunk_z.div_euclid(32), slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn block(name: &str) -> Nbt {
        Nbt::Compound(HashMap::from([("Name".into(), Nbt::String(name.into()))]))
    }
    fn section(y: i64, palette: Vec<Nbt>, data: Option<Vec<i64>>) -> Nbt {
        let states = Nbt::Compound(HashMap::from([
            ("palette".into(), Nbt::List(palette)),
            (
                "data".into(),
                data.map(Nbt::LongArray).unwrap_or(Nbt::Other),
            ),
        ]));
        Nbt::Compound(HashMap::from([
            ("Y".into(), Nbt::Number(y as f64)),
            ("block_states".into(), states),
        ]))
    }
    fn full_chunk(sections: Vec<Nbt>) -> Nbt {
        Nbt::Compound(HashMap::from([
            ("Status".into(), Nbt::String("minecraft:full".into())),
            ("sections".into(), Nbt::List(sections)),
        ]))
    }

    fn packed_section_with_blocks(y: i64, values: &[(usize, usize)]) -> Nbt {
        let mut packed = vec![0u64; 4096 / 16];
        for (index, palette) in values {
            packed[index / 16] |= (*palette as u64) << ((index % 16) * 4);
        }
        section(
            y,
            vec![
                block("minecraft:air"),
                block("minecraft:netherrack"),
                block("minecraft:basalt"),
            ],
            Some(packed.into_iter().map(|v| v as i64).collect()),
        )
    }

    #[test]
    fn structure_cutaway_masks_columns_outside_the_saved_structure_bounds() {
        let chunk = surface_chunk(
            &full_chunk(vec![section(0, vec![block("minecraft:stone")], None)]),
            -1,
            0,
        )
        .unwrap();
        let clipped = clip_surface_chunk(
            chunk,
            StructureBounds {
                min_x: -16,
                min_y: 0,
                min_z: 0,
                max_x: -9,
                max_y: 15,
                max_z: 7,
            },
        );
        assert_eq!(clipped.columns[0].block, "minecraft:stone");
        assert_eq!(clipped.columns[7].block, "minecraft:stone");
        assert!(clipped.columns[8].block.is_empty());
        assert_eq!(clipped.columns[8].y, i32::MIN);
        assert!(clipped.columns[8 * 16].block.is_empty());
        assert_eq!(clipped.columns[7 * 16 + 7].block, "minecraft:stone");
    }

    #[test]
    fn cave_slice_selects_exposed_floor_and_ignores_roof() {
        let cell = |y: usize, x: usize, z: usize| (y * 256 + z * 16 + x, 1);
        let root = full_chunk(vec![packed_section_with_blocks(
            0,
            &[cell(4, 2, 3), cell(12, 2, 3)],
        )]);
        let chunk = surface_slice_chunk(&root, -33, -1, 10).unwrap();
        let floor = &chunk.columns[3 * 16 + 2];
        assert_eq!(floor.y, 4);
        assert_eq!(floor.block, "minecraft:netherrack");
        assert!(chunk.columns.iter().filter(|c| c.y != i32::MIN).count() == 1);
    }

    #[test]
    fn band_projection_crosses_sections_and_keeps_only_highest_block_within_inclusive_band() {
        let cell = |y: usize, x: usize, z: usize| (y * 256 + z * 16 + x, 1);
        let lower = packed_section_with_blocks(1, &[cell(3, 4, 5), cell(12, 4, 5)]);
        let upper = packed_section_with_blocks(2, &[cell(2, 4, 5), cell(12, 4, 5), cell(14, 4, 5)]);
        let chunk = surface_band_chunk(
            &full_chunk(vec![lower.clone(), upper.clone()]),
            0,
            0,
            28,
            45,
        )
        .unwrap();
        let column = &chunk.columns[5 * 16 + 4];
        assert_eq!(column.y, 44);
        assert_eq!(column.block, "minecraft:netherrack");
        assert_eq!(
            chunk
                .columns
                .iter()
                .filter(|column| column.y != i32::MIN)
                .count(),
            1
        );
        let roof_band = surface_band_chunk(
            &full_chunk(vec![lower.clone(), upper.clone()]),
            0,
            0,
            46,
            60,
        )
        .unwrap();
        assert_eq!(roof_band.columns[5 * 16 + 4].y, 46);
        assert!(
            surface_band_chunk(
                &full_chunk(vec![lower.clone(), upper.clone()]),
                0,
                0,
                47,
                60
            )
            .is_none()
        );
        assert!(surface_band_chunk(&full_chunk(vec![lower, upper]), 0, 0, 60, 46).is_none());
    }

    #[test]
    fn biome_palette_reads_negative_section_at_y_and_keeps_z_major_order() {
        let mut packed = vec![0u64; 1];
        // Biome palette index 1 at quart Y=3, z=2, x=1 (linear section biome index 57).
        let i = (3 * 4 + 2) * 4 + 1;
        packed[i / 64] |= 1u64 << (i % 64);
        let biome = Nbt::Compound(HashMap::from([
            (
                "palette".into(),
                Nbt::List(vec![
                    Nbt::String("minecraft:basalt_deltas".into()),
                    Nbt::String("minecraft:nether_wastes".into()),
                ]),
            ),
            (
                "data".into(),
                Nbt::LongArray(packed.into_iter().map(|v| v as i64).collect()),
            ),
        ]));
        let sec = Nbt::Compound(HashMap::from([
            ("Y".into(), Nbt::Number(-1.)),
            ("biomes".into(), biome),
        ]));
        let root = full_chunk(vec![sec]);
        let out = biome_chunk(&root, -1).unwrap();
        assert_eq!(out.len(), 16);
        assert_eq!(out[2 * 4 + 1], "minecraft:nether_wastes");
        assert_eq!(out[0], "minecraft:basalt_deltas");
        assert_eq!(region_address(-33, -1), (-2, -1, 31 * 32 + 31));
    }

    #[test]
    fn packed_palette_uses_non_crossing_long_layout_and_finds_tree_and_crop_surface() {
        let mut words = vec![0u64; 4096usize.div_ceil(16)];
        // 3 palette values require 4 bits, 16 entries per long. Set (x=2,z=1,y=5) crop.
        let at = (5 * 16 + 1) * 16 + 2;
        words[at / 16] |= 2u64 << ((at % 16) * 4);
        let mut above = vec![0u64; 4096 / 16];
        // Tree leaves at column (3,4), y=9 in a second section.
        let at = (9 * 16 + 4) * 16 + 3;
        above[at / 16] |= 1u64 << ((at % 16) * 4);
        let root = full_chunk(vec![
            section(
                0,
                vec![
                    block("minecraft:air"),
                    block("minecraft:oak_leaves"),
                    block("tfc:crop/maize"),
                ],
                Some(words.into_iter().map(|v| v as i64).collect()),
            ),
            section(
                0,
                vec![block("minecraft:air"), block("minecraft:oak_leaves")],
                Some(above.into_iter().map(|v| v as i64).collect()),
            ),
        ]);
        let chunk = surface_chunk(&root, 0, 0).unwrap();
        assert_eq!(chunk.columns.len(), 256);
        let crop = &chunk.columns[1 * 16 + 2];
        assert_eq!((crop.y, crop.crop.as_deref()), (5, Some("tfc:crop/maize")));
        let leaves = &chunk.columns[4 * 16 + 3];
        assert_eq!((leaves.y, leaves.tree.as_deref()), (9, Some("oak")));
    }

    #[test]
    fn uniform_palette_handles_negative_section_y_and_air_only_is_missing_surface() {
        let root = full_chunk(vec![
            section(-1, vec![block("tfc:rock/granite")], None),
            section(0, vec![block("minecraft:air")], None),
        ]);
        let chunk = surface_chunk(&root, -1, -1).unwrap();
        assert!(
            chunk
                .columns
                .iter()
                .all(|c| c.y == -1 && c.block == "tfc:rock/granite")
        );
        let empty = full_chunk(vec![section(0, vec![block("minecraft:air")], None)]);
        assert!(surface_chunk(&empty, 0, 0).is_none());
    }

    #[test]
    fn water_leaf_and_tfc_crop_surface_metadata_is_preserved() {
        let root = full_chunk(vec![
            section(-1, vec![block("minecraft:water")], None),
            section(0, vec![block("tfc:wood/leaves/oak")], None),
        ]);
        let chunk = surface_chunk(&root, 0, 0).unwrap();
        assert_eq!(chunk.columns[0].block, "tfc:wood/leaves/oak");
        assert_eq!(chunk.columns[0].tree.as_deref(), Some("oak"));
        assert_eq!(chunk.columns[0].color, [99, 145, 67]);
        let water_only = surface_chunk(
            &full_chunk(vec![section(0, vec![block("minecraft:water")], None)]),
            0,
            0,
        )
        .unwrap();
        assert_eq!(water_only.columns[0].block, "minecraft:water");
        assert_eq!(water_only.columns[0].color, [76, 137, 176]);
        assert_eq!(tint("minecraft:lava"), [226, 91, 30]);
        assert_ne!(tint("minecraft:crimson_stem"), tint("minecraft:oak_leaves"));
        assert!(is_crop("tfc:dead_crop/maize"));
        assert!(tree_species("tfc:wood/fallen_leaves/oak").is_none());
        assert_eq!(
            tree_species("tfc:plant/lavacado_leaves").as_deref(),
            Some("lavacado")
        );
    }

    #[test]
    fn negative_chunk_coordinates_select_euclidean_region_slot() {
        assert_eq!(region_address(-1, -33), (-1, -2, 1023));
        assert_eq!(region_address(-32, -64), (-1, -2, 0));
        assert_eq!(region_address(32, 31), (1, 0, 31 * 32));
    }

    #[test]
    fn palette_with_seventeen_states_uses_non_crossing_five_bit_words() {
        let mut palette = (0..16)
            .map(|n| block(&format!("minecraft:stone_{n}")))
            .collect::<Vec<_>>();
        palette.push(block("minecraft:diamond_block"));
        let bits = 5usize;
        let per_long = 64 / bits;
        let mut words = vec![0i64; 4096usize.div_ceil(per_long)];
        for index in [15 * 256 + 11, 15 * 256 + 12] {
            words[index / per_long] |= (16u64 << ((index % per_long) * bits)) as i64;
        }
        let chunk =
            surface_chunk(&full_chunk(vec![section(0, palette, Some(words))]), 0, 0).unwrap();
        assert_eq!(chunk.columns[11].block, "minecraft:diamond_block");
        assert_eq!(chunk.columns[12].block, "minecraft:diamond_block");
    }

    #[test]
    fn incomplete_status_and_short_packed_arrays_are_never_reported_as_surface() {
        let incomplete = Nbt::Compound(HashMap::from([
            ("Status".into(), Nbt::String("minecraft:features".into())),
            (
                "sections".into(),
                Nbt::List(vec![section(-2, vec![block("minecraft:stone")], None)]),
            ),
        ]));
        assert!(surface_chunk(&incomplete, 0, 0).is_none());
        let mut palette = vec![block("minecraft:air")];
        palette.push(block("minecraft:stone"));
        let malformed = full_chunk(vec![section(-2, palette, Some(vec![0]))]);
        assert!(surface_chunk(&malformed, 0, 0).is_none());
        assert!(tree_species("tfc:wood/leaves/oak").as_deref() == Some("oak"));
        assert!(is_crop("tfc:crop/maize"));
    }

    #[test]
    fn oversized_long_array_is_rejected_by_nbt_reader() {
        let mut bytes = vec![10, 0, 0, 12, 0, 4, b'd', b'a', b't', b'a'];
        bytes.extend(((MAX_DECOMPRESSED / 8 + 1) as i32).to_be_bytes());
        bytes.push(0);
        assert!(decode_nbt_with_long_arrays(&bytes).is_err());
    }

    #[test]
    fn normal_nbt_reader_skips_long_array_payload_surface_reader_materializes() {
        let mut bytes = vec![10, 0, 0, 12, 0, 1, b'a'];
        bytes.extend(2i32.to_be_bytes());
        bytes.extend(7i64.to_be_bytes());
        bytes.extend((-9i64).to_be_bytes());
        bytes.push(0);
        let ordinary = super::super::decode_nbt(&bytes).unwrap();
        assert!(matches!(ordinary.get("a"), Some(Nbt::Other)));
        let surface = decode_nbt_with_long_arrays(&bytes).unwrap();
        assert!(matches!(surface.get("a"), Some(Nbt::LongArray(v)) if v == &[7, -9]));
    }
}
