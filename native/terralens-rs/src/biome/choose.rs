//! TFG Core Modern 0.9.21 biome selection (`choose.ts`).
use super::ids::{self as b, id as i};
use crate::region::RegionPoint;
const MOUNTAIN: [u16; 7] = [
    i::MOUNTAINS,
    i::MOUNTAINS,
    i::MOUNTAINS,
    i::OLD_MOUNTAINS,
    i::OLD_MOUNTAINS,
    i::PLATEAU,
    i::HIGHLANDS,
];
const COASTAL: [u16; 6] = [
    i::VOLCANIC_MOUNTAINS,
    i::VOLCANIC_OCEANIC_MOUNTAINS,
    i::VOLCANIC_OCEANIC_MOUNTAINS,
    i::OCEANIC_MOUNTAINS,
    i::OCEANIC_MOUNTAINS,
    i::ROLLING_HILLS,
];
const ALT_LOW: [u16; 8] = [
    i::PLAINS,
    i::PLAINS,
    i::HILLS,
    i::HILLS,
    i::ROLLING_HILLS,
    i::LOW_CANYONS,
    i::LOWLANDS,
    i::LOWLANDS,
];
const ALT_MID: [u16; 13] = [
    i::PLAINS,
    i::HILLS,
    i::ROLLING_HILLS,
    i::ROLLING_HILLS,
    i::ROLLING_HILLS,
    i::HIGHLANDS,
    i::BUTTES,
    i::MESAS,
    i::BADLANDS,
    i::PLATEAU_WIDE,
    i::CANYONS,
    i::CANYONS,
    i::LOW_CANYONS,
];
const ALT_HIGH: [u16; 17] = [
    i::HIGHLANDS,
    i::HIGHLANDS,
    i::HIGHLANDS,
    i::HIGHLANDS,
    i::ROLLING_HILLS,
    i::ROLLING_HILLS,
    i::BADLANDS,
    i::BADLANDS,
    i::STAIR_STEP_CANYONS,
    i::PLATEAU,
    i::PLATEAU,
    i::PLATEAU,
    i::PLATEAU_WIDE,
    i::OLD_MOUNTAINS,
    i::OLD_MOUNTAINS,
    i::OLD_MOUNTAINS,
    i::OLD_MOUNTAINS,
];
const ICE_LOW: [u16; 6] = [
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET_TUYAS,
];
const ICE_MID: [u16; 8] = [
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET_TUYAS,
    i::ICE_SHEET,
    i::ICE_SHEET_TUYAS,
];
const ICE_HIGH: [u16; 8] = [
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET,
    i::ICE_SHEET_TUYAS,
    i::ICE_SHEET_TUYAS,
    i::ICE_SHEET_MOUNTAINS,
    i::ICE_SHEET_MOUNTAINS,
];
const PALEO_LOW: [u16; 9] = [
    i::PATTERNED_GROUND,
    i::INVERTED_PATTERNED_GROUND,
    i::KNOB_AND_KETTLE,
    i::KNOB_AND_KETTLE,
    i::KNOB_AND_KETTLE,
    i::DRUMLINS,
    i::TUYAS,
    i::LOWLANDS,
    i::LOWLANDS,
];
const PALEO_MID: [u16; 8] = [
    i::PATTERNED_GROUND,
    i::KNOB_AND_KETTLE,
    i::DRUMLINS,
    i::DRUMLINS,
    i::DRUMLINS,
    i::DRUMLINS,
    i::TUYAS,
    i::TUYAS,
];
const PALEO_HIGH: [u16; 10] = [
    i::DRUMLINS,
    i::DRUMLINS,
    i::DRUMLINS,
    i::BADLANDS,
    i::BADLANDS,
    i::PLATEAU,
    i::PLATEAU,
    i::PLATEAU,
    i::PLATEAU_WIDE,
    i::ICE_SHEET_MOUNTAINS,
];
const DESERT_LOW: [u16; 7] = [
    i::BUTTES,
    i::GRASSY_DUNES,
    i::DUNE_SEA,
    i::DUNE_SEA,
    i::DUNE_SEA,
    i::SALT_FLATS,
    i::SALT_FLATS,
];
const DESERT_MID: [u16; 11] = [
    i::DUNE_SEA,
    i::BUTTES,
    i::BUTTES,
    i::HOODOOS,
    i::MESAS,
    i::MESAS,
    i::STAIR_STEP_CANYONS,
    i::BADLANDS,
    i::PLATEAU,
    i::CANYONS,
    i::WHORLED_CANYONS,
];
const DESERT_HIGH: [u16; 10] = [
    i::HOODOOS,
    i::MESAS,
    i::STAIR_STEP_CANYONS,
    i::STAIR_STEP_CANYONS,
    i::ROCKY_PLATEAU,
    i::ROCKY_PLATEAU,
    i::OLD_MOUNTAINS,
    i::OLD_MOUNTAINS,
    i::OLD_MOUNTAINS,
    i::WHORLED_CANYONS,
];
const SEMI_LOW: [u16; 11] = [
    i::PLAINS,
    i::HILLS,
    i::HILLS,
    i::GRASSY_DUNES,
    i::GRASSY_DUNES,
    i::GRASSY_DUNES,
    i::GRASSY_DUNES,
    i::LOW_CANYONS,
    i::LOWLANDS,
    i::LOWLANDS,
    i::MUD_FLATS,
];
const SEMI_MID: [u16; 16] = [
    i::PLAINS,
    i::HILLS,
    i::ROLLING_HILLS,
    i::ROLLING_HILLS,
    i::HIGHLANDS,
    i::HIGHLANDS,
    i::BADLANDS,
    i::BADLANDS,
    i::PLATEAU_WIDE,
    i::CANYONS,
    i::LOW_CANYONS,
    i::WHORLED_CANYONS,
    i::BUTTES,
    i::MESAS,
    i::MESAS,
    i::HOODOOS,
];
const SEMI_HIGH: [u16; 15] = [
    i::HIGHLANDS,
    i::HIGHLANDS,
    i::MESAS,
    i::HOODOOS,
    i::ROLLING_HILLS,
    i::ROLLING_HILLS,
    i::BADLANDS,
    i::BADLANDS,
    i::PLATEAU_WIDE,
    i::ROCKY_PLATEAU,
    i::STAIR_STEP_CANYONS,
    i::STAIR_STEP_CANYONS,
    i::OLD_MOUNTAINS,
    i::OLD_MOUNTAINS,
    i::WHORLED_CANYONS,
];
const ISLAND: [u16; 6] = [
    i::PLAINS,
    i::HILLS,
    i::ROLLING_HILLS,
    i::VOLCANIC_OCEANIC_MOUNTAINS,
    i::VOLCANIC_OCEANIC_MOUNTAINS,
    i::GUANO_ISLAND,
];
const MID_OCEAN: [u16; 6] = [
    i::DEEP_OCEAN,
    i::OCEAN,
    i::OCEAN,
    i::OCEAN_REEF,
    i::OCEAN_REEF,
    i::OCEAN_REEF,
];
fn pick(seed: i64, area: i32, v: &[u16]) -> u16 {
    // JS uses floorMod(BigInt(seed) ^ BigInt(area), n). Interpreting the XOR as unsigned
    // changes selection for negative values unless n divides 2^64.
    let xor = seed ^ area as i64;
    v[xor.rem_euclid(v.len() as i64) as usize]
}
fn altitude(p: &RegionPoint, lo: &[u16], mid: &[u16], high: &[u16], seed: i64, a: i32) -> u16 {
    let v = match p.discrete_biome_altitude() {
        0 => lo,
        1 => mid,
        _ => high,
    };
    pick(seed, a, v)
}
fn tower(v: u16) -> u16 {
    if v == i::SALT_MARSH {
        i::TOWER_KARST_BAY
    } else if v == i::LOWLANDS {
        i::TOWER_KARST_LAKE
    } else if v == i::PLAINS || v == i::LOW_CANYONS {
        i::TOWER_KARST_PLAINS
    } else if v == i::CANYONS {
        i::TOWER_KARST_CANYONS
    } else if v == i::HILLS || v == i::ROLLING_HILLS || v == i::BADLANDS {
        i::TOWER_KARST_HILLS
    } else if v == i::HIGHLANDS {
        i::TOWER_KARST_HIGHLANDS
    } else if v == i::PLATEAU || v == i::PLATEAU_WIDE {
        i::EXTREME_DOLINE_PLATEAU
    } else if v == i::OLD_MOUNTAINS || v == i::MOUNTAINS || v == i::OCEANIC_MOUNTAINS {
        i::EXTREME_DOLINE_MOUNTAINS
    } else {
        v
    }
}
fn shilin(v: u16) -> u16 {
    match v {
        i::PLAINS => i::SHILIN_PLAINS,
        i::CANYONS => i::SHILIN_CANYONS,
        i::ROLLING_HILLS | i::BADLANDS => i::SHILIN_HILLS,
        i::PLATEAU | i::PLATEAU_WIDE => i::SHILIN_PLATEAU,
        i::HIGHLANDS => i::SHILIN_HIGHLANDS,
        _ => v,
    }
}
fn burren(v: u16) -> u16 {
    match v {
        i::PLAINS | i::CANYONS => i::BURREN_PLAINS,
        i::BADLANDS | i::HILLS | i::ROLLING_HILLS => i::BURREN_BADLANDS,
        i::DRUMLINS => i::BURREN_ROCHE_MOUTONEE,
        i::HIGHLANDS => i::BURREN_BADLANDS_TALL,
        i::PLATEAU | i::PLATEAU_WIDE => i::BURREN_PLATEAU,
        _ => v,
    }
}
fn doline(v: u16) -> u16 {
    match v {
        i::CANYONS => i::DOLINE_CANYONS,
        i::PLAINS | i::LOW_CANYONS => i::DOLINE_PLAINS,
        i::HILLS => i::DOLINE_HILLS,
        i::ROLLING_HILLS => i::DOLINE_ROLLING_HILLS,
        i::HIGHLANDS => i::DOLINE_HIGHLANDS,
        i::PLATEAU | i::PLATEAU_WIDE => i::DOLINE_PLATEAU,
        _ => v,
    }
}
fn cenote(v: u16) -> u16 {
    match v {
        i::CANYONS => i::CENOTE_CANYONS,
        i::PLAINS | i::LOW_CANYONS => i::CENOTE_PLAINS,
        i::HILLS => i::CENOTE_HILLS,
        i::ROLLING_HILLS => i::CENOTE_ROLLING_HILLS,
        i::HIGHLANDS => i::CENOTE_HIGHLANDS,
        i::PLATEAU | i::PLATEAU_WIDE => i::CENOTE_PLATEAU,
        _ => v,
    }
}
pub fn choose_biome(p: &mut RegionPoint, seed: i64, climate_seed: i64, area_seed: i32) {
    if p.island {
        p.biome = pick(seed, area_seed, &ISLAND)
    } else if p.mountain {
        let max = -14.0f32 + 0.006f32 * p.rainfall;
        if p.coastal_mountain {
            let c = -16.0f32 + 0.006f32 * p.rainfall;
            p.biome = if p.temperature < c + 2.0 {
                i::ICE_SHEET_OCEANIC_MOUNTAINS
            } else if p.temperature < c + 6.0 {
                i::GLACIATED_OCEANIC_MOUNTAINS
            } else if p.temperature < c + 10.0 {
                i::GLACIALLY_CARVED_OCEANIC_MOUNTAINS
            } else {
                pick(seed, area_seed, &COASTAL)
            }
        } else {
            p.biome = if p.temperature < max {
                i::ICE_SHEET_MOUNTAINS
            } else if p.temperature < max + 4.0 {
                i::GLACIATED_MOUNTAINS
            } else if p.temperature < max + 10.0 {
                i::GLACIALLY_CARVED_MOUNTAINS
            } else {
                pick(seed, area_seed, &MOUNTAIN)
            }
        }
    } else if p.land {
        let rain = p.rainfall;
        let max = -17.0f32 + 0.006f32 * rain;
        let temp = p.temperature;
        if temp < max {
            let v = altitude(p, &ICE_LOW, &ICE_MID, &ICE_HIGH, seed, area_seed);
            p.biome = if p.distance_to_ocean < 3 && b::is_flat_ice_sheet(v) {
                i::ICE_SHEET_OCEANIC
            } else {
                v
            }
        } else if temp < max + 1.0 {
            p.biome = i::ICE_SHEET_EDGE
        } else if temp < max + 2.5 {
            p.biome = pick(
                seed,
                area_seed,
                &[
                    i::KNOB_AND_KETTLE,
                    i::PATTERNED_GROUND,
                    i::INVERTED_PATTERNED_GROUND,
                ],
            )
        } else if temp < max + 6.0 {
            p.biome = altitude(p, &PALEO_LOW, &PALEO_MID, &PALEO_HIGH, seed, area_seed)
        } else if rain < 60.0 {
            p.biome = altitude(p, &DESERT_LOW, &DESERT_MID, &DESERT_HIGH, seed, area_seed)
        } else if rain < 155.0 {
            p.biome = altitude(p, &SEMI_LOW, &SEMI_MID, &SEMI_HIGH, seed, area_seed)
        } else {
            p.biome = altitude(p, &ALT_LOW, &ALT_MID, &ALT_HIGH, seed, area_seed)
        }
    } else if p.base_ocean_depth < 3 {
        p.biome = i::OCEAN
    } else if p.base_ocean_depth > 9 {
        p.biome = i::DEEP_OCEAN_TRENCH
    } else if p.base_ocean_depth >= 5 || p.distance_to_edge < 2 {
        p.biome = i::DEEP_OCEAN
    } else {
        p.biome = pick(seed, area_seed, &MID_OCEAN)
    }
    if p.hotspot_age > 0 {
        if p.hotspot_age == 4
            && matches!(
                p.biome,
                i::OCEAN | i::DEEP_OCEAN | i::OCEAN_REEF | i::DEEP_OCEAN_TRENCH
            )
        {
            p.biome = i::SUNKEN_SHIELD_VOLCANO
        } else {
            p.biome = match p.hotspot_age {
                4 => i::ANCIENT_SHIELD_VOLCANO,
                3 => i::EXTINCT_SHIELD_VOLCANO,
                2 => i::DORMANT_SHIELD_VOLCANO,
                1 => i::ACTIVE_SHIELD_VOLCANO,
                _ => i::PLAINS,
            }
        }
    }
    let offset = ((area_seed as i64 ^ climate_seed).rem_euclid(40)) as f32;
    let rain = p.rainfall;
    let temp = p.temperature;
    if rain < 90.0 + offset && rain <= 55.0 {
        if p.biome == i::LOWLANDS || p.biome == i::LOW_CANYONS {
            p.biome = i::SALT_FLATS
        } else if matches!(p.biome, i::HILLS | i::ROLLING_HILLS | i::PLATEAU) {
            p.biome = i::DUNE_SEA
        }
    }
    if rain < 145.0 && matches!(p.biome, i::PATTERNED_GROUND | i::INVERTED_PATTERNED_GROUND) {
        p.biome = i::STONE_CIRCLES
    }
    let max_rain = 420.0 + offset;
    if rain > max_rain && p.biome == i::BADLANDS {
        p.biome = i::HIGHLANDS
    }
    let ice = -14.0f32 + 0.006f32 * rain;
    if p.land
        && temp < ice
        && matches!(
            p.biome,
            i::ACTIVE_SHIELD_VOLCANO | i::DORMANT_SHIELD_VOLCANO | i::EXTINCT_SHIELD_VOLCANO
        )
    {
        p.biome = i::ICE_SHEET_SHIELD_VOLCANO
    } else if temp < ice + 4.0
        && matches!(
            p.biome,
            i::ACTIVE_SHIELD_VOLCANO | i::DORMANT_SHIELD_VOLCANO | i::EXTINCT_SHIELD_VOLCANO
        )
    {
        p.biome = i::GLACIATED_SHIELD_VOLCANO
    }
    if p.karst {
        if rain > 375.0 {
            if rain > 425.0 && rain + 10.0 * temp > 500.0 {
                p.biome = tower(p.biome)
            } else if temp > 9.0 {
                p.biome = shilin(p.biome)
            } else if temp < 0.0 {
                p.biome = burren(p.biome)
            } else {
                p.biome = doline(p.biome)
            }
        } else if rain > 250.0 {
            p.biome = if temp > 5.0 {
                cenote(p.biome)
            } else {
                doline(p.biome)
            }
        }
    }
    if p.distance_to_ocean <= 2 && p.biome == i::LOWLANDS && rain > 220.0 && temp > 18.0 {
        p.biome = i::SALT_MARSH
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_selects_biome_and_climate_substitutions() {
        let mut p = RegionPoint::default();
        p.land = true;
        p.rainfall = 20.0;
        p.temperature = 15.0;
        p.biome_altitude = 0;
        choose_biome(&mut p, 31, -14, 5);
        assert!(matches!(
            p.biome,
            i::SALT_FLATS | i::DUNE_SEA | i::PLAINS | i::HILLS | i::ROLLING_HILLS
        ));
    }
    #[test]
    fn ocean_depth_assigns_deep_trench() {
        let mut p = RegionPoint::default();
        p.base_ocean_depth = 10;
        choose_biome(&mut p, 1, 2, 3);
        assert_eq!(p.biome, i::DEEP_OCEAN_TRENCH);
    }

    #[test]
    fn negative_seed_choice_uses_signed_floor_mod() {
        assert_eq!(pick(-3, 1, &[10, 11, 12]), 12);
    }
}
