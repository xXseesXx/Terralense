//! A biome-only raster never asks the terrain/decoration generators for data.
use super::*;

pub(super) struct FastBiomes {
    colors: Vec<Color32>,
    precise: bool,
}

impl FastBiomes {
    pub(super) fn new(world: &World, fallback: Layer, display: &DisplayPrefs) -> Option<Self> {
        if world.dimension != Dimension::Overworld
            || display.grass_colors
            || display.trees
            || display.height_shading
            || display.contours.is_active()
            || display.temperature_enabled
            || display.rainfall_enabled
            || !display.highlighted_rocks.is_empty()
        {
            return None;
        }
        let mut terrain = display.terrain;
        terrain.precise_biome_boundaries = false;
        if terrain != TerrainDetailOptions::fast() {
            return None;
        }
        let layers = display.active_layers(fallback, world.dimension);
        if layers.len() != 1 || layers[0].layer != Layer::Biomes || layers[0].opacity != 1. {
            return None;
        }
        let mut plain = display.clone();
        // This is a biome classification map. Calendar colors belong to actual
        // grass/leaves/fluid surfaces and must not pull climate into this path.
        plain.season = None;
        let colors = (0..biome::ids::BIOME_IDS.len())
            .map(|id| {
                let id = id as u16;
                let probe = Probe {
                    biome: biome::biome_name(id).to_owned(),
                    ocean: biome::is_ocean(id),
                    surface_water: crate::surface_water::is_inland_water_biome(id),
                    ..Probe::default()
                };
                display_color(world, Layer::Biomes, id, &probe, &plain, 0, 0)
            })
            .collect();
        Some(Self {
            colors,
            precise: display.terrain.precise_biome_boundaries,
        })
    }

    pub(super) fn id(&self, world: &World, x: i32, z: i32) -> u16 {
        if self.precise {
            map_biome_id(world, x, z)
        } else {
            world.biome_id(x, z)
        }
    }

    pub(super) fn color(&self, id: u16) -> Color32 {
        self.colors
            .get(id as usize)
            .copied()
            .unwrap_or(Color32::GRAY)
    }
}

pub(super) fn render_fast_biomes(
    world: &World,
    key: TileKey,
    palette: &FastBiomes,
    cancel: &impl Fn() -> bool,
) -> Option<CpuTile> {
    let resolution = if key.detail { DETAIL } else { COARSE };
    let side = resolution + 2;
    let cache_key = HeightGridKey::new(world, key).with_biomes(palette.precise);
    let cached = BIOME_GRID_CACHE.access(|cache| cache.get(&cache_key).cloned());
    let mut grid = cached.is_none().then(|| vec![0; side * side]);
    let mut pixels = vec![Color32::TRANSPARENT; side * side];
    let mut biomes = vec![0; resolution * resolution];
    let mut water = vec![false; resolution * resolution];
    for (i, &(x, z)) in tile_sample_order(key.detail).iter().enumerate() {
        if i % 32 == 0 && cancel() {
            return None;
        }
        let offset = z * side + x;
        let id = if let Some(cache) = &cached {
            cache[offset]
        } else {
            let (bx, bz) = tile_sample(key, resolution, x as i32 - 1, z as i32 - 1);
            palette.id(world, bx, bz)
        };
        if let Some(grid) = &mut grid {
            grid[offset] = id;
        }
        pixels[offset] = palette.color(id);
        if (1..=resolution).contains(&x) && (1..=resolution).contains(&z) {
            let index = (z - 1) * resolution + x - 1;
            biomes[index] = id;
            water[index] = biome::is_ocean(id) || crate::surface_water::is_inland_water_biome(id);
        }
    }
    if cancel() {
        return None;
    }
    if let Some(grid) = grid {
        BIOME_GRID_CACHE.access(|cache| cache.put(cache_key, Arc::new(grid)));
    }
    Some(CpuTile {
        key,
        resolution,
        pixels,
        biomes,
        surface_water: water,
    })
}

