//! Surface-column approximation of TFC's pre-caldera magma rivulets.
//!
//! This follows `RivuletFeature`'s placement RNG and downhill branch traversal. The
//! sampler has no generated block states, so solidity and fluid checks use its terrain
//! and surface-water model; caves and fluids away from the surface are not represented.

use crate::{
    biome::ids::id,
    surface_water::{self, SEA_SURFACE_BLOCK_Y},
    vegetation::FeatureRandom,
    world::World,
};
use std::collections::{HashMap, HashSet, VecDeque};

type Pos = (i32, i32, i32);
type Column = (i32, i32);

const DIRECTIONS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
const MAX_QUEUED_BRANCHES: usize = 4096;

#[derive(Default)]
pub(super) struct Carving {
    /// New highest-solid Y after the source removes the surface block above magma.
    pub(super) ground: HashMap<Column, i32>,
    /// Highest exposed TFC magma block for each affected column.
    pub(super) magma: HashMap<Column, i32>,
}

/// Predict the block-height effects of the six placed magma rivulets around a feature.
/// Each configured placement's full random path runs before the next placement offset,
/// matching the lazy placed-feature stream used by Minecraft world generation.
pub(super) fn carve(world: &World, center: (i32, i32), rng: &mut FeatureRandom) -> Carving {
    let mut changes = Carving::default();
    for _ in 0..6 {
        let origin = (
            center.0.saturating_add(rng.next_int_bound(25) - 12),
            center.1.saturating_add(rng.next_int_bound(25) - 12),
        );
        carve_one(world, origin, center, rng, &mut changes);
    }
    changes
}

fn carve_one(
    world: &World,
    origin: Column,
    owner: Column,
    rng: &mut FeatureRandom,
    changes: &mut Carving,
) {
    let Some(start_y) = top_air(world, origin, &changes.ground) else {
        return;
    };
    let start = (origin.0, start_y, origin.1);
    if fluid_at(world, &changes.ground, (start.0, start.1 - 1, start.2)) {
        return;
    }

    // ChunkPos uses arithmetic right shift for negative coordinates.
    let chunk_x = origin.0 >> 4;
    let chunk_z = origin.1 >> 4;
    let bounds = (
        (chunk_x * 16 - 14, chunk_z * 16 - 14),
        (chunk_x * 16 + 29, chunk_z * 16 + 29),
    );
    let mut chosen = HashSet::<Pos>::new();
    let mut branches = VecDeque::from([start]);
    let mut main_branch = true;

    // The cap only protects against pathological repeated branches. Normal source paths
    // finish after a few dozen branches; source code itself has no explicit work cap.
    let mut processed = 0usize;
    while let Some(branch_start) = branches.pop_front() {
        processed += 1;
        if processed > MAX_QUEUED_BRANCHES {
            break;
        }
        let mut max_length = 5 + rng.next_int_bound(8);
        if main_branch {
            main_branch = false;
            max_length += 12;
        }
        let mut last = branch_start;
        for _ in 0..max_length {
            // Preserve source's `chosen.add(branchStartPos)` placement inside the loop.
            chosen.insert(branch_start);
            let mut selected: Option<((i32, i32), i32)> = None;
            let mut possible = 0;
            for (dx, dz) in DIRECTIONS {
                let x = last.0.saturating_add(dx);
                let z = last.2.saturating_add(dz);
                if x < bounds.0.0 || x > bounds.1.0 || z < bounds.0.1 || z > bounds.1.1 {
                    continue;
                }
                if fluid_at(world, &changes.ground, (x, last.1 - 1, z)) {
                    continue;
                }
                let Some(height) = top_air(world, (x, z), &changes.ground) else {
                    continue;
                };
                if height > last.1 || !solid_below(world, &changes.ground, (x, z), height) {
                    continue;
                }
                possible += 1;
                if selected.is_none() || rng.next_int_bound(possible) == 0 {
                    selected = Some(((x, z), height));
                }
            }
            if possible == 0 {
                break;
            }
            if possible > 1 && rng.next_int_bound(3) == 0 {
                branches.push_back(last);
            }
            let ((x, z), height) =
                selected.expect("positive candidate count has a selected direction");
            let next = (x, height, z);
            if chosen.contains(&next) || fluid_at(world, &changes.ground, (x, height - 1, z)) {
                break;
            }
            last = next;
            chosen.insert(last);
        }
    }

    // The feature replaces the surface block then puts magma two and three blocks below
    // the top-air position. The visible solid top consequently drops by one block.
    for (x, y, z) in chosen {
        // WorldGenRegion permits feature writes only in the owner chunk and its
        // eight neighbors, even when an offset rivulet's own box extends farther.
        if ((x >> 4) - (owner.0 >> 4)).abs() > 1 || ((z >> 4) - (owner.1 >> 4)).abs() > 1 {
            continue;
        }
        let column = (x, z);
        // `chosenPos` is top-air. The surface block at y-1 is replaced by the first
        // adjacent fluid, if any, otherwise air; configured magma occupies y-2/y-3.
        // Record a visible magma top only when that intervening block becomes air.
        let surface_block = y - 1;
        let fluid_cover = DIRECTIONS.iter().any(|(dx, dz)| {
            fluid_at(
                world,
                &changes.ground,
                (x.saturating_add(*dx), surface_block, z.saturating_add(*dz)),
            )
        });
        let new_ground = y - 2;
        changes.ground.insert(column, new_ground);
        if !fluid_cover {
            changes.magma.insert(column, new_ground);
        } else {
            changes.magma.remove(&column);
        }
    }
}

fn top_air(world: &World, column: Column, changes: &HashMap<Column, i32>) -> Option<i32> {
    let sampled = world.height(column.0, column.1)?;
    if !sampled.is_finite() {
        return None;
    }
    let base_ground = sampled.trunc() as i32;
    let ground = changes.get(&column).copied().unwrap_or(base_ground);
    // Only submerged columns have a water top at sea level. A river/lake label
    // can extend onto dry banks and terrain above the fluid plane.
    let biome = world.region.biome(column.0, column.1);
    if surface_water::is_surface_water(biome, Some(sampled)) {
        Some(SEA_SURFACE_BLOCK_Y + 1)
    } else {
        Some(ground.saturating_add(1))
    }
}

fn fluid_at(world: &World, changes: &HashMap<Column, i32>, pos: Pos) -> bool {
    let (x, y, z) = pos;
    if y > SEA_SURFACE_BLOCK_Y {
        return false;
    }
    let biome = world.region.biome(x, z);
    let Some(height) = world.height(x, z) else {
        return false;
    };
    let ground = changes
        .get(&(x, z))
        .copied()
        .unwrap_or(height.trunc() as i32);
    // Water-biome surface filling and sea-level aquifers are represented. Hot springs,
    // caves and other sub-surface fluids need chunk block data and are deliberately absent.
    surface_water::is_surface_water(biome, Some(height))
        && y > ground
        && (biome == id::OCEAN || y <= SEA_SURFACE_BLOCK_Y)
}

fn solid_below(
    world: &World,
    changes: &HashMap<Column, i32>,
    column: Column,
    top_air: i32,
) -> bool {
    let below = top_air - 1;
    !fluid_at(world, changes, (column.0, below, column.1))
}
