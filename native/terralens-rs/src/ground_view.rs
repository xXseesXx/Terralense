//! Shared ground data for map rasters, hover probes and detailed terrain.
use crate::{
    dimension::Dimension,
    ground, ground_palette,
    model::{Climate, Probe},
    world::World,
};
use lru::LruCache;
use std::{cell::RefCell, collections::HashMap, num::NonZeroUsize, sync::Arc};

#[derive(Clone, Debug)]
pub(crate) struct Ground {
    pub y: i32,
    pub block: Arc<str>,
    pub saved: bool,
}

type SavedTile = Option<Arc<Vec<Option<Ground>>>>;
thread_local! {
    // Only saved data is cached here: its identity depends on the save revision,
    // while procedural soil rules have their own seed/settings-dependent inputs.
    static SAVED: RefCell<LruCache<(u64, i32, i32), SavedTile>> =
        RefCell::new(LruCache::new(NonZeroUsize::new(128).unwrap()));
}

pub(crate) fn sample_with_options(
    world: &World,
    x: i32,
    z: i32,
    use_saved: bool,
    options: &crate::display::TerrainDetailOptions,
) -> Option<Ground> {
    sample_inner(world, x, z, use_saved, true, options)
}

fn sample_inner(
    world: &World,
    x: i32,
    z: i32,
    use_saved: bool,
    fallback: bool,
    options: &crate::display::TerrainDetailOptions,
) -> Option<Ground> {
    if world.dimension != Dimension::Overworld {
        return None;
    }
    if use_saved && world.surface_source_id() != 0 {
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        let key = (world.surface_source_id(), cx, cz);
        let cached = SAVED.with(|cache| cache.borrow_mut().get(&key).cloned());
        let tile = cached.unwrap_or_else(|| {
            let tile = world.saved_ground(cx, cz).map(|chunk| {
                let mut names = HashMap::<&str, Arc<str>>::new();
                Arc::new(
                    chunk
                        .columns
                        .iter()
                        .map(|column| {
                            (!column.block.is_empty()).then(|| Ground {
                                y: column.y,
                                block: names
                                    .entry(&column.block)
                                    .or_insert_with(|| Arc::from(column.block.as_str()))
                                    .clone(),
                                saved: true,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            });
            SAVED.with(|cache| cache.borrow_mut().put(key, tile.clone()));
            tile
        });
        if let Some(tile) = tile {
            let index = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
            if let Some(Some(ground)) = tile.get(index) {
                return Some(ground.clone());
            }
        }
    }
    let surface = if fallback {
        ground::sample_with_options(world, x, z, options)
    } else {
        None
    };
    surface.map(|sample| Ground {
        y: sample.y,
        block: sample.block,
        saved: false,
    })
}

pub(crate) fn populate(world: &World, x: i32, z: i32, probe: &mut Probe, use_saved: bool) {
    populate_with_options(
        world,
        x,
        z,
        probe,
        use_saved,
        &crate::display::TerrainDetailOptions::full(),
    );
}

pub(crate) fn populate_with_options(
    world: &World,
    x: i32,
    z: i32,
    probe: &mut Probe,
    use_saved: bool,
    options: &crate::display::TerrainDetailOptions,
) {
    populate_at_scale_with_options(world, x, z, probe, use_saved, true, options)
}

pub(crate) fn populate_at_scale_with_options(
    world: &World,
    x: i32,
    z: i32,
    probe: &mut Probe,
    use_saved: bool,
    detail: bool,
    options: &crate::display::TerrainDetailOptions,
) {
    let was_bare = probe.is_bare_surface();
    let mut sample_options = *options;
    if !detail {
        sample_options.physical_surface = false;
        sample_options.caves = false;
        sample_options.rivers_lakes = false;
        sample_options.volcanoes = false;
        sample_options.springs = false;
        sample_options.exposed_dikes = false;
        sample_options.large_rocks_special_soils = false;
    }
    // Distant samples still need their inexpensive surface-builder cap. The
    // detail flag controls expensive stages, not whether ground is sampled.
    let ground = sample_inner(world, x, z, use_saved, !was_bare, &sample_options);
    let Some(ground) = ground else {
        return;
    };
    apply_ground(probe, ground);
}

/// Categorical water-biome colors need saved fluid corrections, but no material
/// prediction. Avoid generating soil patches merely to paint a covered lake.
pub(crate) fn populate_saved(world: &World, x: i32, z: i32, probe: &mut Probe) {
    if world.surface_source_id() != 0
        && let Some(ground) = sample_inner(
            world,
            x,
            z,
            true,
            false,
            &crate::display::TerrainDetailOptions::fast(),
        )
    {
        apply_ground(probe, ground);
    }
}

fn apply_ground(probe: &mut Probe, ground: Ground) {
    probe.ground_block = Some(ground.block.to_string());
    probe.ground_y = Some(ground.y);
    probe.ground_saved = ground.saved;
    if ground.saved {
        // A saved substrate column also tells us whether its visible surface is
        // fluid. Never flood known dry ground using the estimated noise height.
        probe.surface_lava = None;
        probe.surface_magma = None;
        probe.surface_water_y = None;
        probe.surface_water = false;
        match crate::volcanic_lava::Surface::from_block(&ground.block, ground.y) {
            Some(crate::volcanic_lava::Surface::Lava(y)) => {
                probe.surface_lava = Some(y);
                probe.ocean = false;
            }
            Some(crate::volcanic_lava::Surface::Magma(y)) => {
                probe.surface_magma = Some(y);
                probe.ocean = false;
            }
            Some(crate::volcanic_lava::Surface::Water(y)) => {
                probe.surface_water = true;
                probe.surface_water_y = Some(y);
            }
            None => probe.ocean = false,
        }
    }
}

pub(crate) fn color(block: &str, climate: &Climate, y: i32) -> Option<[u8; 3]> {
    if ground_palette::is_grass_soil(block) {
        Some(ground_palette::multiply_tint(
            ground_palette::grass_tint_overlay().unwrap_or([255; 3]),
            crate::map_colors::grass(climate.temperature, climate.rainfall, y),
        ))
    } else {
        ground_palette::color(block)
    }
}

pub(crate) fn probe_color(probe: &Probe) -> Option<[u8; 3]> {
    color(
        probe.ground_block.as_deref()?,
        &probe.climate,
        probe.ground_y.unwrap_or(63),
    )
}

/// Recolor existing ground data without changing its material or height.
pub(crate) fn probe_color_at(
    probe: &Probe,
    temperature_scale: f64,
    season: Option<f32>,
) -> Option<[u8; 3]> {
    if season.is_none() {
        return probe_color(probe);
    }
    let block = probe.ground_block.as_deref()?;
    if !ground_palette::is_grass_soil(block) {
        return ground_palette::color(block);
    }
    Some(ground_palette::multiply_tint(
        ground_palette::grass_tint_overlay().unwrap_or([255; 3]),
        crate::map_colors::grass_at(
            probe.climate.temperature,
            probe.climate.rainfall,
            probe.point.x.floor() as i32,
            probe.ground_y.unwrap_or(63),
            probe.point.z.floor() as i32,
            temperature_scale,
            season,
        ),
    ))
}

/// Visible solid surface for material-map relief, separate from the generator's
/// raw height input used by water, biomes, ores and structure placement.
#[cfg(test)]
pub(crate) fn relief_height(world: &World, x: i32, z: i32, terrain: Option<f64>) -> Option<f64> {
    relief_height_with_options(
        world,
        x,
        z,
        terrain,
        &crate::display::TerrainDetailOptions::full(),
    )
}

pub(crate) fn relief_height_with_options(
    world: &World,
    x: i32,
    z: i32,
    terrain: Option<f64>,
    options: &crate::display::TerrainDetailOptions,
) -> Option<f64> {
    if !options.physical_surface {
        return terrain;
    }
    let terrain = (options.physical_surface && options.caves)
        .then(|| crate::caves::column(world, x, z))
        .flatten()
        .map(|c| f64::from(c.solid_y))
        .or(terrain);
    if options.physical_surface
        && (options.volcanoes || options.springs)
        && world.volcanic_surface_with_options(x, z, options).is_some()
    {
        return terrain;
    }
    let Some(ground) = sample_with_options(world, x, z, true, options) else {
        return terrain;
    };
    if crate::volcanic_lava::Surface::from_block(&ground.block, ground.y).is_some()
        || !ground.saved
            && options.physical_surface
            && options.rivers_lakes
            && world.is_surface_water_with_options(x, z, options)
    {
        terrain
    } else {
        Some(f64::from(ground.y))
    }
}

/// Natural spring excavation affects relief even without the Ground layer.
/// Only positive spring columns consult saved substrate, keeping normal map
/// shading and distant views free of additional save scans.
pub(crate) fn spring_relief_height_with_options(
    world: &World,
    x: i32,
    z: i32,
    terrain: Option<f64>,
    options: &crate::display::TerrainDetailOptions,
) -> Option<f64> {
    if !options.physical_surface {
        return terrain;
    }
    let terrain = (options.physical_surface && options.caves)
        .then(|| crate::caves::column(world, x, z))
        .flatten()
        .map(|column| f64::from(column.solid_y))
        .or(terrain);
    if !(options.physical_surface && options.springs) {
        return terrain;
    }
    let Some(spring) = crate::hot_springs::sample(world, x, z) else {
        return terrain;
    };
    if crate::volcanic_lava::Surface::from_block(&spring.block, spring.y).is_some() {
        return terrain;
    }
    let Some(ground) = sample_with_options(world, x, z, true, options) else {
        return terrain;
    };
    if crate::volcanic_lava::Surface::from_block(&ground.block, ground.y).is_some() {
        terrain
    } else {
        Some(f64::from(ground.y))
    }
}
