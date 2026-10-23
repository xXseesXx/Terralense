//! TFG's region-biome edge, shore, and ice-edge layers.
use super::{
    area::{self, Area},
    ids as b,
};
use crate::random::{JavaRandom, Xoroshiro};
fn any(v: [u16; 4], p: impl Fn(u16) -> bool) -> bool {
    v.into_iter().any(p)
}

pub fn edge_biome(n: u16, e: u16, s: u16, w: u16, c: u16) -> u16 {
    let v = [n, e, s, w];
    if b::is_low(c) {
        if any(v, b::is_ocean) && any(v, b::is_mountains) {
            return b::id::OCEANIC_MOUNTAINS;
        } else if any(v, b::is_ocean) && any(v, |x| x == b::id::LOWLANDS) {
            return b::id::SALT_MARSH;
        }
    }
    if b::is_flats(c) && any(v, b::is_ocean) && any(v, b::is_flats) {
        return b::id::CANYONS;
    }
    if c == b::id::PLATEAU || c == b::id::BADLANDS {
        if any(v, |x| x == b::id::LOW_CANYONS || x == b::id::LOWLANDS) {
            return b::id::HILLS;
        }
        if any(v, |x| x == b::id::PLAINS || x == b::id::HILLS) {
            return b::id::ROLLING_HILLS;
        }
    } else if b::is_mountains(c) {
        if any(v, b::is_low) {
            return b::id::ROLLING_HILLS;
        }
    } else if c == b::id::LOWLANDS || c == b::id::LOW_CANYONS {
        if any(v, |x| x == b::id::PLATEAU || x == b::id::BADLANDS) {
            return b::id::HILLS;
        }
        if any(v, b::is_mountains) {
            return b::id::ROLLING_HILLS;
        }
    } else if c == b::id::PLAINS || c == b::id::HILLS {
        if any(v, |x| x == b::id::PLATEAU || x == b::id::BADLANDS) {
            return b::id::HILLS;
        }
        if any(v, b::is_mountains) {
            return b::id::ROLLING_HILLS;
        }
    } else if c == b::id::DEEP_OCEAN_TRENCH && any(v, |x| !b::is_ocean(x)) {
        return b::id::OCEAN;
    }
    c
}
fn shore_for(v: u16) -> u16 {
    use b::id as i;
    if v == i::LOWLANDS || v == i::SALT_MARSH {
        return i::SALT_MARSH;
    }
    if v == i::MOUNTAINS {
        return i::OCEANIC_MOUNTAINS;
    }
    if v == i::VOLCANIC_MOUNTAINS {
        return i::VOLCANIC_OCEANIC_MOUNTAINS;
    }
    if v == i::TOWER_KARST_LAKE {
        return i::TOWER_KARST_BAY;
    }
    if v == i::ACTIVE_SHIELD_VOLCANO {
        return i::SHIELD_VOLCANO_SHORE;
    }
    if matches!(
        v,
        i::DORMANT_SHIELD_VOLCANO | i::EXTINCT_SHIELD_VOLCANO | i::ANCIENT_SHIELD_VOLCANO
    ) {
        return i::OLD_SHIELD_VOLCANO_SHORE;
    }
    if b::is_flat_ice_sheet(v) || v == i::ICE_SHEET_EDGE || v == i::ICE_SHEET_OCEANIC {
        return i::ICE_SHEET_SHORE;
    }
    if v == i::ICE_SHEET_OCEANIC_MOUNTAINS {
        return i::ICE_SHEET_OCEANIC_MOUNTAINS_EDGE;
    }
    if matches!(
        v,
        i::GLACIALLY_CARVED_OCEANIC_MOUNTAINS | i::GLACIALLY_CARVED_MOUNTAINS
    ) {
        return i::GLACIATED_OCEANIC_MOUNTAINS;
    }
    if v == i::OLD_MOUNTAINS || v == i::EXTREME_DOLINE_MOUNTAINS {
        return i::TERRACE_LOWER;
    }
    if matches!(
        v,
        i::PLATEAU | i::EXTREME_DOLINE_PLATEAU | i::BURREN_PLATEAU | i::SHILIN_PLATEAU
    ) {
        return i::SEA_STACKS;
    }
    if matches!(v, i::PLATEAU_WIDE | i::ROCKY_PLATEAU | i::DOLINE_PLATEAU) {
        return i::SETBACK_CLIFFS;
    }
    if matches!(
        v,
        i::HIGHLANDS
            | i::CENOTE_HIGHLANDS
            | i::DOLINE_HIGHLANDS
            | i::SHILIN_HIGHLANDS
            | i::TOWER_KARST_HIGHLANDS
    ) {
        return i::ROCKY_SHORES;
    }
    if matches!(
        v,
        i::ROLLING_HILLS | i::DOLINE_ROLLING_HILLS | i::CENOTE_ROLLING_HILLS
    ) {
        return i::EMBAYMENTS;
    }
    if matches!(
        v,
        i::HILLS
            | i::CENOTE_HILLS
            | i::DOLINE_HILLS
            | i::SHILIN_HILLS
            | i::TOWER_KARST_HILLS
            | i::GRASSY_DUNES
            | i::DUNE_SEA
    ) {
        return i::COASTAL_DUNES;
    }
    i::TIDAL_FLATS
}
pub fn shore_biome(n: u16, e: u16, s: u16, w: u16, c: u16) -> u16 {
    if !b::is_ocean(c) && b::has_shore(c) && [n, e, s, w].into_iter().any(b::is_ocean) {
        shore_for(c)
    } else {
        c
    }
}
pub fn more_shores_biome(n: u16, e: u16, s: u16, w: u16, c: u16) -> u16 {
    use b::id as i;
    if c != i::OCEAN {
        let v = [n, e, s, w];
        if v.contains(&i::TERRACE_LOWER) {
            return i::TERRACE_UPPER;
        }
        if v.contains(&i::SEA_STACKS) {
            return i::SEA_STACKS;
        }
        if v.contains(&i::TIDAL_FLATS) || v.contains(&i::SHORE) {
            return i::SHORE;
        }
        for (source, target) in [
            (i::COASTAL_DUNES, i::COASTAL_DUNES),
            (i::SETBACK_CLIFFS, i::SETBACK_CLIFFS),
            (i::ROCKY_SHORES, i::ROCKY_SHORES),
            (i::EMBAYMENTS, i::EMBAYMENTS),
        ] {
            if v.contains(&source) {
                return target;
            }
        }
    }
    c
}
fn not_ice(v: u16) -> bool {
    !matches!(
        v,
        b::id::ICE_SHEET
            | b::id::ICE_SHEET_TUYAS
            | b::id::SUBGLACIAL_LAKE
            | b::id::ICE_SHEET_MOUNTAINS
            | b::id::ICE_SHEET_OCEANIC_MOUNTAINS
    )
}
fn not_ice_or_glaciated(v: u16) -> bool {
    not_ice(v)
        && !matches!(
            v,
            b::id::GLACIATED_MOUNTAINS | b::id::GLACIATED_OCEANIC_MOUNTAINS
        )
}
pub fn ice_sheet_edge_biome(n: u16, e: u16, s: u16, w: u16, c: u16) -> u16 {
    use b::id as i;
    let v = [n, e, s, w];
    let has = |x| v.contains(&x);
    let any = |p: fn(u16) -> bool| v.into_iter().any(p);
    if matches!(
        c,
        i::KNOB_AND_KETTLE | i::PATTERNED_GROUND | i::INVERTED_PATTERNED_GROUND | i::STONE_CIRCLES
    ) {
        if has(i::ICE_SHEET_TUYAS) {
            return i::ICE_SHEET_TUYAS_EDGE;
        }
        if any(b::is_flat_ice_sheet) {
            return i::ICE_SHEET_EDGE;
        }
    }
    if c == i::ICE_SHEET_OCEANIC_MOUNTAINS && any(not_ice) {
        return i::ICE_SHEET_OCEANIC_MOUNTAINS_EDGE;
    }
    if c == i::ICE_SHEET_MOUNTAINS && any(not_ice) {
        return i::ICE_SHEET_MOUNTAINS_EDGE;
    }
    if c == i::ICE_SHEET_EDGE
        && any(|x| {
            matches!(
                x,
                i::ICE_SHEET_MOUNTAINS
                    | i::ICE_SHEET_MOUNTAINS_EDGE
                    | i::ICE_SHEET_OCEANIC_MOUNTAINS
                    | i::ICE_SHEET_OCEANIC
            )
        })
    {
        return i::KNOB_AND_KETTLE;
    }
    let mountain = any(|x| {
        matches!(
            x,
            i::ICE_SHEET_MOUNTAINS
                | i::ICE_SHEET_MOUNTAINS_EDGE
                | i::ICE_SHEET_OCEANIC_MOUNTAINS
                | i::ICE_SHEET_OCEANIC
        )
    });
    if c == i::LAKE && any(b::is_flat_ice_sheet) && !mountain {
        return i::SUBGLACIAL_LAKE;
    }
    if b::is_flat_ice_sheet(c) && has(i::MELTWATER_LAKE) && any(not_ice) {
        return i::SUBGLACIAL_LAKE;
    }
    if b::is_flat_ice_sheet(c)
        && any(|x| {
            matches!(
                x,
                i::OCEAN
                    | i::OCEAN_REEF
                    | i::DEEP_OCEAN
                    | i::DEEP_OCEAN_TRENCH
                    | i::ICE_SHEET_SHORE
            )
        })
    {
        return i::ICE_SHEET_OCEANIC;
    }
    if not_ice_or_glaciated(c) {
        if has(i::GLACIATED_MOUNTAINS) {
            return i::GLACIALLY_CARVED_MOUNTAINS;
        }
        if has(i::GLACIATED_OCEANIC_MOUNTAINS) {
            return i::GLACIALLY_CARVED_OCEANIC_MOUNTAINS;
        }
    }
    if matches!(
        c,
        i::PLATEAU | i::BADLANDS | i::BURREN_BADLANDS | i::BURREN_BADLANDS_TALL
    ) && has(i::ICE_SHEET_OCEANIC_MOUNTAINS_EDGE)
    {
        return i::GLACIATED_OCEANIC_MOUNTAINS;
    }
    if matches!(c, i::ICE_SHEET | i::ICE_SHEET_TUYAS) && has(i::ICE_SHEET_OCEANIC_MOUNTAINS_EDGE) {
        return i::ICE_SHEET_OCEANIC;
    }
    if c == i::ICE_SHEET_MOUNTAINS && has(i::ICE_SHEET_OCEANIC_MOUNTAINS_EDGE) {
        return i::ICE_SHEET_OCEANIC_MOUNTAINS;
    }
    c
}
fn adjacent(parent: Area, transform: fn(u16, u16, u16, u16, u16) -> u16) -> Area {
    area::adjacent_transform(parent, move |n, e, s, w, c| {
        transform(n as u16, e as u16, s as u16, w as u16, c as u16) as i32
    })
}
/// At quart coordinates, 16 blocks per output cell.
pub fn region_biome_layer(seed: i64, region_biome: Area) -> Area {
    let mut random = Xoroshiro::new(seed);
    random.next_long();
    let mut layer = area::cached_area(move |x, z| region_biome(x, z));
    random.next_long();
    layer = adjacent(layer, edge_biome);
    layer = area::zoom_area(random.next_long(), layer);
    random.next_long();
    layer = adjacent(layer, shore_biome);
    random.next_long();
    layer = adjacent(layer, more_shores_biome);
    random.next_long();
    layer = adjacent(layer, ice_sheet_edge_biome);
    for _ in 0..4 {
        layer = area::zoom_area(random.next_long(), layer);
    }
    area::smooth_area(random.next_long(), layer)
}

