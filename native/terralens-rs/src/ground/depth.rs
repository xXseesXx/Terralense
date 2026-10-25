//! The altitude/slope gate used before TFG chooses a top surface state.
use super::{builders, gravel_rock};
use crate::{biome::ids::BIOME_IDS, world::World};
use std::{collections::HashMap, sync::OnceLock};

/// TFC `SurfaceBuilderContext.calculateAltitudeSlopeSurfaceDepth`.
/// A negative result skips grass/soil, exposing the builder's under-state or raw rock.
pub(super) fn surface_depth(y: i32, slope: f64, max: i32, min: i32) -> i32 {
    let slope_factor = 1.0 - (slope / 15.0).clamp(0.0, 1.0);
    let altitude_factor = if y < 63 {
        1.0 + ((63.0 - f64::from(y)) / 15.0 / 0.4).clamp(0.0, 1.0) * 0.4
    } else {
        1.0 - ((f64::from(y) - 63.0) / 140.0 / 0.8).clamp(0.0, 1.0) * 0.8
    };
    (f64::from(min) + slope_factor * altitude_factor * f64::from(max - min))
        .trunc()
        .clamp(f64::from(min), f64::from(max)) as i32
}

#[derive(serde::Deserialize)]
struct Profile {
    biomes: HashMap<String, Builder>,
}
#[derive(serde::Deserialize)]
struct Builder {
    normal_min_depth: Option<i32>,
}
static MIN_DEPTHS: OnceLock<[Option<i32>; 109]> = OnceLock::new();

/// Return only the exposed under-state when a source normal builder omits its
/// ordinary top state. Other builders and valid grass/soil layers return None.
pub(super) fn normal_exposure(
    world: &World,
    biome: u16,
    x: i32,
    y: i32,
    z: i32,
    rainfall: f64,
    temperature: f64,
) -> Option<String> {
    let depths = MIN_DEPTHS.get_or_init(|| {
        let profile: Profile = serde_json::from_str(include_str!("assets/surface-builders.json"))
            .expect("bundled TFG surface factory profile is valid");
        std::array::from_fn(|i| {
            profile
                .biomes
                .get(BIOME_IDS[i].rsplit('/').next().unwrap())
                .and_then(|b| b.normal_min_depth)
        })
    });
    let min = depths.get(biome as usize).copied().flatten()?;
    if y < 62 {
        return Some(gravel_rock(world, x, y, z));
    }
    let slope = crate::geology::surface_slope(&world.region, world.seed, x, z)?;
    match surface_depth(y, slope, 5, min) {
        n if n < -1 => Some(builders::raw_rock(world, x, y, z)),
        -1 => {
            // TFG's UNDER_GRAVEL has raw stone in its coldest two bands,
            // then a noise transition to gravel in band two.
            let band = super::map_index(rainfall, 35., 450., 3., 17.99, 18).min(super::map_index(
                super::elevation_temperature(temperature, y),
                -19.,
                -4.,
                0.,
                17.99,
                18,
            ));
            Some(if band < 2 || band == 2 && super::patch_noise(x, z) > 0. {
                builders::raw_rock(world, x, y, z)
            } else {
                gravel_rock(world, x, y, z)
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_depth_gates_grass_gravel_and_bare_rock() {
        assert_eq!(surface_depth(63, 0., 5, -3), 5);
        assert_eq!(surface_depth(63, 15., 5, -3), -3);
        assert_eq!(surface_depth(63, 12., 5, -3), -1);
        assert_eq!(surface_depth(63, 15., 5, -1), -1);
        assert_eq!(surface_depth(250, 0., 5, -3), -1);
        assert_eq!(surface_depth(30, 0., 5, -1), 5);
    }
}
