//! Overworld surface-water approximation, separate from biome identity and terrain height.
//!
//! TFC's sea-level aquifer fills air blocks strictly below its fluid level. Its sampled
//! terrain height is a noise input, not the highest solid block: the height-only density
//! and upper slide can leave solid river banks above that input. Underground aquifer
//! borders, carving samplers and cave pockets are not represented by this surface model.
pub const SEA_LEVEL: i32 = 63;
pub const SEA_SURFACE_BLOCK_Y: i32 = SEA_LEVEL - 1;

pub fn is_water_biome(id: u16) -> bool {
    crate::biome::is_ocean(id) || is_inland_water_biome(id)
}

/// A biome label, not evidence that the surface block contains water.
pub fn is_inland_water_biome(id: u16) -> bool {
    let name = crate::biome::biome_name(id);
    name.ends_with("river") || name.ends_with("lake")
}

/// A dry river-biome column whose blended mountain profile keeps the river
/// underneath the surface. This is a display classification, not surface water.
pub(crate) fn is_underground_river(
    world: &crate::world::World,
    x: i32,
    z: i32,
    surface_y: i32,
) -> bool {
    surface_y > SEA_LEVEL
        && crate::geology::river_cave_weight(&world.region, world.seed, x, z)
            .is_some_and(|weight| weight >= 0.25)
}

pub fn is_surface_water(id: u16, terrain_height: Option<f64>) -> bool {
    crate::biome::is_ocean(id)
        || terrain_height.is_some_and(|height| {
            height.is_finite() && surface_density(SEA_SURFACE_BLOCK_Y, height) <= 0.
        })
}

/// TFGChunkNoiseFiller.calculateNoiseAtHeight for a height-only biome sampler. Such
/// samplers return zero carving noise; the filler converts that to 0.4 solid density
/// and applies its upper slide against the integer heightNoiseValue. Preserve Java's
/// float-to-double promotion: using f64 0.2 changes the two-block boundary's sign.
fn surface_density(y: i32, terrain_height: f64) -> f64 {
    const AIR_THRESHOLD: f64 = 0.4;
    const UPPER_SLIDE: f64 = 0.2_f32 as f64;
    let height_noise_value = terrain_height.trunc();
    AIR_THRESHOLD - (f64::from(y) - height_noise_value).max(0.) * UPPER_SLIDE
}

/// Highest solid cell of the ordinary height-only filler, before features.
/// The fill ceiling is the larger of the noise height and sea level; below
/// sea level the upper density slide retains the first block above the noise.
/// Special carving/density samplers can produce a different top cell.
pub(crate) fn ordinary_solid_top(terrain_height: f64) -> Option<i32> {
    if !terrain_height.is_finite() {
        return None;
    }
    let noise = terrain_height as i32;
    let upper = noise.saturating_add(2).min(noise.max(SEA_LEVEL));
    Some(if surface_density(upper, terrain_height) > 0. {
        upper
    } else {
        upper.saturating_sub(1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::ids::id;

    #[test]
    fn ordinary_solid_cap_distinguishes_the_saved_river_bank_from_raw_noise() {
        assert_eq!(ordinary_solid_top(61.), Some(62));
        assert_eq!(ordinary_solid_top(60.), Some(61));
        assert_eq!(ordinary_solid_top(62.), Some(63));
        assert_eq!(ordinary_solid_top(63.), Some(63));
        assert_eq!(ordinary_solid_top(100.), Some(100));
        assert_eq!(ordinary_solid_top(f64::NAN), None);
        let world = crate::world::World::new(0, crate::model::Settings::default());
        let bank = crate::ground::base_sample(&world, 8833, 11107).unwrap();
        assert_eq!(bank.y, 62, "saved grass-bank solid Y, noise height is 61");
        assert!(crate::ground_palette::is_grass_soil(&bank.block));
    }

    #[test]
    fn floodplain_water_matches_the_independent_saved_bank_and_riverbed_columns() {
        let observation: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/flooded-lowlands/seed0-floodplain-bank-8833-11107.json"
        ))
        .unwrap();
        let world = crate::world::World::new(0, crate::model::Settings::default());
        let mut wet = 0;
        let columns = observation["columns"].as_array().unwrap();
        assert_eq!(columns.len(), 225);
        for column in columns {
            let x = column["x"].as_i64().unwrap() as i32;
            let z = column["z"].as_i64().unwrap() as i32;
            let probe = world.probe(x, z);
            assert_eq!(probe.biome, column["biome"].as_str().unwrap(), "{x},{z}");
            assert_eq!(
                probe.height,
                column["terrain_noise_height"].as_f64(),
                "noise height {x},{z}"
            );
            let expected = column["surface_water"].as_bool().unwrap();
            assert_eq!(
                probe.surface_water, expected,
                "saved surface at {x},{z}: {}",
                column["ground_block"]
            );
            wet += usize::from(expected);
        }
        assert_eq!(wet, 36);
        assert_eq!(columns.len() - wet, 189);
    }

    #[test]
    fn low_non_ocean_terrain_fills_to_the_sea_level_boundary_without_changing_biome() {
        assert!(!crate::biome::is_ocean(id::PLAINS));
        for height in [-30., 0., 59., 60., 60.99] {
            assert!(is_surface_water(id::PLAINS, Some(height)));
        }
        for height in [61., 61.99, 62., 62.99] {
            assert!(
                !is_surface_water(id::PLAINS, Some(height)),
                "solid terrain at the fluid surface must not be flooded: noise height {height}"
            );
        }
        assert!(!is_surface_water(id::PLAINS, Some(63.)));
        assert!(!is_surface_water(id::PLAINS, Some(64.)));
        assert!(!is_surface_water(id::PLAINS, None));
        assert!(!is_surface_water(id::PLAINS, Some(f64::NAN)));
        assert!(is_surface_water(id::OCEAN, None));
        assert_eq!(SEA_SURFACE_BLOCK_Y, 62);
    }

    #[test]
    fn inland_biome_labels_cannot_flood_solid_surface_terrain() {
        for biome in [
            id::RIVER,
            id::LAKE,
            id::MOUNTAIN_LAKE,
            id::PLATEAU_LAKE,
            id::SUBGLACIAL_LAKE,
            id::MELTWATER_LAKE,
        ] {
            assert!(is_water_biome(biome));
            for height in [61., 62., 63., 64., 100., 240.] {
                assert!(
                    !is_surface_water(biome, Some(height)),
                    "dry inland terrain: biome {biome}, height {height}"
                );
            }
            assert!(is_surface_water(biome, Some(60.)));
            assert!(!is_surface_water(biome, None));
        }
    }
}
