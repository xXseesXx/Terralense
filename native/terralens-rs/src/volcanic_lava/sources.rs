//! Exposed fluid from TFG's post-caldera fissure/spout features.
//!
//! Fissure carving and spout placement follow their pinned TFC/TFG source RNG. The
//! preview only stores exposed fluid surfaces; it does not model underground block
//! replacement, scheduled fluid ticks, or spreading after chunk generation.

use crate::{vegetation::FeatureRandom, world::World};

const DIRECTIONS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
const MIN_FISSURE_Y: i32 = -48;

/// Adds exposed lava predicted from the random fissure selector and lava spout.
///
/// `ground_at` lazily samples highest solid block Y (including previous rivulet cuts);
/// `lava` contains pre-existing exposed pool tops.
/// The feature paths are bounded by their source configs (fissures have at most three
/// runs, each fewer than 24 pieces; the spout affects a 5×5 surface footprint).
pub(super) fn add(
    _world: &World,
    center: (i32, i32),
    min: (i32, i32),
    side: usize,
    ground_at: &mut impl FnMut(i32, i32) -> Option<i32>,
    lava: &mut [Option<i32>],
    random: &mut FeatureRandom,
) {
    if side < 3 || lava.len() < side.saturating_mul(side) {
        return;
    }

    // random_volcano_fissure first offsets by [-4, 4] in X/Z, then its simple selector
    // draws from four inactive TFC ore fissures and four TFG lava fissures. The TFC
    // VolcanoPlacement rejects the legacy volcanic flag in TFG cinder-cone biomes.
    let _offset_x = random.next_int_bound(9) - 4;
    let _offset_z = random.next_int_bound(9) - 4;
    if random.next_int_bound(8) >= 4 {
        add_fissures(center, min, side, ground_at, lava, random);
    }

    // minecraft:rarity_filter(chance=2) consumes nextFloat even when it rejects.
    if random.next_float() >= 0.5 {
        return;
    }
    // The cinder-cone center modifier is deterministic. Uniform absolute Y [-50,-40].
    let source_y = -50 + random.next_int_bound(11);
    let size = 12 + random.next_int_bound(5);
    let Some(surface_y) = surface_height(center, min, side, ground_at, lava) else {
        return;
    };
    if source_y >= surface_y {
        return;
    }
    // EncasedSpoutFeature's chance=1 always passes, but still consumes nextFloat.
    if random.next_float() > 1.0 {
        return;
    }
    let spring_top = surface_y + 1 + random.next_int_bound(2);

    // The configured sphere is centered around Y -40..-50 and cannot reach the Earth
    // surface in the supported cinder-cone terrain. Its size draw above still matches
    // source RNG. The feature's surface pipe emits fluid at the center at every level;
    // at WORLD_SURFACE it also emits a cross, diagonal corners, and the radius-2 cross.
    let _radius = (size + 1) / 2;
    for y in surface_y..=spring_top {
        record_exposed(lava, center, min, side, ground_at, y);
        if y <= surface_y {
            for (dx, dz) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, -1),
                (-1, 1),
                (2, 0),
                (-2, 0),
                (0, 2),
                (0, -2),
            ] {
                record_exposed(
                    lava,
                    (center.0 + dx, center.1 + dz),
                    min,
                    side,
                    ground_at,
                    y,
                );
            }
        }
    }
}

/// Adds dead-cone spring-water fissures after the water caldera has been filled.
/// Unlike the active-cone selector, this placed feature has no random offset or selector
/// draw; it starts directly with `FissureFeature`'s count draw. The separate 70-position
/// `SpringFeature` that follows is not synthesized because its success depends on
/// neighboring block identities and caves, which a height-column model cannot recover.
pub(super) fn add_water(
    _world: &World,
    center: (i32, i32),
    min: (i32, i32),
    side: usize,
    ground_at: &mut impl FnMut(i32, i32) -> Option<i32>,
    water: &mut [Option<i32>],
    random: &mut FeatureRandom,
) {
    if side < 3 || water.len() < side.saturating_mul(side) {
        return;
    }
    add_fissures(center, min, side, ground_at, water, random);
}

