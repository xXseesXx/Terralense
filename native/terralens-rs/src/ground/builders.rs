//! TFG's non-generic top-surface builder selections.
//!
//! Ported from the pinned `TFGBiomes`, `ShoreAndOceanSurfaceBuilder`,
//! `TFGComplexSurfaceStates`, `TFGSimpleSurfaceStates`, `FlatsSurfaceBuilder`,
//! and `DuneSurfaceBuilder` sources in TFG Core Modern 0.9.25. This only returns
//! a state where the builder's material can be selected from the available map
//! context; otherwise the generic soil sampler remains responsible.
use super::{elevation_temperature, map_index, position_hash, sand_variant};
use crate::{biome::ids::id, noise::OpenSimplex2D, world::World};
use lru::LruCache;
use std::{cell::RefCell, num::NonZeroUsize, sync::OnceLock};

static SHORE_BEACH_NOISE: OnceLock<OpenSimplex2D> = OnceLock::new();
static SHORE_VARIANT_NOISE: OnceLock<OpenSimplex2D> = OnceLock::new();

struct SeededNoise {
    tide_broad: OpenSimplex2D,
    tide_detail: OpenSimplex2D,
    lowlands: OpenSimplex2D,
    lava_material: OpenSimplex2D,
    lava_flow: OpenSimplex2D,
}

impl SeededNoise {
    fn new(seed: i64) -> Self {
        let mut tide_broad = OpenSimplex2D::new(seed);
        tide_broad
            .octaves(3)
            .spread(f64::from(0.005_f32))
            .scaled(57.0, 69.0);
        let mut tide_detail = OpenSimplex2D::new(seed);
        tide_detail.spread(0.03);
        let mut lowlands = OpenSimplex2D::new(seed);
        lowlands.octaves(2).spread(f64::from(0.04_f32));
        let mut lava_material = OpenSimplex2D::new(seed);
        lava_material.octaves(2).spread(0.25);
        let mut lava_flow = OpenSimplex2D::new(seed.wrapping_add(23_891));
        lava_flow.spread(0.01);
        Self {
            tide_broad,
            tide_detail,
            lowlands,
            lava_material,
            lava_flow,
        }
    }
}

thread_local! {
    static SEEDED_NOISE: RefCell<LruCache<i64, SeededNoise>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(8).expect("nonzero cache size"))
    );
}