pub(super) fn overlay_litter(
    world: &World,
    base: Color32,
    x: i32,
    z: i32,
    step: f64,
    display: &DisplayPrefs,
    bare: bool,
    cancel: &impl Fn() -> bool,
) -> Color32 {
    if !display.terrain.surface_litter
        || step > 4.
        || bare
        || world.dimension != Dimension::Overworld
    {
        return base;
    }
    let litter = if let Some(saved) = world.saved_litter(x.div_euclid(16), z.div_euclid(16)) {
        saved
            .get(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize)
            .cloned()
    } else {
        crate::surface_litter::sample_predicted(world, x, z, cancel)
    };
    let Some(litter) = litter else {
        return base;
    };
    let rgb = crate::surface_litter::composite_at(
        [base.r(), base.g(), base.b()],
        world,
        &litter,
        x,
        z,
        display.simulated_season(),
    );
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_tiles_and_exports_do_not_query_terrain_even_after_full_mode() {
        let world = World::new(0, Settings::default());
        let key = TileKey {
            x: 53,
            z: 55,
            zoom: 0,
            detail: false,
            layer: Layer::Biomes,
        };
        let fast = DisplayPrefs::default();
        assert_eq!(fast.terrain, TerrainDetailOptions::fast());
        crate::world::reset_test_query_counts();
        let first = render_tile(&world, key, &fast, || false).unwrap();
        assert_eq!(crate::world::test_query_counts(), (0, 0, 0));
        let raster = RasterSampler::new(&world, key.layer, &fast, [4., 4.]);
        for (x, z) in [(0, 0), (12, 27), (COARSE - 1, COARSE - 1)] {
            let (bx, bz) = tile_sample(key, COARSE, x as i32, z as i32);
            assert_eq!(
                first.pixels[(z + 1) * (COARSE + 2) + x + 1].to_array(),
                raster.color(bx, bz, &|| false)
            );
        }
        assert_eq!(crate::world::test_query_counts(), (0, 0, 0));
        let full = DisplayPrefs {
            terrain: TerrainDetailOptions::full(),
            ..fast.clone()
        };
        let detailed = render_tile(&world, key, &full, || false).unwrap();
        assert!(crate::world::test_query_counts().0 > 0);
        crate::world::reset_test_query_counts();
        let second = render_tile(&world, key, &fast, || false).unwrap();
        assert_eq!(first.pixels, second.pixels);
        assert_eq!(first.biomes, second.biomes);
        assert_eq!(crate::world::test_query_counts(), (0, 0, 0));
        assert_ne!(
            first.pixels, detailed.pixels,
            "volcanic detail must remain selectable"
        );
    }

    #[test]
    fn fast_cancellation_does_not_publish_partial_biome_grids() {
        let world = World::new(912_867, Settings::default());
        let key = TileKey {
            x: -3,
            z: 2,
            zoom: 1,
            detail: false,
            layer: Layer::Biomes,
        };
        let display = DisplayPrefs::default();
        let palette = FastBiomes::new(&world, key.layer, &display).unwrap();
        let cache_key = HeightGridKey::new(&world, key).with_biomes(false);
        BIOME_GRID_CACHE.access(|cache| {
            cache.pop(&cache_key);
        });
        assert!(render_fast_biomes(&world, key, &palette, &|| true).is_none());
        assert!(!BIOME_GRID_CACHE.access(|cache| cache.contains(&cache_key)));
    }

    #[test]
    fn fast_map_keeps_water_distinct_and_preserves_biome_highlights() {
        let world = World::new(0, Settings::default());
        let mut display = DisplayPrefs::default();
        display.highlighted_biomes = vec![crate::biome::ids::id::PLATEAU];
        let palette = FastBiomes::new(&world, Layer::Biomes, &display).unwrap();
        let ocean = crate::biome::ids::id::OCEAN;
        let river = crate::biome::ids::id::RIVER;
        assert_ne!(
            palette.color(ocean),
            palette.color(crate::biome::ids::id::PLATEAU)
        );
        assert_ne!(
            palette.color(river),
            palette.color(crate::biome::ids::id::PLATEAU)
        );
        let plain = FastBiomes::new(&world, Layer::Biomes, &DisplayPrefs::default()).unwrap();
        assert_eq!(palette.color(ocean), plain.color(ocean));
        assert_ne!(
            palette.color(crate::biome::ids::id::PLATEAU),
            plain.color(crate::biome::ids::id::PLATEAU)
        );
    }

    #[test]
    fn terrain_quality_does_not_throw_away_completed_structure_scans() {
        let mut atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        let key = FeatureTileKey {
            x: 0,
            z: 0,
            tier: 0,
            scale: 0,
            ores: true,
            structures: true,
        };
        let marker = Marker {
            id: "test:structure".into(),
            label: "Structure".into(),
            namespace: "test".into(),
            x: 12.,
            z: 14.,
            y: None,
            kind: MarkerKind::Structure,
            confirmed: false,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [180; 3],
            bounds: None,
        };
        atlas.cache_feature_result(key, vec![marker.clone()], false);
        let mut full = atlas.spec.display.clone();
        full.apply_terrain_preset(crate::display::TerrainPreset::Full);
        atlas.set_display_options(&full);
        assert_eq!(
            atlas.feature_tiles.get(&key).unwrap().markers[0].id,
            marker.id
        );
        let mut fast = full;
        fast.apply_terrain_preset(crate::display::TerrainPreset::Fast);
        atlas.set_display_options(&fast);
        assert_eq!(
            atlas.feature_tiles.get(&key).unwrap().markers[0].id,
            marker.id
        );
    }

    #[test]
    fn basic_hover_skips_terrain_but_explicit_destination_keeps_actual_height() {
        let world = World::new(0, Settings::default());
        let display = DisplayPrefs::default();
        crate::world::reset_test_query_counts();
        let hover = sample_probe_with_display(&world, -319262, 16107, false, &display);
        assert_eq!(crate::world::test_query_counts(), (0, 0, 0));
        assert!(hover.height.is_none() && hover.ground_y.is_none());
        let destination = sample_probe_with_display(&world, -319262, 16107, true, &display);
        assert_eq!(destination.ground_y, Some(87));
        assert_eq!(
            crate::exports::destination_y(
                destination.point,
                Some(&destination),
                Dimension::Overworld,
                96,
                None
            ),
            Some(88)
        );
    }

    #[test]
    fn known_empty_surface_masks_skip_sampler_calls_on_another_layer() {
        let world = World::new(9_128_341, Settings::default());
        let key = TileKey {
            x: 0,
            z: 0,
            zoom: 0,
            detail: false,
            layer: Layer::Biomes,
        };
        let mut display = DisplayPrefs::default();
        display.terrain.physical_surface = true;
        display.terrain.springs = true;
        let mask_key = HeightGridKey::new(&world, key).with_terrain(&display.terrain);
        VOLCANIC_SURFACE_CACHE.access(|cache| {
            cache.pop(&mask_key);
        });
        crate::world::reset_test_query_counts();
        render_tile(&world, key, &display, || false).unwrap();
        assert!(crate::world::test_query_counts().1 > 0);
        assert!(VOLCANIC_SURFACE_CACHE.access(|cache| cache.contains(&mask_key)));
        crate::world::reset_test_query_counts();
        render_tile(
            &world,
            TileKey {
                layer: Layer::Rainfall,
                ..key
            },
            &display,
            || false,
        )
        .unwrap();
        assert_eq!(
            crate::world::test_query_counts().1,
            0,
            "cached empty cells use Some(None), not a new surface probe"
        );
        let mut precise = display.terrain;
        precise.precise_biome_boundaries = true;
        assert_ne!(
            mask_key,
            HeightGridKey::new(&world, key).with_terrain(&precise)
        );
    }
}
