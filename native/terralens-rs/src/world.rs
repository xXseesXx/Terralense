//! Worker-local TFG samplers reuse shared immutable region data.
use crate::{
    biome,
    dimension::Dimension,
    geology,
    model::{Climate, Point, Probe, Settings, TreeEstimate},
    region::Generator,
};
use std::{cell::RefCell, num::NonZeroUsize};
use std::{
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

#[cfg(test)]
thread_local! { static TEST_QUERY_COUNTS: std::cell::Cell<(u64,u64,u64)> = const { std::cell::Cell::new((0,0,0)) }; }
#[cfg(test)]
pub(crate) fn count_test_cave_query() {
    TEST_QUERY_COUNTS.with(|c| {
        let (h, v, k) = c.get();
        c.set((h, v, k + 1));
    });
}
#[cfg(test)]
pub(crate) fn reset_test_query_counts() {
    TEST_QUERY_COUNTS.with(|c| c.set((0, 0, 0)));
}
#[cfg(test)]
pub(crate) fn test_query_counts() -> (u64, u64, u64) {
    TEST_QUERY_COUNTS.with(|c| c.get())
}

pub struct World {
    pub seed: i64,
    pub region: Generator,
    pub dimension: Dimension,
    pub slice_y: i32,
    biome_zoom_seed: i64,
    beneath: Option<crate::beneath::Beneath>,
    planetary: Option<crate::planetary::Planetary>,
    volcanic_lava: Option<crate::volcanic_lava::Sampler>,
    surface_source: Option<PathBuf>,
    surface_source_id: u64,
    saved_surfaces: RefCell<lru::LruCache<(i32, i32), Option<Arc<crate::prism::SurfaceChunk>>>>,
    saved_grounds: RefCell<lru::LruCache<(i32, i32), Option<Arc<crate::prism::GroundChunk>>>>,
    saved_litters:
        RefCell<lru::LruCache<(i32, i32), Option<Arc<crate::surface_litter::LitterChunk>>>>,
    /// Both raw terrain and fuzzy block selection repeatedly revisit the same quart.
    earth_biomes: RefCell<lru::LruCache<(i32, i32), u16>>,
    planetary_cache: RefCell<lru::LruCache<(i32, i32), u16>>,
    beneath_cache: RefCell<lru::LruCache<(i32, i32, i32), u16>>,
    /// Land themed layers reuse the same terrain samples for their water mask.
    height_cache: RefCell<lru::LruCache<(i32, i32), Option<f64>>>,
}

#[cfg(test)]
mod tests {
    fn accurate_display() -> crate::display::DisplayPrefs {
        crate::display::DisplayPrefs {
            terrain: crate::display::TerrainDetailOptions {
                surface_litter: false,
                ..crate::display::TerrainDetailOptions::full()
            },
            ..crate::display::DisplayPrefs::default()
        }
    }

    use super::*;

    #[test]
    fn fast_terrain_sampling_skips_caves_and_volcanic_probes() {
        let world = World::new(0, Settings::default());
        let options = crate::display::TerrainDetailOptions::fast();
        reset_test_query_counts();
        let _ = crate::ground::sample_with_options(&world, 1231, -204, &options);
        let (height_calls, volcanic_calls, cave_calls) = test_query_counts();
        assert_eq!(volcanic_calls, 0);
        assert_eq!(cave_calls, 0);
        assert!(
            height_calls > 0,
            "ground material sampling still needs height"
        );

        reset_test_query_counts();
        let _ = crate::ground_view::relief_height_with_options(
            &world,
            1231,
            -204,
            Some(70.0),
            &options,
        );
        assert_eq!(
            test_query_counts(),
            (0, 0, 0),
            "disabled relief must use supplied terrain only"
        );

        reset_test_query_counts();
        let mut probe = Probe::default();
        crate::ground_view::populate_at_scale_with_options(
            &world, 1231, -204, &mut probe, false, false, &options,
        );
        assert!(
            probe.ground_block.is_some(),
            "distant scale retains cheap base ground"
        );
        assert_eq!(
            test_query_counts().2,
            0,
            "cheap base ground never samples caves"
        );

        reset_test_query_counts();
        crate::caves::populate_with_options(&world, 1231, -204, &mut probe, &options);
        assert_eq!(
            test_query_counts().2,
            0,
            "disabled cave feature does no column work"
        );
    }

    #[test]
    fn saved_reference_spring_water_and_dry_rim_remain_distinct_on_every_layer() {
        use crate::{
            atlas::{self, Layer},
            prism::{GroundChunk, GroundColumn, SurfaceChunk, SurfaceColumn},
        };
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/ground/dike-user-site-seed0.json"
        ))
        .unwrap();
        let world = World::new(0, Settings::default())
            .with_surface_source(Some(PathBuf::from("/virtual/world0-spring-reference")), 1);
        for chunk in reference["chunks"].as_array().unwrap() {
            let cx = chunk["chunk"][0].as_i64().unwrap() as i32;
            let cz = chunk["chunk"][1].as_i64().unwrap() as i32;
            let rows = chunk["columns"].as_array().unwrap();
            let ground = rows
                .iter()
                .map(|r| GroundColumn {
                    y: r["ground_y"].as_i64().unwrap() as i32,
                    block: r["ground_block"].as_str().unwrap().into(),
                })
                .collect();
            let surface = rows
                .iter()
                .map(|r| SurfaceColumn {
                    y: r["top_y"].as_i64().unwrap() as i32,
                    block: r["top_block"].as_str().unwrap().into(),
                    tree: None,
                    crop: None,
                    color: [0; 3],
                })
                .collect();
            world.saved_grounds.borrow_mut().put(
                (cx, cz),
                Some(Arc::new(GroundChunk {
                    chunk_x: cx,
                    chunk_z: cz,
                    columns: ground,
                })),
            );
            world.saved_surfaces.borrow_mut().put(
                (cx, cz),
                Some(Arc::new(SurfaceChunk {
                    chunk_x: cx,
                    chunk_z: cz,
                    columns: surface,
                })),
            );
        }
        let mut water = 0;
        for chunk in reference["chunks"].as_array().unwrap() {
            for row in chunk["columns"].as_array().unwrap() {
                let x = row["world_x"].as_i64().unwrap() as i32;
                let z = row["world_z"].as_i64().unwrap() as i32;
                let wet = row["ground_block"] == "minecraft:water";
                assert_eq!(
                    world.is_surface_water(x, z),
                    wet,
                    "saved spring column {x},{z}"
                );
                if wet {
                    water += 1;
                    assert_eq!(world.surface_water_y(x, z), Some(86));
                    for layer in Layer::ALL {
                        assert_eq!(
                            atlas::raster_color(&world, layer, x, z),
                            [43, 105, 133, 255],
                            "covered spring water {x},{z} on {layer:?}"
                        );
                    }
                }
            }
        }
        assert_eq!(water, 156);
    }

    #[test]
    fn saved_elevated_lake_water_stays_blue_without_flooding_the_whole_biome() {
        use crate::{
            atlas::{self, Layer},
            prism::{GroundChunk, GroundColumn},
        };
        let (x, z) = (10752_i32, -15616_i32);
        let world = World::new(0, Settings::default())
            .with_surface_source(Some(PathBuf::from("/virtual/elevated-lake-surface")), 1);
        assert_eq!(world.biome_id(x, z), biome::ids::id::OLD_MOUNTAIN_LAKE);
        assert!(
            !world.probe(x, z).is_water(),
            "the seed height alone is dry"
        );
        let mut columns = vec![
            GroundColumn {
                y: 86,
                block: "tfc:grass/loam".into()
            };
            256
        ];
        let index = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
        columns[index] = GroundColumn {
            y: 92,
            block: "minecraft:water".into(),
        };
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        world.saved_grounds.borrow_mut().put(
            (cx, cz),
            Some(Arc::new(GroundChunk {
                chunk_x: cx,
                chunk_z: cz,
                columns,
            })),
        );
        for layer in Layer::ALL {
            assert_eq!(
                atlas::raster_color(&world, layer, x, z),
                [43, 105, 133, 255],
                "actual elevated water in {layer:?}"
            );
            assert_ne!(
                atlas::raster_color(&world, layer, x + 1, z),
                [43, 105, 133, 255],
                "adjacent saved dry ground in {layer:?}"
            );
        }
    }

    #[test]
    fn saved_canopies_survive_height_estimates_and_overhang_water() {
        use crate::{
            atlas::{self, Layer},
            display::DisplayPrefs,
            prism::{SurfaceChunk, SurfaceColumn},
        };
        // The dry river bank's estimated ground is Y61. The saved surface may
        // have a different terrain height; its actual leaf must still be shown.
        // The neighboring wet bed also permits a canopy above its water surface.
        for (x, z, y, wet) in [(8833_i32, 11107_i32, 60, false), (8827, 11107, 80, true)] {
            let world = World::new(0, Settings::default()).with_surface_source(
                Some(PathBuf::from(format!("/virtual/canopy-regression/{x}/{z}"))),
                1,
            );
            assert_eq!(world.probe(x, z).is_water(), wet);
            if !wet {
                assert!(world.height(x, z).unwrap() > f64::from(y));
            }
            let key = (x.div_euclid(16), z.div_euclid(16));
            let mut columns = vec![
                SurfaceColumn {
                    y: 55,
                    block: "tfc:grass/loam".into(),
                    tree: None,
                    crop: None,
                    color: [0; 3],
                };
                256
            ];
            let index = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
            columns[index] = SurfaceColumn {
                y,
                block: "tfc:wood/leaves/oak".into(),
                tree: Some("oak".into()),
                crop: None,
                color: [0; 3],
            };
            world.saved_surfaces.borrow_mut().put(
                key,
                Some(Arc::new(SurfaceChunk {
                    chunk_x: key.0,
                    chunk_z: key.1,
                    columns,
                })),
            );
            let plain = atlas::raster_color(&world, Layer::Biomes, x, z);
            let display = DisplayPrefs {
                trees: true,
                ..accurate_display()
            };
            let actual = atlas::raster_color_with_display(&world, Layer::Biomes, x, z, &display);
            let climate = world.climate(x, z);
            let rgb = crate::ground_palette::multiply_tint(
                crate::ground_palette::tree_color("oak", true).unwrap(),
                crate::map_colors::foliage(climate.temperature, climate.rainfall, y),
            );
            assert_eq!(actual, [rgb[0], rgb[1], rgb[2], 255], "leaf at {x},{z}");
            assert_ne!(actual, plain);
            assert_eq!(
                atlas::raster_color_with_display_step(&world, Layer::Biomes, x, z, &display, 4.),
                plain
            );
        }
    }

    #[test]
    fn saved_tree_overlay_is_independent_of_theme_and_full_terrain_detail() {
        use crate::{
            atlas::{self, Layer},
            display::{DisplayPrefs, LayerBlend},
            prism::{SurfaceChunk, SurfaceColumn},
        };
        let (x, z) = (8833_i32, 11107_i32);
        let world = World::new(0, Settings::default())
            .with_surface_source(Some(PathBuf::from("/virtual/independent-tree-layer")), 1);
        let key = (x.div_euclid(16), z.div_euclid(16));
        let mut columns = vec![
            SurfaceColumn {
                y: 64,
                block: "tfc:grass/loam".into(),
                tree: None,
                crop: None,
                color: [0; 3]
            };
            256
        ];
        let index = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
        columns[index].y = 80;
        columns[index].block = "tfc:wood/leaves/oak".into();
        columns[index].tree = Some("oak".into());
        world.saved_surfaces.borrow_mut().put(
            key,
            Some(Arc::new(SurfaceChunk {
                chunk_x: key.0,
                chunk_z: key.1,
                columns,
            })),
        );
        let mut display = DisplayPrefs {
            layer_stack: Some(vec![
                LayerBlend {
                    layer: Layer::Temperature,
                    opacity: 1.,
                },
                LayerBlend {
                    layer: Layer::Rainfall,
                    opacity: 0.4,
                },
            ]),
            ..accurate_display()
        };
        let plain = atlas::raster_color_with_display(&world, Layer::Biomes, x, z, &display);
        display.trees = true;
        let canopy = atlas::raster_color_with_display(&world, Layer::Height, x, z, &display);
        assert_ne!(canopy, plain);
        let climate = world.climate(x, z);
        let rgb = crate::ground_palette::multiply_tint(
            crate::ground_palette::tree_color("oak", true).unwrap(),
            crate::map_colors::foliage(climate.temperature, climate.rainfall, 80),
        );
        assert_eq!(canopy, [rgb[0], rgb[1], rgb[2], 255]);
        display.trees = false;
        let next_plain =
            atlas::raster_color_with_display(&world, Layer::Biomes, x + 1, z, &display);
        display.trees = true;
        assert_eq!(
            atlas::raster_color_with_display(&world, Layer::Height, x + 1, z, &display),
            next_plain,
            "saved non-tree columns suppress seed canopies"
        );
        assert_eq!(
            atlas::raster_color_with_display_step(&world, Layer::Height, x, z, &display, 4.),
            plain,
            "subpixel trees fade without scanning a distant view"
        );
    }

    #[test]
    fn seed_only_dead_calderas_and_exposed_magma_match_saved_positive_controls() {
        use crate::volcanic_lava::Surface;
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/volcanic-terrain/seed0-water-magma.json"
        ))
        .unwrap();
        let world = World::new(0, Settings::default());
        // These independently captured centers check the actual dead branch and its
        // pre-rivulet RNG, without a save path or injected cache in the sampled World.
        for site in fixture["sites"].as_array().unwrap().iter().take(3) {
            let x = site["center"][0].as_i64().unwrap() as i32;
            let z = site["center"][1].as_i64().unwrap() as i32;
            let row = site["columns"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| {
                    row[0].as_i64() == Some(i64::from(x)) && row[1].as_i64() == Some(i64::from(z))
                })
                .unwrap();
            let expected =
                Surface::from_block(row[3].as_str().unwrap(), row[2].as_i64().unwrap() as i32);
            assert!(matches!(expected, Some(Surface::Water(_))));
            assert_eq!(world.volcanic_surface(x, z), expected, "caldera {x},{z}");
            let probe = world.probe(x, z);
            assert!(probe.is_water() && probe.is_fluid());
            assert_eq!(probe.surface_water_y, expected.map(Surface::y));
        }
        let (x, z) = (-14062, 40758);
        let row = fixture["sites"][4]["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0].as_i64() == Some(x) && row[1].as_i64() == Some(z))
            .unwrap();
        let expected =
            Surface::from_block(row[3].as_str().unwrap(), row[2].as_i64().unwrap() as i32);
        assert!(matches!(expected, Some(Surface::Magma(_))));
        assert_eq!(world.volcanic_surface(x as i32, z as i32), expected);
        let probe = world.probe(x as i32, z as i32);
        assert!(!probe.is_water() && !probe.is_fluid());
        assert_eq!(world.tree_estimate(&probe, 10_000.).total, 0.);
    }

    #[test]
    fn saved_water_lava_magma_and_dry_columns_survive_missing_neighbors() {
        use crate::prism::{SurfaceChunk, SurfaceColumn};
        let (x, z) = (13664_i32, 14159_i32);
        let world = World::new(0, Settings::default())
            .with_surface_source(Some(PathBuf::from("/virtual/partial-volcano")), 1);
        let mut columns = vec![
            SurfaceColumn {
                y: 110,
                block: "tfc:rock/raw/basalt".into(),
                tree: None,
                crop: None,
                color: [0; 3]
            };
            256
        ];
        let index = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
        columns[index].y = 104;
        columns[index].block = "minecraft:lava".into();
        columns[index + 2].y = 107;
        columns[index + 2].block = "tfc:fluid/spring_water".into();
        columns[index + 3].y = 103;
        columns[index + 3].block = "tfc:rock/magma/basalt".into();
        let key = (x.div_euclid(16), z.div_euclid(16));
        world.saved_surfaces.borrow_mut().put(
            key,
            Some(Arc::new(SurfaceChunk {
                chunk_x: key.0,
                chunk_z: key.1,
                columns,
            })),
        );
        assert_eq!(world.surface_lava_y(x, z), Some(104));
        assert_eq!(world.surface_water_y(x + 2, z), Some(107));
        assert!(world.is_surface_water(x + 2, z));
        assert_eq!(world.surface_magma_y(x + 3, z), Some(103));
        assert!(!world.is_surface_water(x + 3, z));
        assert!(!world.is_surface_fluid(x + 3, z));
        assert_eq!(
            world.surface_lava_y(x + 1, z),
            None,
            "complete saved dry columns override a seed pool"
        );
        let fallback = World::new(0, Settings::default()).surface_lava_y(x - 1, z);
        assert!(
            fallback.is_some(),
            "the neighboring unknown chunk exercises procedural lava"
        );
        assert_eq!(
            world.surface_lava_y(x - 1, z),
            fallback,
            "unknown columns retain the seed model"
        );
    }
    #[test]
    fn saved_volcanic_surfaces_match_actual_columns_on_all_layers_without_detail() {
        use crate::{
            atlas::{self, Layer},
            display::DisplayPrefs,
            prism::{SurfaceChunk, SurfaceColumn},
        };
        for text in [
            include_str!("../tests/fixtures/volcanic-terrain/seed0-cinder-fluids.json"),
            include_str!("../tests/fixtures/volcanic-terrain/seed0-surface-fluids.json"),
            include_str!("../tests/fixtures/volcanic-terrain/seed0-water-magma.json"),
        ] {
            let data: serde_json::Value = serde_json::from_str(text).unwrap();
            for site in data["sites"].as_array().unwrap() {
                let path = PathBuf::from(format!("/virtual/volcano/{:?}", site["center"]));
                let world = World::new(0, Settings::default()).with_surface_source(Some(path), 1);
                let mut chunks = std::collections::HashMap::new();
                for row in site["columns"].as_array().unwrap() {
                    let (x, z, y) = (
                        row[0].as_i64().unwrap() as i32,
                        row[1].as_i64().unwrap() as i32,
                        row[2].as_i64().unwrap() as i32,
                    );
                    let chunk = chunks
                        .entry((x.div_euclid(16), z.div_euclid(16)))
                        .or_insert_with(|| SurfaceChunk {
                            chunk_x: x.div_euclid(16),
                            chunk_z: z.div_euclid(16),
                            columns: vec![
                                SurfaceColumn {
                                    y: 0,
                                    block: "minecraft:air".into(),
                                    tree: None,
                                    crop: None,
                                    color: [0; 3]
                                };
                                256
                            ],
                        });
                    chunk.columns[(z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize] =
                        SurfaceColumn {
                            y,
                            block: row[3].as_str().unwrap().to_owned(),
                            tree: None,
                            crop: None,
                            color: [0; 3],
                        };
                }
                for (key, chunk) in chunks {
                    world
                        .saved_surfaces
                        .borrow_mut()
                        .put(key, Some(Arc::new(chunk)));
                }
                let mut counts = [0; 3];
                let mut prefs = DisplayPrefs {
                    height_shading: true,
                    palette: crate::display::Palette::Colorblind,
                    temperature_enabled: true,
                    temperature_min: -30.,
                    temperature_max: -29.,
                    ..accurate_display()
                };
                prefs.contours.enabled = true;
                for row in site["columns"].as_array().unwrap() {
                    let (x, z, y) = (
                        row[0].as_i64().unwrap() as i32,
                        row[1].as_i64().unwrap() as i32,
                        row[2].as_i64().unwrap() as i32,
                    );
                    let expected = row[4].as_bool().unwrap().then_some(y);
                    assert_eq!(
                        world.surface_lava_y(x, z),
                        expected,
                        "saved fluid at {x},{z}"
                    );
                    use crate::volcanic_lava::Surface;
                    let expected_surface = Surface::from_block(row[3].as_str().unwrap(), y);
                    assert_eq!(
                        world.volcanic_surface(x, z),
                        expected_surface,
                        "saved block {} at {x},{z}",
                        row[3]
                    );
                    if let Some(expected_surface) = expected_surface {
                        let probe = world.probe(x, z);
                        assert_eq!(probe.volcanic_surface(), Some(expected_surface));
                        assert_eq!(
                            probe.height,
                            world.height(x, z),
                            "fluid/solid overlay must not alter terrain generation height"
                        );
                        assert_eq!(world.tree_estimate(&probe, 10_000.).total, 0.);
                        let color = match expected_surface {
                            Surface::Water(_) => {
                                counts[0] += 1;
                                None
                            }
                            Surface::Lava(_) => {
                                counts[1] += 1;
                                Some(crate::volcanic_lava::COLOR)
                            }
                            Surface::Magma(_) => {
                                counts[2] += 1;
                                assert!(!probe.is_fluid() && !probe.is_water());
                                Some(crate::volcanic_lava::MAGMA_COLOR)
                            }
                        };
                        for layer in Layer::ALL {
                            let actual =
                                atlas::raster_color_with_display(&world, layer, x, z, &prefs);
                            if let Some(color) = color {
                                assert_eq!(actual, [color[0], color[1], color[2], 255]);
                            } else {
                                assert!(
                                    actual[2] > actual[0] && actual[2] > actual[1],
                                    "water stays blue on {layer:?} at {x},{z}: {actual:?}"
                                );
                            }
                        }
                    }
                }
                eprintln!(
                    "saved {:?}: {counts:?} water/lava/magma columns",
                    site["center"]
                );
            }
        }
    }

    #[test]
    fn planetary_probes_do_not_sample_earth_climate_or_geology() {
        for (dimension, range) in [(Dimension::Moon, 2000..2010), (Dimension::Mars, 2100..2114)] {
            let world = World::for_dimension(0, Settings::default(), dimension, 96);
            for (x, z) in [(338, 514), (-622, 82), (842, -270)] {
                let id = world.biome_id(x, z);
                assert!(range.contains(&id));
                let probe = world.sample(x, z, false);
                assert_eq!(probe.biome, world.biome_at(x, 80, z));
                assert!(!probe.ocean);
                assert_eq!(probe.rocks, Probe::default().rocks);
                assert_eq!(probe.climate.rainfall, Climate::default().rainfall);
                assert_eq!(probe.climate.temperature, Climate::default().temperature);
                assert!(world.rock_at(x, 64, z).is_none());
                assert_eq!(world.tree_estimate(&probe, 10_000.).per_hectare, 0.);
            }
        }
    }
}
impl World {
    pub fn new(seed: i64, settings: Settings) -> Self {
        Self::for_dimension(seed, settings, Dimension::Overworld, 64)
    }
    pub fn for_dimension(
        seed: i64,
        settings: Settings,
        dimension: Dimension,
        slice_y: i32,
    ) -> Self {
        Self {
            seed,
            region: Generator::new(seed, settings),
            dimension,
            slice_y: if dimension == Dimension::Beneath {
                dimension.band(slice_y).1
            } else {
                dimension.default_y()
            },
            biome_zoom_seed: biome::block::obfuscate_seed(seed),
            beneath: (dimension == Dimension::Beneath).then(|| crate::beneath::Beneath::new(seed)),
            planetary: dimension
                .is_planetary()
                .then(|| crate::planetary::Planetary::new(seed, dimension)),
            volcanic_lava: (dimension == Dimension::Overworld)
                .then(|| crate::volcanic_lava::Sampler::new(seed)),
            surface_source: None,
            surface_source_id: 0,
            saved_surfaces: RefCell::new(lru::LruCache::new(NonZeroUsize::new(128).unwrap())),
            saved_grounds: RefCell::new(lru::LruCache::new(NonZeroUsize::new(128).unwrap())),
            saved_litters: RefCell::new(lru::LruCache::new(NonZeroUsize::new(128).unwrap())),
            earth_biomes: RefCell::new(lru::LruCache::new(
                NonZeroUsize::new(if dimension == Dimension::Overworld {
                    16_384
                } else {
                    1
                })
                .unwrap(),
            )),
            planetary_cache: RefCell::new(lru::LruCache::new(
                NonZeroUsize::new(if dimension.is_planetary() { 8192 } else { 1 }).unwrap(),
            )),
            beneath_cache: RefCell::new(lru::LruCache::new(
                NonZeroUsize::new(if dimension == Dimension::Beneath {
                    8192
                } else {
                    1
                })
                .unwrap(),
            )),
            height_cache: RefCell::new(lru::LruCache::new(
                NonZeroUsize::new(if dimension != Dimension::Beneath {
                    131_072
                } else {
                    1
                })
                .unwrap(),
            )),
        }
    }
    pub fn biome_id(&self, x: i32, z: i32) -> u16 {
        if let Some(planetary) = &self.planetary {
            let key = (x >> 2, z >> 2);
            if let Some(id) = self.planetary_cache.borrow_mut().get(&key).copied() {
                return id;
            }
            let id = planetary.biome_id(x, z);
            self.planetary_cache.borrow_mut().put(key, id);
            id
        } else if let Some(beneath) = &self.beneath {
            self.cached_beneath_id(beneath, x, self.dimension.band(self.slice_y).0 + 8, z)
        } else {
            self.earth_biome_at_quart(x >> 2, z >> 2)
        }
    }
    pub(crate) fn with_surface_source(mut self, path: Option<PathBuf>, revision: u64) -> Self {
        if self.dimension == Dimension::Overworld && path.is_some() {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            path.hash(&mut hash);
            revision.hash(&mut hash);
            self.surface_source_id = hash.finish().max(1);
            self.surface_source = path;
        }
        self
    }
    pub(crate) fn surface_source_id(&self) -> u64 {
        self.surface_source_id
    }
    pub(crate) fn saved_surface(
        &self,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Option<Arc<crate::prism::SurfaceChunk>> {
        let path = self.surface_source.as_ref()?;
        let key = (chunk_x, chunk_z);
        if let Some(cached) = self.saved_surfaces.borrow_mut().get(&key).cloned() {
            return cached;
        }
        let chunk = crate::prism::read_surface_tile(path, chunk_x, chunk_z)
            .ok()
            .flatten()
            .map(Arc::new);
        self.saved_surfaces.borrow_mut().put(key, chunk.clone());
        chunk
    }
    /// Cached read-only ground substrate extraction, separate from the foliage surface cache.
    pub(crate) fn saved_ground(
        &self,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Option<Arc<crate::prism::GroundChunk>> {
        let path = self.surface_source.as_ref()?;
        let key = (chunk_x, chunk_z);
        if let Some(cached) = self.saved_grounds.borrow_mut().get(&key).cloned() {
            return cached;
        }
        let chunk = crate::prism::read_ground_tile(path, chunk_x, chunk_z)
            .ok()
            .flatten()
            .map(Arc::new);
        self.saved_grounds.borrow_mut().put(key, chunk.clone());
        chunk
    }
    /// Cached Xaero-style surface litter decoded from the saved top block.
    pub(crate) fn saved_litter(
        &self,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Option<Arc<crate::surface_litter::LitterChunk>> {
        let key = (chunk_x, chunk_z);
        if let Some(cached) = self.saved_litters.borrow_mut().get(&key).cloned() {
            return cached;
        }
        let chunk = self
            .saved_surface(chunk_x, chunk_z)
            .map(|surface| Arc::new(crate::surface_litter::LitterChunk::from_surface(&surface)));
        self.saved_litters.borrow_mut().put(key, chunk.clone());
        chunk
    }
    /// Vanilla Minecraft 1.20.1 fuzzy per-block biome lookup. The underlying
    /// generator remains quart sampled; this selects the nearest of its eight
    /// surrounding quart samples exactly as `BiomeManager#getBiome(BlockPos)`.
    pub fn block_biome_id(&self, x: i32, y: i32, z: i32) -> u16 {
        let (qx, qy, qz) = biome::block::nearest_quart(self.biome_zoom_seed, x, y, z);
        let (sx, sy, sz) = (qx.wrapping_mul(4), qy.wrapping_mul(4), qz.wrapping_mul(4));
        if let Some(planetary) = &self.planetary {
            // Planetary climate routing is Y-independent; reuse the existing
            // quart cache shared with the raw surface biome sampler.
            let key = (qx, qz);
            if let Some(id) = self.planetary_cache.borrow_mut().get(&key).copied() {
                return id;
            }
            let id = planetary.biome_id_at(sx, sy, sz);
            self.planetary_cache.borrow_mut().put(key, id);
            id
        } else if let Some(beneath) = &self.beneath {
            self.cached_beneath_id(beneath, sx, sy, sz)
        } else {
            self.earth_biome_at_quart(qx, qz)
        }
    }
    pub(crate) fn earth_biome_at_quart(&self, qx: i32, qz: i32) -> u16 {
        let key = (qx, qz);
        if let Some(id) = self.earth_biomes.borrow_mut().get(&key).copied() {
            return id;
        }
        let id = self.region.biome_at_quart(qx, qz);
        self.earth_biomes.borrow_mut().put(key, id);
        id
    }
    pub fn biome_at(&self, x: i32, y: i32, z: i32) -> String {
        if let Some(planetary) = &self.planetary {
            crate::planetary::biome_name(planetary.biome_id_at(x, y, z)).to_owned()
        } else if let Some(beneath) = &self.beneath {
            crate::beneath::biome_name(self.cached_beneath_id(beneath, x, y, z)).to_owned()
        } else {
            self.biome(x, z)
        }
    }
    fn cached_beneath_id(&self, beneath: &crate::beneath::Beneath, x: i32, y: i32, z: i32) -> u16 {
        let key = (x >> 2, y >> 2, z >> 2);
        let cached = self.beneath_cache.borrow_mut().get(&key).copied();
        if let Some(id) = cached {
            return id;
        }
        let id = beneath.biome_id(x, y, z);
        self.beneath_cache.borrow_mut().put(key, id);
        id
    }
    #[inline]
    pub fn biome(&self, x: i32, z: i32) -> String {
        biome::biome_name(self.biome_id(x, z)).to_owned()
    }
    pub fn climate(&self, x: i32, z: i32) -> Climate {
        if self.dimension != Dimension::Overworld {
            return Climate::default();
        }
        self.region.climate(x, z)
    }
    pub fn sample(&self, x: i32, z: i32, height: bool) -> Probe {
        if self.dimension != Dimension::Overworld {
            return Probe {
                point: Point {
                    x: x as f64,
                    z: z as f64,
                },
                biome: self.biome(x, z),
                height: height.then(|| self.height(x, z)).flatten(),
                ..Probe::default()
            };
        }
        let id = self.region.biome(x, z);
        let climate = self.region.climate(x, z);
        let rocks = geology::rock_stack(self.region.rock_point_at(x, z));
        let hotspot = self.region.hotspot_at(x, z);
        let terrain_height = if height || !biome::is_ocean(id) {
            self.height(x, z)
        } else {
            None
        };
        use crate::volcanic_lava::Surface;
        let volcanic_surface = self.volcanic_surface(x, z);
        let surface_lava = match volcanic_surface {
            Some(Surface::Lava(y)) => Some(y),
            _ => None,
        };
        let surface_water_y = match volcanic_surface {
            Some(Surface::Water(y)) => Some(y),
            _ => None,
        };
        let surface_magma = match volcanic_surface {
            Some(Surface::Magma(y)) => Some(y),
            _ => None,
        };
        Probe {
            point: Point {
                x: x as f64,
                z: z as f64,
            },
            biome: biome::biome_name(id).to_owned(),
            ocean: biome::is_ocean(id),
            surface_water: surface_lava.is_none()
                && surface_magma.is_none()
                && (surface_water_y.is_some()
                    || crate::surface_water::is_surface_water(id, terrain_height)),
            surface_lava,
            surface_water_y,
            surface_magma,
            ground_block: None,
            ground_y: None,
            ground_saved: false,
            climate,
            height: if height { terrain_height } else { None },
            forest: self.region.forest_type(x >> 4, z >> 4),
            rocks,
            hotspot: hotspot.0,
            hotspot_age: hotspot.1 as f64,
        }
    }
    pub fn probe(&self, x: i32, z: i32) -> Probe {
        self.sample(x, z, true)
    }
    pub fn is_surface_water(&self, x: i32, z: i32) -> bool {
        if self.dimension != Dimension::Overworld {
            return false;
        }
        if let Some(surface) = self.volcanic_surface(x, z) {
            return matches!(surface, crate::volcanic_lava::Surface::Water(_));
        }
        let id = self.biome_id(x, z);
        crate::caves::column(self, x, z).map_or_else(
            || biome::is_ocean(id) || crate::surface_water::is_surface_water(id, self.height(x, z)),
            |column| {
                column.fluid.is_some_and(|(cell, y)| {
                    cell == crate::caves::Cell::Water
                        && crate::ground::rock_above_fluid(self, x, z, y).is_none()
                })
            },
        )
    }
    /// Render-only physical water sample. Cheap ordinary biome water is retained
    /// when detailed rivers/lakes or caves are disabled.
    pub(crate) fn is_surface_water_with_options(
        &self,
        x: i32,
        z: i32,
        options: &crate::display::TerrainDetailOptions,
    ) -> bool {
        if self.dimension != Dimension::Overworld {
            return false;
        }
        if options.physical_surface
            && options.volcanoes
            && let Some(surface) = self.volcanic_surface_with_options(x, z, options)
        {
            return matches!(surface, crate::volcanic_lava::Surface::Water(_));
        }
        let id = self.biome_id(x, z);
        if !(options.physical_surface && options.rivers_lakes && options.caves) {
            return biome::is_ocean(id)
                || crate::surface_water::is_surface_water(id, self.height(x, z));
        }
        crate::caves::column(self, x, z).map_or_else(
            || biome::is_ocean(id) || crate::surface_water::is_surface_water(id, self.height(x, z)),
            |column| {
                column.fluid.is_some_and(|(cell, y)| {
                    cell == crate::caves::Cell::Water
                        && (!options.large_rocks_special_soils
                            || crate::ground::rock_above_fluid(self, x, z, y).is_none())
                })
            },
        )
    }
    pub fn surface_lava_y(&self, x: i32, z: i32) -> Option<i32> {
        match self.volcanic_surface(x, z)? {
            crate::volcanic_lava::Surface::Lava(y) => Some(y),
            _ => None,
        }
    }
    pub fn volcanic_surface(&self, x: i32, z: i32) -> Option<crate::volcanic_lava::Surface> {
        self.volcanic_lava
            .as_ref()
            .and_then(|sampler| sampler.surface(self, x, z))
            .or_else(|| {
                let spring = crate::hot_springs::sample(self, x, z)?;
                // A known dry column must not resurrect a seed-only pool when
                // the volcanic sampler returns None. Check saves only at rare
                // spring candidates, never across an entire distant viewport.
                if self.surface_source_id != 0 {
                    // The highest saved block can be a lily or other cover.
                    // Use the independently decoded substrate/fluid plane so
                    // plant-covered water is retained and known dry rims still
                    // suppress the procedural pool.
                    if let Some(chunk) = self.saved_ground(x.div_euclid(16), z.div_euclid(16))
                        && let Some(column) = chunk
                            .columns
                            .get((z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize)
                            .filter(|column| column.y != i32::MIN && !column.block.is_empty())
                    {
                        return crate::volcanic_lava::Surface::from_block(&column.block, column.y);
                    }
                    if let Some(chunk) = self.saved_surface(x.div_euclid(16), z.div_euclid(16))
                        && let Some(column) = chunk
                            .columns
                            .get((z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize)
                    {
                        return crate::volcanic_lava::Surface::from_block(&column.block, column.y);
                    }
                }
                crate::volcanic_lava::Surface::from_block(&spring.block, spring.y)
            })
    }
    /// Render-only volcanic/spring overlay respecting this request's detail flags.
    pub(crate) fn volcanic_surface_with_options(
        &self,
        x: i32,
        z: i32,
        options: &crate::display::TerrainDetailOptions,
    ) -> Option<crate::volcanic_lava::Surface> {
        #[cfg(test)]
        TEST_QUERY_COUNTS.with(|c| {
            let (h, v, k) = c.get();
            c.set((h, v + 1, k));
        });
        if !options.physical_surface {
            return None;
        }
        if options.volcanoes
            && let Some(surface) = self
                .volcanic_lava
                .as_ref()
                .and_then(|sampler| sampler.surface(self, x, z))
        {
            return Some(surface);
        }
        if !options.springs {
            return None;
        }
        let spring = crate::hot_springs::sample(self, x, z)?;
        if self.surface_source_id != 0
            && let Some(chunk) = self.saved_ground(x.div_euclid(16), z.div_euclid(16))
            && let Some(column) = chunk
                .columns
                .get((z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize)
                .filter(|column| column.y != i32::MIN && !column.block.is_empty())
        {
            return crate::volcanic_lava::Surface::from_block(&column.block, column.y);
        }
        crate::volcanic_lava::Surface::from_block(&spring.block, spring.y)
    }
    pub fn surface_water_y(&self, x: i32, z: i32) -> Option<i32> {
        if let Some(surface) = self.volcanic_surface(x, z) {
            return match surface {
                crate::volcanic_lava::Surface::Water(y) => Some(y),
                _ => None,
            };
        }
        crate::caves::column(self, x, z)?
            .fluid
            .and_then(|(cell, y)| {
                (cell == crate::caves::Cell::Water
                    && crate::ground::rock_above_fluid(self, x, z, y).is_none())
                .then_some(y)
            })
    }
    pub fn surface_magma_y(&self, x: i32, z: i32) -> Option<i32> {
        match self.volcanic_surface(x, z)? {
            crate::volcanic_lava::Surface::Magma(y) => Some(y),
            _ => None,
        }
    }
    pub fn is_surface_fluid(&self, x: i32, z: i32) -> bool {
        if self.dimension != Dimension::Overworld {
            return false;
        }
        if let Some(surface) = self.volcanic_surface(x, z) {
            return !matches!(surface, crate::volcanic_lava::Surface::Magma(_));
        }
        let id = self.biome_id(x, z);
        crate::caves::column(self, x, z).map_or_else(
            || biome::is_ocean(id) || crate::surface_water::is_surface_water(id, self.height(x, z)),
            |column| {
                column
                    .fluid
                    .is_some_and(|(_, y)| crate::ground::rock_above_fluid(self, x, z, y).is_none())
            },
        )
    }
    pub fn height(&self, x: i32, z: i32) -> Option<f64> {
        #[cfg(test)]
        TEST_QUERY_COUNTS.with(|c| {
            let (h, v, k) = c.get();
            c.set((h + 1, v, k));
        });
        if self.dimension == Dimension::Beneath {
            return None;
        }
        if let Some(height) = self.height_cache.borrow_mut().get(&(x, z)).copied() {
            return height;
        }
        let height = if let Some(planetary) = &self.planetary {
            planetary.height(x, z)
        } else {
            geology::surface_height(&self.region, self.seed, x, z)
        };
        self.height_cache.borrow_mut().put((x, z), height);
        height
    }
    pub fn rock_at(&self, x: i32, y: i32, z: i32) -> Option<String> {
        if self.dimension != Dimension::Overworld {
            return None;
        }
        let h = self.height(x, z)?;
        let (point, layer) = self.region.rock_layer_at(x, y, z, h as i32);
        Some(geology::sample_rock_layer(point, layer))
    }
    pub fn tree_estimate(&self, p: &Probe, area: f64) -> TreeEstimate {
        if self.dimension != Dimension::Overworld || p.is_bare_surface() {
            return TreeEstimate::default();
        }
        geology::estimate_trees(p, area)
    }
}
