//! TFG surface-builder biome selection from the blended block-column weights.
//!
//! `TFGChunkHeightFiller.sampleColumnHeightAndBiome` partitions weighted biomes
//! into normal, shore and ocean groups. The largest member of each group is
//! retained; shore wins only when total shore weight exceeds 0.5, and ocean is
//! the fallback when there is no normal biome. The selected biome may then be
//! replaced by the river overlay in `TFGChunkNoiseFiller.updateLocalCaches`.
use crate::{biome::ids::BIOME_IDS, geology::blend_group};
use lru::LruCache;
use serde::Deserialize;
use std::{cell::RefCell, num::NonZeroUsize};
use std::{collections::HashMap, sync::OnceLock};

const SHORE_DATA: &str = include_str!("../../assets/data/tfg/biome-shores.json");

#[derive(Deserialize)]
struct ShoreData {
    shore: HashMap<String, (String, f64)>,
}

static BIOME_GROUPS: OnceLock<([bool; 109], [bool; 109])> = OnceLock::new();

type SlopeKey = (i64, [u64; 6], i32, i32);
thread_local! {
    static SLOPES: RefCell<LruCache<SlopeKey, [f64; 36]>> =
        RefCell::new(LruCache::new(NonZeroUsize::new(128).unwrap()));
}