/// Return the specialized exposed material for biomes whose builder has a simple
/// deterministic material choice. `None` means the builder needs unavailable
/// per-column state (blend weight, slope, seeded badlands strata, etc.).
pub(super) fn sample(
    world: &World,
    biome: u16,
    x: i32,
    y: i32,
    z: i32,
    rainfall: f64,
    temperature: f64,
) -> Option<String> {
    use id::*;
    match biome {
        RIVER => river_gravel(world, x, y, z),
        ROCKY_PLATEAU => rocky_plateau_raw(world, x, y, z),
        ACTIVE_SHIELD_VOLCANO => {
            if let Some(block) = shield_volcano_override(world, x, y, z, true) {
                return Some(block);
            }
            let top = active_volcano_top(world.seed, x, y, z, rainfall, temperature);
            Some(top)
        }
        DORMANT_SHIELD_VOLCANO
        | EXTINCT_SHIELD_VOLCANO
        | ANCIENT_SHIELD_VOLCANO
        | SUNKEN_SHIELD_VOLCANO => {
            if let Some(block) = shield_volcano_override(world, x, y, z, false) {
                return Some(block);
            }
            Some(volcanic_top(x, y, z, rainfall, temperature, None))
        }
        CANYONS | DOLINE_CANYONS => {
            let dry = if position_hash(x, y, z, 728_275_914) & 127 > 48 {
                local_sand(world, x, y, z)
            } else {
                gravel_rock(world, x, y, z)
            };
            Some(volcanic_top(x, y, z, rainfall, temperature, Some(&dry)))
        }
        VOLCANIC_MOUNTAINS
        | VOLCANIC_MOUNTAIN_LAKE
        | VOLCANIC_OCEANIC_MOUNTAINS
        | VOLCANIC_OCEANIC_MOUNTAIN_LAKE => {
            let dry = gravel_rock(world, x, y, z);
            Some(volcanic_top(x, y, z, rainfall, temperature, Some(&dry)))
        }
        // Ocean builders apply mud below the tide floor, then a sand/gravel
        // beach selector in the shallow band. Their configured land builder
        // is also `SimpleSurfaceBuilder.OCEAN_MUD`, so it keeps mud above shore.
        DEEP_OCEAN_TRENCH | DEEP_OCEAN | OCEAN | OCEAN_REEF => {
            let tide = tide_level(world.seed, x, z);
            if ocean_surface_band(f64::from(y + 1), tide) {
                Some(shore_surface(world, x, y, z, rainfall, temperature))
            } else {
                Some("tfc:mud/silt".into())
            }
        }
        // A sand-builder beach uses the exact TFG sand color noise and the
        // climate + sea-level rock rare-color decision in TFGComplexSurfaceStates.
        SHORE | TIDAL_FLATS | SETBACK_CLIFFS | COASTAL_DUNES => {
            let tide = tide_level(world.seed, x, z);
            if f64::from(y + 1) <= tide + 6.0 {
                Some(shore_sand(world, x, y, z, rainfall, temperature))
            } else {
                None
            }
        }
        // The sea-cliff factory's shore state is the same sand/gravel selector;
        // it applies only through `tide + 2`, above which its land builder takes over.
        SEA_STACKS | TERRACE_UPPER | TERRACE_LOWER | EMBAYMENTS => {
            if f64::from(y + 1) <= tide_level(world.seed, x, z) + 2.0 {
                Some(shore_surface(world, x, y, z, rainfall, temperature))
            } else {
                None
            }
        }
        // The rocky-shore factory's underwater material is local gravel and its
        // dry material is local raw rock.
        ROCKY_SHORES | GUANO_ISLAND => Some(if y < 62 {
            gravel_rock(world, x, y, z)
        } else {
            raw_rock(world, x, y, z)
        }),
        // Oceanic mountain shores select gravel/raw, while land uses another
        // builder whose rough-slope behavior needs slope information.
        OCEANIC_MOUNTAINS | OCEANIC_MOUNTAIN_LAKE => {
            if f64::from(y + 1) <= tide_level(world.seed, x, z) + 2.0 {
                Some(gravel_rock(world, x, y, z))
            } else {
                None
            }
        }
        // Lowlands' noise+random mixer has an unavailable per-column RNG. Only
        // return the mud outcome when its 0.9*noise term guarantees that result.
        LOWLANDS | SALT_MARSH => lowlands_top(world, x, y, z, rainfall),
        // Glacial ice sheets use noise-derived base/surface heights, moraines,
        // snow, packed/blue ice and slope rules; leave those to saved ground or
        // the fallback rather than painting the whole biome as ice.
        // TFG's `DuneSurfaceBuilder` places the rock's sand-type state on the
        // top (except where its snowable wrapper selects snow).
        DUNE_SEA => Some(if snowable_sand(y, x, z, rainfall, temperature) {
            "minecraft:snow_block".into()
        } else {
            local_sand(world, x, y, z)
        }),
        // The muddy and salt flat builders choose fixed top blocks.
        MUD_FLATS => Some("tfg:hardened_clay".into()),
        SALT_FLATS => Some("tfg:halite".into()),
        // These surface types directly use TFG's volcanic shore state.
        SHIELD_VOLCANO_SHORE | OLD_SHIELD_VOLCANO_SHORE => {
            let sand_height = if biome == SHIELD_VOLCANO_SHORE {
                2.0
            } else {
                6.0
            };
            if f64::from(y + 1) <= tide_level(world.seed, x, z) + sand_height {
                Some(if rainfall > 420.0 {
                    "tfc:sand/green".into()
                } else {
                    "tfc:sand/black".into()
                })
            } else if biome == OLD_SHIELD_VOLCANO_SHORE {
                shield_volcano_override(world, x, y, z, true).or_else(|| {
                    Some(active_volcano_top(
                        world.seed,
                        x,
                        y,
                        z,
                        rainfall,
                        temperature,
                    ))
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// RockyPlateauSurfaceBuilder has a dedicated rock exposure branch. When the
/// upper surface start lies two blocks above the biome's small-hill field, the
/// builder uses RAW for its top, middle, and lower states. The manager supplies
/// WORLD_SURFACE_WG + 1 as `startY`, so compare that value rather than the solid
/// block Y passed to the material sampler.
fn rocky_plateau_raw(world: &World, x: i32, y: i32, z: i32) -> Option<String> {
    // The caller supplies the actual pre-carver solid cap, including density
    // openings. Raw terrain noise alone can retain a higher, carved-away roof.
    let start_y = y.saturating_add(1);
    if start_y.saturating_sub(2) as f64 > rocky_plateau_hills(world.seed, x, z) {
        Some(raw_rock(world, x, y, z))
    } else {
        None
    }
}

thread_local! {
    static ROCKY_PLATEAU_HILLS: RefCell<LruCache<i64, OpenSimplex2D>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(8).expect("nonzero cache size"))
    );
}

fn rocky_plateau_hills(seed: i64, x: i32, z: i32) -> f64 {
    ROCKY_PLATEAU_HILLS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.peek(&seed).is_none() {
            let mut hills = OpenSimplex2D::new(seed);
            hills
                .octaves(4)
                .spread(f64::from(0.05_f32))
                .scaled(85.0, 95.0);
            cache.put(seed, hills);
        }
        cache.get(&seed).unwrap().sample(f64::from(x), f64::from(z))
    })
}

/// TFGRiverSurfaceBuilder's explicit gravel branch. The original biome, not
/// the displayed/fuzzy river biome, controls sandy-bank eligibility.
fn river_gravel(world: &World, x: i32, y: i32, z: i32) -> Option<String> {
    static SANDY_RIVERS: OnceLock<[bool; 109]> = OnceLock::new();
    let sandy = SANDY_RIVERS.get_or_init(|| {
        let profiles: serde_json::Value =
            serde_json::from_str(include_str!("assets/surface-builders.json")).unwrap();
        std::array::from_fn(|i| {
            let name = crate::biome::ids::BIOME_IDS[i].rsplit('/').next().unwrap();
            let p = &profiles["biomes"][name];
            p["is_shore"].as_bool() == Some(false)
                && p["sandy_river_shores"].as_bool() == Some(true)
        })
    });
    let original = crate::geology::surface_biome_with_height(
        &world.region,
        world.seed,
        x,
        z,
        world.height(x, z),
    )?
    .original_biome;
    if !sandy[original as usize] {
        return None;
    }
    let slope = crate::geology::surface_slope(&world.region, world.seed, x, z)?;
    (slope >= 5. || y < 62).then(|| gravel_rock(world, x, y, z))
}

/// `ShoreAndOceanSurfaceBuilder` assigns mud below the tide floor and beach
/// material up through `tide + sandHeight`. The OCEAN factory's later
/// land-builder pass is also ocean mud.
fn ocean_surface_band(ocean_floor_y: f64, tide: f64) -> bool {
    if ocean_floor_y < tide - 5.0 {
        false // ocean mud
    } else if ocean_floor_y <= tide + 6.0 {
        true // beach state
    } else {
        false // land builder is also ocean mud
    }
}

fn shore_sand(world: &World, x: i32, _y: i32, z: i32, rainfall: f64, temperature: f64) -> String {
    let value = shore_variant_noise().sample(x as f64, z as f64).abs() as f32;
    if value > 0.55 {
        if rainfall > 300.0 && temperature > 15.0 {
            "tfc:sand/pink".into()
        } else if is_mafic(world, x, z) {
            if rainfall > 300.0 {
                "tfc:sand/green".into()
            } else {
                "tfc:sand/black".into()
            }
        } else {
            "tfc:sand/white".into()
        }
    } else if value > 0.2 {
        "tfc:sand/yellow".into()
    } else if value > 0.1 {
        "tfc:sand/brown".into()
    } else {
        "tfc:sand/red".into()
    }
}

fn shore_surface(world: &World, x: i32, y: i32, z: i32, rainfall: f64, temperature: f64) -> String {
    if f64::from(shore_beach_noise().sample(x as f64, z as f64) as f32)
        > lerp_clamped(f64::from(temperature as f32), -15.0, 25.0, -0.7, 0.7)
    {
        gravel_rock(world, x, y, z)
    } else {
        shore_sand(world, x, y, z, rainfall, temperature)
    }
}

fn tide_level(seed: i64, x: i32, z: i32) -> f64 {
    // TFGBiomeNoise.shoreTideLevelNoise: scaled octave field, clamped to 63..67,
    // plus a small-scale additive OpenSimplex field. Sea level is 63 in TFC.
    with_seeded_noise(seed, |noise| {
        noise
            .tide_broad
            .sample(x as f64, z as f64)
            .clamp(63.0, 67.0)
            + noise.tide_detail.sample(x as f64, z as f64)
    }) as i32 as f64
}

fn shore_beach_noise() -> &'static OpenSimplex2D {
    SHORE_BEACH_NOISE.get_or_init(|| {
        let mut noise = OpenSimplex2D::new(124_154);
        noise.octaves(3).spread(f64::from(0.00002_f32));
        noise
    })
}

fn shore_variant_noise() -> &'static OpenSimplex2D {
    SHORE_VARIANT_NOISE.get_or_init(|| {
        let mut noise = OpenSimplex2D::new(36_263_276);
        noise.octaves(5).spread(f64::from(0.0003_f32));
        noise
    })
}