/// TFC's block-resolution rock layer; raw seed is intentionally reused for every zoom.
pub fn overworld_rock_layer(seed: i64, region_rock: Area) -> Area {
    let mut random = JavaRandom::new(seed);
    random.next_long();
    let mut layer = area::cached_area(move |x, z| region_rock(x, z));
    for _ in 0..6 {
        layer = area::zoom_area(seed, layer);
    }
    layer = area::smooth_area(seed, layer);
    layer = area::zoom_area(seed, layer);
    area::smooth_area(seed, layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use b::id as i;
    #[test]
    fn coastal_lowlands_make_salt_marsh() {
        assert_eq!(
            edge_biome(i::OCEAN, i::LOWLANDS, i::PLAINS, i::PLAINS, i::LOWLANDS),
            i::SALT_MARSH
        )
    }
    #[test]
    fn shore_is_selected_from_adjacent_ocean() {
        assert_eq!(
            shore_biome(i::OCEAN, i::PLAINS, i::PLAINS, i::PLAINS, i::HILLS),
            i::COASTAL_DUNES
        )
    }
    #[test]
    fn biome_layer_has_deterministic_results() {
        let base: Area = std::rc::Rc::new(|x, z| {
            if x < 0 {
                i::OCEAN as i32
            } else if z < 0 {
                i::HILLS as i32
            } else {
                i::PLAINS as i32
            }
        });
        let a = region_biome_layer(1234, base);
        let x = a(3, 4);
        assert_eq!(a(3, 4), x)
    }

    #[test]
    fn tfg_region_biome_layer_matches_typescript_capture() {
        use std::rc::Rc;
        let base: Area = Rc::new(|x, z| {
            if x < 0 {
                i::OCEAN as i32
            } else if z < 0 {
                i::HILLS as i32
            } else {
                i::PLAINS as i32
            }
        });
        let layer = region_biome_layer(987_654_321, base);
        let points = [(-9, -8), (-1, 0), (0, 0), (3, 7), (22, -18), (100, 63)];
        assert_eq!(points.map(|(x, z)| layer(x, z)), [2, 21, 21, 21, 21, 4]);
    }
}