fn biome_groups() -> &'static ([bool; 109], [bool; 109]) {
    BIOME_GROUPS.get_or_init(|| {
        let shore: ShoreData = serde_json::from_str(SHORE_DATA).expect("valid TFG shore data");
        // River shape and `hasRivers` are separate TFG biome properties.
        // In particular, several biomes declare a river profile but call
        // `noRivers()`, while some default-profile biomes retain rivers.
        let no_rivers = [
            "ocean",
            "ocean_reef",
            "deep_ocean",
            "deep_ocean_trench",
            "shore",
            "tidal_flats",
            "sea_stacks",
            "terrace_upper",
            "terrace_lower",
            "setback_cliffs",
            "coastal_dunes",
            "rocky_shores",
            "embayments",
            "lake",
            "mountain_lake",
            "old_mountain_lake",
            "oceanic_mountain_lake",
            "volcanic_mountain_lake",
            "volcanic_oceanic_mountain_lake",
            "plateau_lake",
            "subglacial_lake",
            "meltwater_lake",
        ];
        let names: Vec<_> = BIOME_IDS
            .iter()
            .map(|id| id.rsplit('/').next().unwrap_or(""))
            .collect();
        (
            std::array::from_fn(|i| shore.shore.contains_key(names[i])),
            std::array::from_fn(|i| !no_rivers.contains(&names[i])),
        )
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Selection {
    pub biome: u16,
    pub has_rivers: bool,
}

pub(super) fn select_biome(weights: &[f64; 109]) -> Selection {
    let (is_shore, has_rivers) = biome_groups();
    let mut normal = None;
    let mut shore = None;
    let mut ocean = None;
    let mut shore_weight = 0.0;
    for id in 0..109 {
        let weight = weights[id];
        if weight <= 0.0 {
            continue;
        }
        if is_shore[id] {
            shore_weight += weight;
            if shore.is_none_or(|(_, best)| weight > best) {
                shore = Some((id, weight));
            }
        } else if blend_group(id) == 1 {
            if ocean.is_none_or(|(_, best)| weight > best) {
                ocean = Some((id, weight));
            }
        } else if normal.is_none_or(|(_, best)| weight > best) {
            normal = Some((id, weight));
        }
    }

    // Preserve TFG's branch order: a dominant shore applies only above 0.5;
    // an empty normal selection falls through to the dominant ocean biome.
    let selected = if shore_weight > 0.5 {
        shore.or(normal).or(ocean)
    } else {
        normal.or(ocean)
    }
    .expect("TFG biome weights always select at least one biome");
    Selection {
        biome: selected.0 as u16,
        has_rivers: has_rivers[selected.0],
    }
}

/// Source's per-chunk 7×7 quart height samples converted into a 6×6 slope map,
/// then bilinearly sampled at the requested block coordinate.
pub(super) fn slope(
    region: &crate::region::Generator,
    seed: i64,
    block_x: i32,
    block_z: i32,
) -> Option<f64> {
    let (chunk_x, chunk_z) = (block_x.div_euclid(16), block_z.div_euclid(16));
    let key = (
        seed,
        [
            region.settings.temperature_scale.to_bits(),
            region.settings.temperature_constant.to_bits(),
            region.settings.rainfall_scale.to_bits(),
            region.settings.rainfall_constant.to_bits(),
            region.settings.continentalness.to_bits(),
            region.settings.grass_density.to_bits(),
        ],
        chunk_x,
        chunk_z,
    );
    SLOPES.with(|cache| {
        if cache.borrow_mut().get(&key).is_none() {
            let origin_x = chunk_x * 16;
            let origin_z = chunk_z * 16;
            let mut heights = [0i32; 49];
            for qx in 0..7 {
                for qz in 0..7 {
                    let x = origin_x + (qx as i32 - 1) * 4;
                    let z = origin_z + (qz as i32 - 1) * 4;
                    heights[qx + 7 * qz] =
                        crate::geology::surface_height(region, seed, x, z)?.trunc() as i32;
                }
            }
            let values = slope_grid(&heights);
            cache.borrow_mut().put(key, values);
        }
        let values = *cache.borrow_mut().get(&key)?;
        Some(interpolate_slope(
            &values,
            block_x.rem_euclid(16),
            block_z.rem_euclid(16),
        ))
    })
}

fn slope_grid(heights: &[i32; 49]) -> [f64; 36] {
    let mut values = [0.0; 36];
    for qx in 0..6 {
        for qz in 0..6 {
            let nw = f64::from(heights[qx + 7 * qz]);
            let ne = f64::from(heights[qx + 1 + 7 * qz]);
            let sw = f64::from(heights[qx + 7 * (qz + 1)]);
            let se = f64::from(heights[qx + 1 + 7 * (qz + 1)]);
            let center = (nw + ne + sw + se) * 0.25;
            values[qx + 6 * qz] = (nw - center).abs()
                + (ne - center).abs()
                + (sw - center).abs()
                + (se - center).abs();
        }
    }
    values
}

fn interpolate_slope(values: &[f64; 36], local_x: i32, local_z: i32) -> f64 {
    let offset_x = local_x + 2;
    let offset_z = local_z + 2;
    let cell_x = (offset_x >> 2) as usize;
    let cell_z = (offset_z >> 2) as usize;
    let dx = f64::from(offset_x & 3) * 0.25;
    let dz = f64::from(offset_z & 3) * 0.25;
    let at = |x, z| values[x + 6 * z];
    let interpolated = at(cell_x, cell_z) * (1.0 - dx) * (1.0 - dz)
        + at(cell_x + 1, cell_z) * dx * (1.0 - dz)
        + at(cell_x, cell_z + 1) * (1.0 - dx) * dz
        + at(cell_x + 1, cell_z + 1) * dx * dz;
    interpolated * 0.8_f32 as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::ids::id;

    #[test]
    fn selects_the_heaviest_normal_biome_and_keeps_first_ties() {
        let mut weights = [0.0; 109];
        weights[id::PLAINS as usize] = 0.4;
        weights[id::HILLS as usize] = 0.4;
        weights[id::DEEP_OCEAN as usize] = 0.2;
        let selected = select_biome(&weights);
        assert_eq!(selected.biome, id::PLAINS);
        assert!(selected.has_rivers);
    }

    #[test]
    fn shore_wins_only_when_its_combined_weight_exceeds_one_half() {
        let mut weights = [0.0; 109];
        weights[id::PLAINS as usize] = 0.5;
        weights[id::SHORE as usize] = 0.3;
        weights[id::TIDAL_FLATS as usize] = 0.2;
        assert_eq!(select_biome(&weights).biome, id::PLAINS);
        weights[id::SHORE as usize] = 0.31;
        weights[id::PLAINS as usize] = 0.49;
        assert_eq!(select_biome(&weights).biome, id::SHORE);
    }

    #[test]
    fn ocean_is_the_fallback_when_no_normal_or_shore_biome_is_present() {
        let mut weights = [0.0; 109];
        weights[id::OCEAN as usize] = 0.7;
        weights[id::DEEP_OCEAN as usize] = 0.3;
        assert_eq!(select_biome(&weights).biome, id::OCEAN);
    }

    #[test]
    fn source_slope_grid_matches_flat_and_planar_quart_samples() {
        assert_eq!(slope_grid(&[80; 49]), [0.0; 36]);
        let heights = std::array::from_fn(|i| {
            let x = (i % 7) as i32 - 1;
            let z = (i / 7) as i32 - 1;
            80 + 4 * x + 8 * z
        });
        let grid = slope_grid(&heights);
        assert_eq!(grid, [16.0; 36]);
        for z in 0..16 {
            for x in 0..16 {
                assert!((interpolate_slope(&grid, x, z) - 12.8).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn has_rivers_flag_is_separate_from_river_shape_and_shore_flags() {
        let (_, has_rivers) = biome_groups();
        assert!(!has_rivers[id::SHORE as usize]);
        assert!(!has_rivers[id::TIDAL_FLATS as usize]);
        assert!(!has_rivers[id::LAKE as usize]);
        assert!(has_rivers[id::ICE_SHEET_SHORE as usize]);
        assert!(has_rivers[id::PLAINS as usize]);
    }
}