fn with_seeded_noise<T>(seed: i64, f: impl FnOnce(&SeededNoise) -> T) -> T {
    SEEDED_NOISE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.peek(&seed).is_none() {
            cache.put(seed, SeededNoise::new(seed));
        }
        f(cache.get(&seed).expect("seed noise was inserted"))
    })
}

fn lowlands_top(world: &World, x: i32, y: i32, z: i32, rainfall: f64) -> Option<String> {
    // Java SurfaceBuilderContext.random() contributes 0..0.1 after the 0.9 noise
    // multiplier. A nonnegative noise term guarantees the mud branch; the gray
    // interval is deliberately returned as unknown rather than guessed.
    let material_noise = with_seeded_noise(world.seed, |noise| {
        f64::from(noise.lowlands.sample(x as f64, z as f64) as f32 * 0.9_f32)
    });
    if material_noise >= 0.0 {
        Some(if rainfall < 130.0 {
            "tfg:hardened_clay".into()
        } else {
            classic_mud(world, x, y, z, rainfall)
        })
    } else {
        None
    }
}

fn active_volcano_top(seed: i64, x: i32, y: i32, z: i32, rainfall: f64, temp: f64) -> String {
    let (flow_value, material_value) = active_volcano_flow(seed, x, z);
    if flow_value < 0.40 {
        volcanic_top(x, y, z, rainfall, temp, None)
    } else if flow_value < 0.50 {
        if material_value > 0.0 {
            snowable_basalt(y, x, z, rainfall, temp, "tfc:rock/gravel/basalt")
        } else {
            volcanic_top(x, y, z, rainfall, temp, None)
        }
    } else if flow_value < 0.75 {
        if material_value > 0.0 {
            snowable_basalt(y, x, z, rainfall, temp, "tfc:rock/gravel/basalt")
        } else {
            snowable_basalt(y, x, z, rainfall, temp, "tfc:rock/cobble/basalt")
        }
    } else if material_value > -0.6 {
        snowable_basalt(y, x, z, rainfall, temp, "tfc:rock/raw/basalt")
    } else {
        snowable_basalt(y, x, z, rainfall, temp, "tfc:rock/cobble/basalt")
    }
}

