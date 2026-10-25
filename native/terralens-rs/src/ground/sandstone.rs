//! TFG badlands sandstone layers.
//!
//! Port of `TFGBadlandsSurfaceBuilder` from TFG Core Modern 0.9.25. The builder
//! initializes a world-seeded palette once, then chooses its exposed block from
//! the true surface column, biome blend weight, rainfall and slope.

use crate::{biome::ids::id, noise::OpenSimplex2D, random::Xoroshiro, world::World};
use lru::LruCache;
use std::{cell::RefCell, num::NonZeroUsize, sync::Arc};

const LAYER_SIZE: usize = 16;
const SEA_LEVEL: i32 = 63;

#[derive(Clone)]
struct Palette {
    sand0: [&'static str; LAYER_SIZE],
    sand1: [&'static str; LAYER_SIZE],
    sand_karst: [&'static str; LAYER_SIZE],
    sand_mafic: [&'static str; LAYER_SIZE],
    stone0: [&'static str; LAYER_SIZE],
    stone1: [&'static str; LAYER_SIZE],
    stone_karst: [&'static str; LAYER_SIZE],
    stone_mafic: [&'static str; LAYER_SIZE],
    thresholds: [f32; LAYER_SIZE],
    sand_height: OpenSimplex2D,
    dipping_height: OpenSimplex2D,
    grass_height: OpenSimplex2D,
    style: OpenSimplex2D,
}

thread_local! {
    static PALETTES: RefCell<LruCache<i64, Arc<Palette>>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(4).expect("nonzero cache size"))
    );
}

/// Return TFG's top state for biomes using its badlands builder.
///
/// Call this for the selected surface-builder biome (including river-selected
/// biomes), not the raw map biome. Ordinary soil tops remain the responsibility
/// of the existing soil sampler; this function claims only sandstone/sand or a
/// top known to be bare sandstone.
pub(super) fn sample(
    world: &World,
    biome: u16,
    x: i32,
    y: i32,
    z: i32,
    rainfall: f64,
    temperature: f64,
) -> Option<String> {
    let mode = mode(biome)?;
    let height = world.height(x, z)?;
    let context =
        crate::geology::surface_biome_with_height(&world.region, world.seed, x, z, Some(height))?;
    let config = with_palette(world.seed, |p| p.clone());
    let karst = matches!(
        world.rock_at(x, 0, z).as_deref(),
        Some("limestone" | "dolomite" | "chalk" | "marble")
    );
    let mafic = matches!(world.rock_at(x, 0, z).as_deref(), Some("gabbro" | "basalt"));

    if mode.inverted {
        let grass_variation = config.grass_height.sample(x as f64, z as f64);
        let weight_variation = (1.0 - context.weight) * 23.0;
        let rainfall_variation = clamped_map(rainfall, 100.0, 500.0, 0.0, 22.0);
        let shift = if mode.dipping { -10 } else { -16 };
        let base =
            (f64::from(shift) + grass_variation + weight_variation + rainfall_variation) as i32;
        let sand_height = if mode.dipping {
            config.dipping_height.sample(x as f64, z as f64) as i32
        } else {
            config.sand_height.sample(x as f64, z as f64) as i32
        };

        if y < SEA_LEVEL - 1 {
            return Some(local_sand(world, x, y, z));
        }

        let slope = crate::geology::surface_slope(&world.region, world.seed, x, z)?;
        let depth = crate::ground::depth::surface_depth(y, slope, 5, mode.soil_min_depth);
        let sandstone_depth = y + 1 - base;
        if depth < 0 && sandstone_depth > 0 {
            return Some(sample_layer(
                &config,
                x,
                z,
                y + sand_height,
                karst,
                mafic,
                true,
                mode.dipping,
            ));
        }
        if depth < 0 {
            return Some(super::builders::raw_rock(world, x, y, z));
        }
        let dry = local_sand(world, x, y, z);
        return Some(super::soil_top(
            world,
            biome,
            x,
            y,
            z,
            rainfall,
            temperature,
            Some(&dry),
        ));
    }

    // NORMAL badlands use ordinary climate soil on broad tops, but expose
    // layered sand/sandstone on sufficiently eroded faces.
    let grass_variation = config.grass_height.sample(x as f64, z as f64);
    let weight_variation = (1.0 - context.weight) * 23.0;
    let rainfall_variation = clamped_map(rainfall, 100.0, 500.0, 0.0, 22.0);
    if f64::from(y + 1 - 5) > grass_variation - weight_variation - rainfall_variation {
        let slope = crate::geology::surface_slope(&world.region, world.seed, x, z)?;
        return Some(match super::depth::surface_depth(y, slope, 5, -1) {
            -1 => super::gravel_rock(world, x, y, z),
            _ => {
                let dry = local_sand(world, x, y, z);
                super::soil_top(world, biome, x, y, z, rainfall, temperature, Some(&dry))
            }
        });
    }
    if y < SEA_LEVEL - 1 {
        return Some(local_sand(world, x, y, z));
    }
    let sand_height = config.sand_height.sample(x as f64, z as f64) as i32;
    Some(sample_layer(
        &config,
        x,
        z,
        y + sand_height,
        karst,
        mafic,
        false,
        false,
    ))
}

#[derive(Clone, Copy)]
struct Mode {
    inverted: bool,
    dipping: bool,
    soil_min_depth: i32,
}

fn mode(biome: u16) -> Option<Mode> {
    use id::*;
    Some(match biome {
        BADLANDS => Mode {
            inverted: false,
            dipping: false,
            soil_min_depth: -1,
        },
        MESAS | BUTTES | STAIR_STEP_CANYONS => Mode {
            inverted: true,
            dipping: false,
            soil_min_depth: -3,
        },
        HOODOOS => Mode {
            inverted: true,
            dipping: false,
            soil_min_depth: -7,
        },
        // TFG's WARPED factory is used by whorled_canyons.
        WHORLED_CANYONS => Mode {
            inverted: true,
            dipping: true,
            soil_min_depth: -20,
        },
        _ => return None,
    })
}

fn with_palette<T>(seed: i64, f: impl FnOnce(&Arc<Palette>) -> T) -> T {
    PALETTES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.peek(&seed).is_none() {
            cache.put(seed, Arc::new(Palette::new(seed)));
        }
        f(cache.peek(&seed).expect("palette inserted"))
    })
}

impl Palette {
    fn new(seed: i64) -> Self {
        // Seed.of(seed).fork(): two world-seeded Xoroshiro longs, then the
        // factory's sequential noise seeds, shuffle draws and thresholds.
        let mut world_rng = Xoroshiro::new(seed);
        let lo = world_rng.next_long() as u64;
        let hi = world_rng.next_long() as u64;
        let mut rng = Xoroshiro::from_state(lo, hi);

        let sand_height_seed = rng.next_long();
        let mut sand_height = OpenSimplex2D::new(sand_height_seed);
        sand_height
            .octaves(2)
            .scaled(0.0, 6.0)
            .spread(f64::from(0.0014_f32));
        let mut dipping_height = OpenSimplex2D::new(sand_height_seed);
        dipping_height
            .octaves(2)
            .scaled(-250.0, 250.0)
            .spread(0.005);

        let sand0 = fill_palette(&mut rng, "red", "brown", "yellow", false);
        let stone0 = fill_palette(&mut rng, "red", "brown", "yellow", true);
        let sand1 = fill_palette(&mut rng, "brown", "yellow", "white", false);
        let stone1 = fill_palette(&mut rng, "brown", "yellow", "white", true);
        let sand_karst = fill_palette(&mut rng, "red", "yellow", "white", false);
        let stone_karst = fill_palette(&mut rng, "red", "yellow", "white", true);
        let sand_mafic = fill_palette(&mut rng, "black", "tuff", "red", false);
        let stone_mafic = fill_palette(&mut rng, "black", "tuff", "red", true);

        let mut thresholds = [0.0; LAYER_SIZE];
        for threshold in &mut thresholds {
            *threshold = rng.next_float();
        }
        let mut grass_height = OpenSimplex2D::new(rng.next_long());
        grass_height.octaves(2).scaled(77.0, 81.0).spread(0.5);
        let mut style = OpenSimplex2D::new(rng.next_long());
        style
            .octaves(2)
            .scaled(f64::from(-0.3_f32), f64::from(1.3_f32))
            .spread(f64::from(0.0003_f32));

        Self {
            sand0,
            sand1,
            sand_karst,
            sand_mafic,
            stone0,
            stone1,
            stone_karst,
            stone_mafic,
            thresholds,
            sand_height,
            dipping_height,
            grass_height,
            style,
        }
    }
}

fn fill_palette(
    rng: &mut Xoroshiro,
    primary: &'static str,
    secondary: &'static str,
    uncommon: &'static str,
    sandstone_layer: bool,
) -> [&'static str; LAYER_SIZE] {
    let mut layers = [primary; LAYER_SIZE];
    layers[8..13].fill(secondary);
    layers[13..16].fill(uncommon);
    for i in (1..LAYER_SIZE).rev() {
        layers.swap(i, rng.next_int_bound((i + 1) as i32) as usize);
    }
    layers.map(|c| {
        if sandstone_layer {
            match c {
                "tuff" => "minecraft:tuff",
                "black" => "tfc:raw_sandstone/black",
                "brown" => "tfc:raw_sandstone/brown",
                "red" => "tfc:raw_sandstone/red",
                "yellow" => "tfc:raw_sandstone/yellow",
                "white" => "tfc:raw_sandstone/white",
                _ => "tfc:raw_sandstone/red",
            }
        } else {
            match c {
                "tuff" => "tfg:tuff_gravel",
                _ => sand_id(c),
            }
        }
    })
}

fn sand_id(color: &str) -> &'static str {
    match color {
        "black" => "tfc:sand/black",
        "brown" => "tfc:sand/brown",
        "green" => "tfc:sand/green",
        "pink" => "tfc:sand/pink",
        "white" => "tfc:sand/white",
        "yellow" => "tfc:sand/yellow",
        _ => "tfc:sand/red",
    }
}

#[cfg(test)]
fn sandstone_id(color: &str) -> &'static str {
    match color {
        "black" => "tfc:raw_sandstone/black",
        "brown" => "tfc:raw_sandstone/brown",
        "red" => "tfc:raw_sandstone/red",
        "white" => "tfc:raw_sandstone/white",
        "yellow" => "tfc:raw_sandstone/yellow",
        _ => "tfc:raw_sandstone/red",
    }
}

fn sample_layer(
    p: &Palette,
    x: i32,
    z: i32,
    y: i32,
    karst: bool,
    mafic: bool,
    sandstone: bool,
    dipping: bool,
) -> String {
    let layer_y = if dipping { y / 4 } else { y };
    let index = layer_y.rem_euclid(LAYER_SIZE as i32) as usize;
    let (a, b) = if karst {
        if sandstone {
            (&p.stone_karst, &p.stone_karst)
        } else {
            (&p.sand_karst, &p.sand_karst)
        }
    } else if mafic {
        if sandstone {
            (&p.stone_mafic, &p.stone_mafic)
        } else {
            (&p.sand_mafic, &p.sand_mafic)
        }
    } else if sandstone {
        (&p.stone0, &p.stone1)
    } else {
        (&p.sand0, &p.sand1)
    };
    let selected = if p.thresholds[index] < p.style.sample(x as f64, z as f64) as f32 {
        a
    } else {
        b
    };
    selected[index].into()
}

fn local_sand(world: &World, x: i32, y: i32, z: i32) -> String {
    let rock = world.rock_at(x, y, z).unwrap_or_else(|| "granite".into());
    format!("tfc:sand/{}", super::sand_variant(&rock))
}

fn clamped_map(value: f64, in_min: f64, in_max: f64, out_min: f64, out_max: f64) -> f64 {
    let t = ((value - in_min) / (in_max - in_min)).clamp(0.0, 1.0);
    out_min + (out_max - out_min) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tfg_badlands_factory_modes_match_pinned_source() {
        assert_eq!(mode(id::BADLANDS).unwrap().soil_min_depth, -1);
        assert_eq!(mode(id::MESAS).unwrap().soil_min_depth, -3);
        assert_eq!(mode(id::HOODOOS).unwrap().soil_min_depth, -7);
        let warped = mode(id::WHORLED_CANYONS).unwrap();
        assert!(warped.inverted && warped.dipping);
        assert_eq!(warped.soil_min_depth, -20);
        assert!(mode(id::OCEAN).is_none());
        assert!(mode(id::BURREN_BADLANDS).is_none());
    }

    #[test]
    fn sandstone_palette_preserves_sandstone_and_mafic_tuff_states() {
        assert_eq!(sand_id("brown"), "tfc:sand/brown");
        assert_eq!(sandstone_id("brown"), "tfc:raw_sandstone/brown");
        let mut rng = Xoroshiro::new(0);
        let sands = fill_palette(&mut rng, "black", "tuff", "red", false);
        let stones = fill_palette(&mut rng, "black", "tuff", "red", true);
        assert!(sands.contains(&"tfg:tuff_gravel"));
        assert!(stones.contains(&"minecraft:tuff"));
        assert!(stones.contains(&"tfc:raw_sandstone/black"));
    }
}

/// Exposed cells below a carver opening retain the original strata rather than
/// receiving a new grass top after the carving stage.
pub(super) fn at_depth(
    world: &World,
    biome: u16,
    x: i32,
    y: i32,
    z: i32,
    cap: i32,
    rain: f64,
    temp: f64,
) -> Option<String> {
    let mode = mode(biome)?;
    if y == cap {
        return sample(world, biome, x, y, z, rain, temp);
    }
    let raw = world.height(x, z)?;
    let c = crate::geology::surface_biome_with_height(&world.region, world.seed, x, z, Some(raw))?;
    let config = with_palette(world.seed, Arc::clone);
    let grass = config.grass_height.sample(x as f64, z as f64);
    let variation = (1. - c.weight) * 23. + clamped_map(rain, 100., 500., 0., 22.);
    if !mode.inverted && f64::from(cap + 1 - 5) > grass - variation {
        return None;
    }
    let slope = crate::geology::surface_slope(&world.region, world.seed, x, z)?;
    let depth = super::depth::surface_depth(cap, slope, 5, mode.soil_min_depth).max(0);
    if mode.inverted && cap - y <= depth {
        let dry = local_sand(world, x, cap, z);
        let top = super::soil_top(world, biome, x, cap, z, rain, temp, Some(&dry));
        return Some(
            top.replace(":clay_grass/", ":clay/")
                .replace(":grass/", ":dirt/"),
        );
    }
    let rock = world.rock_at(x, 0, z);
    let karst = matches!(
        rock.as_deref(),
        Some("limestone" | "dolomite" | "chalk" | "marble")
    );
    let mafic = matches!(rock.as_deref(), Some("gabbro" | "basalt"));
    let offset = if mode.dipping {
        config.dipping_height.sample(x as f64, z as f64)
    } else {
        config.sand_height.sample(x as f64, z as f64)
    } as i32;
    let in_strata = if mode.inverted {
        let base = ((if mode.dipping { -10. } else { -16. }) + grass + variation) as i32;
        y >= base - 1
    } else {
        cap - y <= 3
    };
    if in_strata {
        return Some(sample_layer(
            &config,
            x,
            z,
            y + offset,
            karst,
            mafic,
            true,
            mode.dipping,
        ));
    }
    Some(super::builders::raw_rock(world, x, y, z))
}