fn add_fissures(
    center: (i32, i32),
    min: (i32, i32),
    side: usize,
    ground_at: &mut impl FnMut(i32, i32) -> Option<i32>,
    surface: &mut [Option<i32>],
    random: &mut FeatureRandom,
) {
    // FissureFeature config: count=3, radius=6, 10..23 pieces, max piece length 6.
    let place_count = 1 + random.next_int_bound(3);
    for _ in 0..place_count {
        let offset_x = random.next_int_bound(6) - random.next_int_bound(6);
        let offset_z = random.next_int_bound(6) - random.next_int_bound(6);
        let offset_center = (center.0 + offset_x, center.1 + offset_z);
        let local_surface =
            surface_height(offset_center, min, side, ground_at, surface).unwrap_or(0);

        // Source's `placeFissure(level, pos, mutablePos, ...)` passes the feature origin
        // as startPos and the sampled terrain location as centerPos. The tubes therefore
        // start above the cone center even when their branch bound center is offset.
        let mut top = (
            center.0,
            surface_height(center, min, side, ground_at, surface).unwrap_or(0),
            center.1,
        );
        let branch_center = (offset_center.0, local_surface, offset_center.1);
        let pieces = 10 + random.next_int_bound(14);
        for _ in 0..pieces {
            let piece_depth = 1 + random.next_int_bound(6);
            for dy in 1..=piece_depth {
                record_exposed(surface, (top.0, top.2), min, side, ground_at, top.1 - dy);
            }

            let direction = random.next_int_bound(4) as usize;
            let (dx, dz) = bounded_direction(direction, branch_center, top, 6);
            top = (top.0, top.1 - piece_depth, top.2);
            top.0 += dx;
            top.2 += dz;
            record_exposed(surface, (top.0, top.2), min, side, ground_at, top.1);

            if top.1 < MIN_FISSURE_Y {
                break;
            }
        }
    }
}

fn bounded_direction(
    direction: usize,
    center: (i32, i32, i32),
    target: (i32, i32, i32),
    radius: i32,
) -> (i32, i32) {
    let (dx, dz) = DIRECTIONS[direction];
    let dist_x = target.0 - center.0;
    let dist_z = target.2 - center.2;
    match direction {
        1 if dist_x > radius => (-1, 0), // EAST -> WEST
        3 if dist_x < -radius => (1, 0), // WEST -> EAST
        0 if dist_z < -radius => (0, 1), // NORTH -> SOUTH
        2 if dist_z > radius => (0, -1), // SOUTH -> NORTH
        _ => (dx, dz),
    }
}

fn surface_height(
    column: (i32, i32),
    min: (i32, i32),
    side: usize,
    ground_at: &mut impl FnMut(i32, i32) -> Option<i32>,
    lava: &[Option<i32>],
) -> Option<i32> {
    let Some(index) = index_of(column, min, side) else {
        return None;
    };
    let ground = ground_at(column.0, column.1)?;
    Some(
        ground
            .max(lava[index].unwrap_or(i32::MIN))
            .saturating_add(1),
    )
}

fn record_exposed(
    lava: &mut [Option<i32>],
    column: (i32, i32),
    min: (i32, i32),
    side: usize,
    ground_at: &mut impl FnMut(i32, i32) -> Option<i32>,
    y: i32,
) {
    let Some(index) = index_of(column, min, side) else {
        return;
    };
    // Keep the highest fissure/spout fluid block at or above the solid surface. Deeper
    // tube fluid remains hidden by unmodified terrain in this height-column model.
    // The one-column border is solely input context for pool fills; source features
    // are owned by the inner 44×44 box and cannot claim predictions beyond it.
    let ix = column.0 - min.0;
    let iz = column.1 - min.1;
    if ix == 0 || iz == 0 || ix >= side as i32 - 1 || iz >= side as i32 - 1 {
        return;
    }
    let Some(ground) = ground_at(column.0, column.1) else {
        return;
    };
    if y >= ground {
        lava[index] = Some(lava[index].map_or(y, |previous| previous.max(y)));
    }
}

fn index_of(column: (i32, i32), min: (i32, i32), side: usize) -> Option<usize> {
    let x = column.0.checked_sub(min.0)?;
    let z = column.1.checked_sub(min.1)?;
    if x < 0 || z < 0 || x >= side as i32 || z >= side as i32 {
        return None;
    }
    Some(z as usize * side + x as usize)
}