fn active_volcano_flow(seed: i64, x: i32, z: i32) -> (f64, f64) {
    with_seeded_noise(seed, |noise| {
        (
            1.0 - 2.0 * noise.lava_flow.sample(x as f64, z as f64).abs(),
            noise.lava_material.sample(x as f64, z as f64),
        )
    })
}

/// Apply the top-layer branch from TFG's active/dormant shield surface
/// builders. Both builders use `calculateAltitudeSlopeSurfaceDepth`; on steep
/// land they expose the selected under-state or raw basalt. Underwater columns
/// use the flow branch's own under-water state.
fn shield_volcano_override(world: &World, x: i32, y: i32, z: i32, active: bool) -> Option<String> {
    let slope = crate::geology::surface_slope(&world.region, world.seed, x, z)?;
    let (under, under_water) = if active {
        let (flow, material) = active_volcano_flow(world.seed, x, z);
        active_volcano_understates(flow, material)
    } else {
        ("tfc:rock/gravel/basalt", "tfc:rock/gravel/basalt")
    };
    shield_volcano_top_override(y, slope, under, under_water).map(Into::into)
}

fn active_volcano_understates(flow: f64, material: f64) -> (&'static str, &'static str) {
    if flow < 0.40 || flow < 0.50 && material <= 0.0 {
        ("tfc:rock/gravel/basalt", "tfc:rock/gravel/basalt")
    } else if flow < 0.50 || flow < 0.75 && material > 0.0 {
        ("tfc:rock/raw/basalt", "tfc:rock/gravel/basalt")
    } else {
        ("tfc:rock/raw/basalt", "tfc:rock/cobble/basalt")
    }
}

fn shield_volcano_top_override(
    y: i32,
    slope: f64,
    under: &'static str,
    under_water: &'static str,
) -> Option<&'static str> {
    let underwater = y < 62; // source tests `surfaceY < seaLevel - 1`
    let min_depth = if underwater { -1 } else { -3 };
    let depth = crate::ground::depth::surface_depth(y, slope, 5, min_depth);
    if depth < -1 {
        Some("tfc:rock/raw/basalt")
    } else if underwater {
        Some(under_water)
    } else if depth == -1 {
        Some(under)
    } else {
        None
    }
}

fn snowable_basalt(y: i32, x: i32, z: i32, rainfall: f64, temp: f64, normal: &str) -> String {
    let rain_i = map_index(rainfall, 35.0, 450.0, 3.0, 17.99, 18);
    let temp_i = map_index(elevation_temperature(temp, y), -19.0, -4.0, 0.0, 17.99, 18);
    match rain_i.min(temp_i) {
        0 | 1 => "minecraft:snow_block".into(),
        2 if position_hash(x, y, z, 729_375_982) & 127 > 63 => "minecraft:snow_block".into(),
        _ => normal.into(),
    }
}

pub(super) fn volcanic_top(
    x: i32,
    y: i32,
    z: i32,
    rainfall: f64,
    temp: f64,
    dry: Option<&str>,
) -> String {
    let rain_i = map_index(rainfall, 35.0, 450.0, 3.0, 17.99, 18);
    let temp_i = map_index(elevation_temperature(temp, y), -19.0, -4.0, 0.0, 17.99, 18);
    match rain_i.min(temp_i) {
        0 | 1 => "minecraft:snow_block".into(),
        2 if position_hash(x, y, z, 729_375_982) & 127 > 63 => "minecraft:snow_block".into(),
        2 | 3 => dry
            .map(str::to_owned)
            .unwrap_or_else(|| basalt_sand_gravel(x, y, z)),
        4 if position_hash(x, y, z, 729_375_982) & 127 > 63 => dry
            .map(str::to_owned)
            .unwrap_or_else(|| basalt_sand_gravel(x, y, z)),
        4 => "tfg:coarse_dirt/mollisol".into(),
        5 => "tfg:coarse_dirt/mollisol".into(),
        6 if position_hash(x, y, z, 729_375_982) & 127 > 63 => "tfg:coarse_dirt/sandy_loam".into(),
        6 => "tfg:grass/mollisol".into(),
        _ => "tfg:grass/mollisol".into(),
    }
}

fn classic_mud(world: &World, x: i32, y: i32, z: i32, rainfall: f64) -> String {
    let index = map_index(rainfall, 20., 500., 0., 8.99, 9);
    let mixed = super::patch_noise(x, z) > 0.;
    let soil = match index {
        0 => return local_sand(world, x, y, z),
        1 | 2 => "sandy_loam",
        3 if mixed => "sandy_loam",
        3 | 4 => "loam",
        5 if mixed => "loam",
        5 | 6 => "silty_loam",
        7 if mixed => "silty_loam",
        _ => "silt",
    };
    format!("tfc:mud/{soil}")
}

fn basalt_sand_gravel(x: i32, y: i32, z: i32) -> String {
    if position_hash(x, y, z, 728_275_914) & 127 > 48 {
        "tfc:rock/gravel/basalt".into()
    } else {
        "tfc:sand/black".into()
    }
}

fn snowable_sand(y: i32, x: i32, z: i32, rainfall: f64, temperature: f64) -> bool {
    // BuildSnowableSurface has 18 entries: snow, snow, hash transition, then
    // typical material. This mirrors the same rainfall/elevation temperature banding.
    let rain_i = map_index(rainfall, 35.0, 450.0, 3.0, 17.99, 18);
    let temp_i = map_index(
        elevation_temperature(temperature, y),
        -19.0,
        -4.0,
        0.0,
        17.99,
        18,
    );
    match rain_i.min(temp_i) {
        0 | 1 => true,
        2 => position_hash(x, y, z, 729_375_982) & 127 > 63,
        _ => false,
    }
}

fn is_mafic(world: &World, x: i32, z: i32) -> bool {
    matches!(
        world.rock_at(x, 63, z).as_deref(),
        Some("gabbro" | "basalt")
    )
}

fn local_sand(world: &World, x: i32, y: i32, z: i32) -> String {
    let rock = world.rock_at(x, y, z).unwrap_or_else(|| "granite".into());
    format!("tfc:sand/{}", sand_variant(&rock))
}

fn gravel_rock(world: &World, x: i32, y: i32, z: i32) -> String {
    let rock = world.rock_at(x, y, z).unwrap_or_else(|| "granite".into());
    format!("tfc:rock/gravel/{rock}")
}

pub(super) fn raw_rock(world: &World, x: i32, y: i32, z: i32) -> String {
    let rock = world.rock_at(x, y, z).unwrap_or_else(|| "granite".into());
    format!("tfc:rock/raw/{rock}")
}

fn lerp_clamped(v: f64, a: f64, b: f64, min: f64, max: f64) -> f64 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    min + (max - min) * t
}

#[cfg(test)]
mod tests {
    use super::{active_volcano_understates, ocean_surface_band, shield_volcano_top_override};

    #[test]
    fn ocean_surface_builder_uses_ocean_mud_as_its_land_builder() {
        // TFG Core 0.9.25 ShoreAndOceanSurfaceBuilder uses `< tide - 5`
        // for underwater mud and `<= tide + sandHeight` (6 for these
        // standard oceans) for the beach pass. The OCEAN factory's landBuilder
        // is `SimpleSurfaceBuilder.OCEAN_MUD`, so high columns also become mud.
        assert!(!ocean_surface_band(57.0, 63.0));
        assert!(ocean_surface_band(58.0, 63.0));
        assert!(ocean_surface_band(69.0, 63.0));
        assert!(!ocean_surface_band(70.0, 63.0));
    }

    #[test]
    fn shield_volcano_surface_uses_the_source_altitude_slope_gate() {
        assert_eq!(
            shield_volcano_top_override(90, 0.0, "under", "underwater"),
            None
        );
        assert_eq!(
            shield_volcano_top_override(90, 15.0, "under", "underwater"),
            Some("tfc:rock/raw/basalt")
        );
        assert_eq!(
            shield_volcano_top_override(50, 0.0, "under", "underwater"),
            Some("underwater")
        );
        assert_eq!(
            shield_volcano_top_override(50, 15.0, "under", "underwater"),
            Some("underwater")
        );
        assert_eq!(
            shield_volcano_top_override(80, 12.0, "under", "underwater"),
            Some("under")
        );
    }

    #[test]
    fn active_shield_understates_follow_the_lava_flow_material_branches() {
        assert_eq!(
            active_volcano_understates(0.2, 1.0),
            ("tfc:rock/gravel/basalt", "tfc:rock/gravel/basalt")
        );
        assert_eq!(
            active_volcano_understates(0.45, 1.0),
            ("tfc:rock/raw/basalt", "tfc:rock/gravel/basalt")
        );
        assert_eq!(
            active_volcano_understates(0.65, -1.0),
            ("tfc:rock/raw/basalt", "tfc:rock/cobble/basalt")
        );
        assert_eq!(
            active_volcano_understates(0.9, 0.0),
            ("tfc:rock/raw/basalt", "tfc:rock/cobble/basalt")
        );
    }
}
