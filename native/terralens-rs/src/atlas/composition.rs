//! One sampling plan for a complete tile/export, independent of the focused legend.
use super::*;
use crate::display::LayerBlend;

pub(super) struct Composition {
    layers: Vec<LayerBlend>,
    plain: DisplayPrefs,
    climate: bool,
    rocks: bool,
    forest: bool,
    hotspots: bool,
    height: bool,
    ground: bool,
    saved_ground: bool,
    grass: f32,
    trees: f32,
}

impl Composition {
    pub(super) fn needs_height(&self) -> bool {
        self.height
    }
    pub(super) fn uses_saved_ground(&self) -> bool {
        self.ground && self.saved_ground
    }
    pub(super) fn uses_ground_material(&self) -> bool {
        self.ground
    }
    pub(super) fn uses_trees(&self) -> bool {
        self.trees > 0.
    }
    pub(super) fn new(
        display: &DisplayPrefs,
        fallback: Layer,
        dimension: Dimension,
        step: f64,
    ) -> Option<Self> {
        if display.layer_stack.is_none()
            && !display.grass_colors
            && !display.trees
            && fallback != Layer::Ground
        {
            return None;
        }
        let layers: Vec<_> = display
            .active_layers(fallback, dimension)
            .into_iter()
            .filter(|blend| blend.opacity.is_finite() && blend.opacity > 0.)
            .collect();
        let has = |layer| layers.iter().any(|blend| blend.layer == layer);
        let earth = dimension == Dimension::Overworld;
        let grass = if earth && display.grass_colors {
            opacity(display.grass_opacity)
        } else {
            0.
        };
        // Keep the user's opacity at local scales. Distant tiles skip tree
        // enumeration rather than showing ground through opaque canopies.
        let trees = if earth && display.trees && step < 4. {
            opacity(display.tree_opacity)
        } else {
            0.
        };
        let ground = earth && (grass > 0. || has(Layer::Ground));
        let climate = earth
            && (grass > 0.
                || ground
                || has(Layer::Temperature)
                || has(Layer::Rainfall)
                || display.temperature_enabled
                || display.rainfall_enabled);
        let rocks = earth && (has(Layer::Geology) || !display.highlighted_rocks.is_empty());
        let forest = earth && has(Layer::Forest);
        let hotspots = earth && has(Layer::Hotspots);
        let height = has(Layer::Height);
        // Reuse every existing layer palette, but apply land highlighting once
        // after composition instead of tinting each input repeatedly.
        let mut plain = display.clone();
        plain.highlighted_biomes.clear();
        plain.highlighted_rocks.clear();
        plain.temperature_enabled = false;
        plain.rainfall_enabled = false;
        Some(Self {
            layers,
            plain,
            climate,
            rocks,
            forest,
            hotspots,
            height,
            ground,
            // Read generated chunks only at local exploration scales. A distant
            // material map samples the seed without opening thousands of files.
            saved_ground: step <= 4. && display.terrain.physical_surface,
            grass,
            trees,
        })
    }

    pub(super) fn sample(
        &self,
        world: &World,
        x: i32,
        z: i32,
        id: u16,
        water: Option<bool>,
        surface: Option<Option<crate::volcanic_lava::Surface>>,
        terrain: Option<Option<f64>>,
    ) -> Probe {
        let mut sample = sample_layer_with_water(
            world,
            x,
            z,
            id,
            Layer::Biomes,
            water,
            surface,
            terrain,
            self.saved_ground,
            &self.plain.terrain,
        );
        if world.dimension != Dimension::Overworld {
            if self.height {
                sample.height = terrain.unwrap_or_else(|| world.height(x, z));
            }
            return sample;
        }
        if self.ground {
            crate::ground_view::populate_at_scale_with_options(
                world,
                x,
                z,
                &mut sample,
                self.saved_ground,
                self.saved_ground,
                &self.plain.terrain,
            );
        }
        if sample.is_bare_surface() {
            return sample;
        }
        if self.climate {
            sample.climate = world.region.climate(x, z);
        }
        if self.rocks {
            sample.rocks = crate::geology::rock_stack(world.region.rock_point_at(x, z));
        }
        if self.forest {
            sample.forest = world.region.forest_type(x >> 4, z >> 4);
        }
        if self.hotspots {
            let (value, age) = world.region.hotspot_at(x, z);
            sample.hotspot = value;
            sample.hotspot_age = f64::from(age);
        }
        if self.height {
            sample.height = if self.saved_ground
                && self.plain.terrain.physical_surface
                && self.plain.terrain.caves
            {
                crate::caves::column(world, x, z).map(|c| f64::from(c.solid_y))
            } else {
                None
            }
            .or_else(|| terrain.unwrap_or_else(|| world.height(x, z)));
        }
        sample
    }

    pub(super) fn color(
        &self,
        world: &World,
        id: u16,
        sample: &Probe,
        display: &DisplayPrefs,
        x: i32,
        z: i32,
    ) -> Color32 {
        let bare = sample.is_bare_surface();
        let base = if bare {
            // Open fluids and magma retain their physical colors. Real canopy
            // columns may overhang them and are composited above this surface.
            display_color(world, Layer::Biomes, id, sample, &self.plain, x, z)
        } else {
            self.land_color(world, id, sample, x, z)
        };
        if bare {
            return base;
        }
        if world.dimension != Dimension::Overworld {
            return highlighted_biome_color(base, id, display, false);
        }
        if !display.has_highlight_filter() {
            return base;
        }
        if display.matches_land_sample(
            id,
            sample.climate.temperature as f32,
            sample.climate.rainfall as f32,
            &sample.rocks,
        ) {
            mix_color(base, Color32::from_rgb(255, 222, 108), 0.58)
        } else {
            mix_color(base, Color32::from_rgb(23, 31, 35), 0.42)
        }
    }

    /// Paint the visible tree surface last, after thematic layers, ground relief
    /// and contours. Crown slopes come from the tree blocks, never buried rock.
    pub(super) fn overlay_trees(
        &self,
        world: &World,
        base: Color32,
        x: i32,
        z: i32,
        surface: Option<crate::volcanic_lava::Surface>,
        step: f64,
        cancelled: &impl Fn() -> bool,
    ) -> Color32 {
        if self.trees <= 0. {
            return base;
        }
        let Some(canopy) = crate::vegetation::canopy_at(world, x, z, cancelled) else {
            return base;
        };
        if !canopy.saved && surface.is_some_and(|surface| canopy.y <= surface.y()) {
            return base;
        }
        let mut color = match canopy.kind {
            crate::vegetation::CanopyKind::Wood => color_from_rgb(
                crate::ground_palette::tree_color(&canopy.species, false).unwrap_or([103, 87, 64]),
            ),
            crate::vegetation::CanopyKind::Leaves => {
                let climate = world.region.climate(x, z);
                let rgb = crate::map_colors::foliage_at(
                    climate.temperature,
                    climate.rainfall,
                    x,
                    canopy.y,
                    z,
                    &canopy.species,
                    world.region.settings.temperature_scale,
                    self.plain.simulated_season(),
                );
                color_from_rgb(crate::ground_palette::multiply_tint(
                    crate::ground_palette::tree_color(&canopy.species, true).unwrap_or([148; 3]),
                    rgb,
                ))
            }
        };
        if self.plain.height_shading && self.plain.hillshade_strength > 0. {
            let offset = step.round().max(1.) as i32;
            // Missing neighboring crowns do not turn into terrain-height cliffs.
            // A shared block grid keeps the silhouette identical on tile edges.
            let heights = [(0, 0), (0, -offset), (-offset, -offset)].map(|(dx, dz)| {
                Some(f64::from(
                    crate::vegetation::canopy_at(
                        world,
                        x.saturating_add(dx),
                        z.saturating_add(dz),
                        cancelled,
                    )
                    .map_or(canopy.y, |neighbor| neighbor.y),
                ))
            });
            color = apply_hillshade(
                color,
                heights,
                step.round().max(1.),
                self.plain.hillshade_strength,
            );
        }
        mix_color(base, color, self.trees)
    }

    fn land_color(&self, world: &World, id: u16, sample: &Probe, x: i32, z: i32) -> Color32 {
        let mut base = match world.dimension {
            Dimension::Overworld => Color32::from_rgb(190, 190, 167),
            Dimension::Beneath => beneath_biome_color(id, self.plain.palette),
            _ => planetary_biome_color(id, self.plain.palette),
        };
        // Grass tint belongs to grass blocks. Sand, gravel, exposed rock, duff
        // and snow keep the colors of their actual surface material.
        let grass = (self.grass > 0.)
            .then(|| {
                crate::ground_view::probe_color_at(
                    sample,
                    world.region.settings.temperature_scale,
                    self.plain.simulated_season(),
                )
                .map(color_from_rgb)
            })
            .flatten();
        if let Some(grass) = grass {
            base = mix_color(base, grass, self.grass);
        }
        let water_only =
            self.grass == 0. && self.layers.len() == 1 && self.layers[0].layer == Layer::Water;
        for blend in &self.layers {
            if blend.layer == Layer::Water
                && !water_only
                && !sample.biome.ends_with("martian_river")
            {
                continue;
            }
            let mut color = display_color(world, blend.layer, id, sample, &self.plain, x, z);
            if blend.layer == Layer::Biomes
                && let Some(grass) = grass
            {
                color = mix_color(color, grass, self.grass);
            }
            base = mix_color(base, color, blend.opacity);
        }
        base
    }
}

fn opacity(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0., 1.)
    } else {
        0.
    }
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

    struct SavedFixture(std::path::PathBuf);
    impl Drop for SavedFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    impl SavedFixture {
        fn new(gzip: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "terralens-ground-map-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(path.join("region")).unwrap();
            let sectors = (gzip.len() + 5).div_ceil(4096);
            let mut region = vec![0u8; (2 + sectors) * 4096];
            region[..4].copy_from_slice(&[0, 0, 2, sectors as u8]);
            region[8192..8196].copy_from_slice(&((gzip.len() + 1) as u32).to_be_bytes());
            region[8196] = 1;
            region[8197..8197 + gzip.len()].copy_from_slice(gzip);
            std::fs::write(path.join("region/r.0.0.mca"), region).unwrap();
            Self(path)
        }
        fn world(&self, revision: u64) -> World {
            World::new(0, Settings::default()).with_surface_source(Some(self.0.clone()), revision)
        }
    }

    #[test]
    fn actual_saved_litter_is_optional_and_agrees_with_independent_anvil_columns() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ground/seed0-ground-samples.json"
        ))
        .unwrap();
        let mut observed = 0;
        for (name, nbt) in [
            (
                "floodplain_nearshore",
                include_bytes!("../../tests/fixtures/ground/floodplain_nearshore.nbt.gz")
                    .as_slice(),
            ),
            (
                "active_shield_canopy",
                include_bytes!("../../tests/fixtures/ground/active_shield_canopy.nbt.gz")
                    .as_slice(),
            ),
            (
                "saved_plain",
                include_bytes!("../../tests/fixtures/ground/saved_plain.nbt.gz").as_slice(),
            ),
            (
                "cinder_cone",
                include_bytes!("../../tests/fixtures/ground/cinder_cone.nbt.gz").as_slice(),
            ),
        ] {
            let saved = SavedFixture::new(nbt);
            let world = saved.world(12);
            let chunk = world
                .saved_litter(0, 0)
                .expect("actual saved chunk decoded through production reader");
            let rows = fixture["sites"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["name"] == name)
                .unwrap()["columns"]
                .as_array()
                .unwrap();
            for row in rows {
                let x = row["x"].as_i64().unwrap() as i32;
                let z = row["z"].as_i64().unwrap() as i32;
                let actual = row["top_block"].as_str().unwrap();
                let found = chunk.get(x as usize, z as usize);
                assert_eq!(
                    found.is_some(),
                    crate::surface_litter::classify(actual).is_some(),
                    "{name} {x},{z}: {actual}"
                );
                let Some(block) = found else {
                    continue;
                };
                observed += 1;
                assert_eq!(block.block, actual);
                assert_eq!(Some(i64::from(block.y)), row["top_y"].as_i64());
                let base = Color32::from_rgb(80, 120, 70);
                let mut display = DisplayPrefs::default();
                assert_eq!(
                    sampling::overlay_litter(&world, base, x, z, 1., &display, false, &|| false),
                    base
                );
                display.terrain.surface_litter = true;
                let expected =
                    crate::surface_litter::composite_at([80, 120, 70], &world, block, x, z, None);
                assert_eq!(
                    sampling::overlay_litter(&world, base, x, z, 1., &display, false, &|| false)
                        .to_array(),
                    [expected[0], expected[1], expected[2], 255]
                );
                assert_eq!(
                    sampling::overlay_litter(&world, base, x, z, 1., &display, true, &|| false),
                    base,
                    "never paint debris over open water"
                );
                assert_eq!(
                    sampling::overlay_litter(&world, base, x, z, 8., &display, false, &|| false),
                    base,
                    "regional view skips fine clutter"
                );
            }
        }
        assert!(
            observed >= 20,
            "independent saved controls need a substantial positive litter sample"
        );
    }

    #[test]
    fn ground_and_grass_maps_use_saved_materials_beneath_cover() {
        let saved = SavedFixture::new(include_bytes!(
            "../../tests/fixtures/ground/saved_plain.nbt.gz"
        ));
        let world = saved.world(1);
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ground/seed0-ground-samples.json"
        ))
        .unwrap();
        let rows = fixture["sites"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "saved_plain")
            .unwrap()["columns"]
            .as_array()
            .unwrap();
        let ground_display = accurate_display();
        let grass_display = DisplayPrefs {
            grass_colors: true,
            ..accurate_display()
        };
        let mut covered = 0;
        for row in rows {
            let x = row["x"].as_i64().unwrap() as i32;
            let z = row["z"].as_i64().unwrap() as i32;
            let mut probe = world.probe(x, z);
            crate::ground_view::populate(&world, x, z, &mut probe, true);
            assert!(probe.ground_saved);
            assert_eq!(probe.ground_block.as_deref(), row["ground_block"].as_str());
            let rgb = crate::ground_view::probe_color(&probe).unwrap();
            let expected = [rgb[0], rgb[1], rgb[2], 255];
            assert_eq!(
                raster_color_with_display(&world, Layer::Ground, x, z, &ground_display),
                expected,
                "ground at {x},{z}"
            );
            assert_eq!(
                raster_color_with_display(&world, Layer::Biomes, x, z, &grass_display),
                expected,
                "grass/material at {x},{z}"
            );
            covered += usize::from(row["top_block"] != row["ground_block"]);
        }
        assert_eq!(covered, 104);
    }

    #[test]
    fn opaque_trees_cover_material_layers_and_terrain_contours() {
        let saved = SavedFixture::new(include_bytes!(
            "../../tests/fixtures/ground/active_shield_canopy.nbt.gz"
        ));
        let world = saved.world(1);
        let surface = world.saved_surface(0, 0).unwrap();
        let mut covered = 0;
        for (index, column) in surface.columns.iter().enumerate() {
            if column.tree.is_none() {
                continue;
            }
            let (x, z) = ((index % 16) as i32, (index / 16) as i32);
            let prefs = DisplayPrefs {
                trees: true,
                height_shading: true,
                ..accurate_display()
            };
            let plain = raster_color_with_display(&world, Layer::Ground, x, z, &prefs);
            let composition =
                Composition::new(&prefs, Layer::Ground, Dimension::Overworld, 1.).unwrap();
            assert_eq!(
                composition
                    .overlay_trees(
                        &world,
                        Color32::BLACK,
                        x,
                        z,
                        Some(crate::volcanic_lava::Surface::Water(column.y)),
                        1.,
                        &|| false,
                    )
                    .to_array(),
                plain,
                "a saved waterlogged leaf is still the visible tree block at {x},{z}"
            );
            for layers in [
                vec![Layer::Geology, Layer::Ground],
                vec![Layer::Ground, Layer::Geology],
                vec![Layer::Rainfall, Layer::Height, Layer::Ground],
            ] {
                let mut display = prefs.clone();
                display.layer_stack = Some(
                    layers
                        .into_iter()
                        .map(|layer| LayerBlend { layer, opacity: 1. })
                        .collect(),
                );
                display.contours.enabled = true;
                display.contours.interval = 1;
                assert_eq!(
                    raster_color_with_display(&world, Layer::Ground, x, z, &display),
                    plain,
                    "buried material relief and contours must not alter tree blocks at {x},{z}"
                );
            }
            covered += 1;
        }
        assert!(covered > 100, "reference must exercise actual saved canopy");
    }

    #[test]
    fn saved_ground_overrides_cannot_change_another_layers_cached_water_mask() {
        // Relocate an actual saved wet chunk over otherwise dry seed terrain.
        // Separate source revisions give the control an independent raster cache.
        let saved = SavedFixture::new(include_bytes!(
            "../../tests/fixtures/ground/river_waterlogged_bank.nbt.gz"
        ));
        let world = saved.world(1);
        let control = saved.world(2);
        let display = accurate_display();
        let key = TileKey {
            zoom: 0,
            x: 0,
            z: 0,
            layer: Layer::Biomes,
            detail: false,
        };
        let expected = render_tile(&control, key, &display, || false).unwrap();
        let material = render_tile(
            &world,
            TileKey {
                layer: Layer::Ground,
                ..key
            },
            &display,
            || false,
        )
        .unwrap();
        assert_ne!(
            material.surface_water, expected.surface_water,
            "fixture must override the seed water mask"
        );
        let actual = render_tile(&world, key, &display, || false).unwrap();
        assert_eq!(actual.surface_water, expected.surface_water);
        assert_eq!(actual.pixels, expected.pixels);
    }

    #[test]
    fn layer_opacity_combines_both_maps_and_preserves_endpoints() {
        let world = World::new(0, Settings::default());
        let (x, z) = (8833, 11107);
        assert!(!world.probe(x, z).is_bare_surface());
        let first = raster_color(&world, Layer::Temperature, x, z);
        let second = raster_color(&world, Layer::Rainfall, x, z);
        assert_ne!(first, second);
        let mut display = DisplayPrefs {
            layer_stack: Some(vec![
                LayerBlend {
                    layer: Layer::Temperature,
                    opacity: 1.,
                },
                LayerBlend {
                    layer: Layer::Rainfall,
                    opacity: 0.,
                },
            ]),
            ..accurate_display()
        };
        assert_eq!(
            raster_color_with_display(&world, Layer::Biomes, x, z, &display),
            first
        );
        display.layer_stack.as_mut().unwrap()[1].opacity = 1.;
        assert_eq!(
            raster_color_with_display(&world, Layer::Biomes, x, z, &display),
            second
        );
        display.layer_stack.as_mut().unwrap()[1].opacity = 0.25;
        let actual = raster_color_with_display(&world, Layer::Biomes, x, z, &display);
        for i in 0..3 {
            assert_eq!(
                actual[i],
                (f32::from(first[i]) * 0.75 + f32::from(second[i]) * 0.25) as u8
            );
        }
        // The legacy selected layer is only a focus/legend when a stack exists.
        assert_eq!(
            actual,
            raster_color_with_display(&world, Layer::Height, x, z, &display)
        );

        display.move_layer(Layer::Temperature, 1, Layer::Biomes, Dimension::Overworld);
        assert_eq!(
            raster_color_with_display(&world, Layer::Height, x, z, &display),
            first,
            "an opaque layer brought to the foreground must cover lower layers"
        );
    }

    #[test]
    fn grass_is_independent_of_categorical_biomes_and_fluid_colors_survive_every_option() {
        let world = World::new(0, Settings::default());
        let (x, z) = (8833, 11107);
        let probe = world.probe(x, z);
        let mut display = DisplayPrefs {
            layer_stack: Some(vec![LayerBlend {
                layer: Layer::Biomes,
                opacity: 1.,
            }]),
            grass_colors: true,
            ..accurate_display()
        };
        let rgb = crate::ground_palette::multiply_tint(
            crate::ground_palette::grass_tint_overlay().unwrap(),
            crate::map_colors::grass(
                probe.climate.temperature,
                probe.climate.rainfall,
                probe.height.unwrap() as i32,
            ),
        );
        assert_eq!(
            raster_color_with_display(&world, Layer::Height, x, z, &display),
            [rgb[0], rgb[1], rgb[2], 255]
        );
        display.layer_stack = Some(vec![]);
        assert_eq!(
            raster_color_with_display(&world, Layer::Height, x, z, &display),
            [rgb[0], rgb[1], rgb[2], 255]
        );
        display.layer_stack = Some(vec![LayerBlend {
            layer: Layer::Water,
            opacity: 1.,
        }]);
        assert_eq!(
            raster_color_with_display(&world, Layer::Water, x, z, &display),
            [rgb[0], rgb[1], rgb[2], 255],
            "water overlay leaves grass visible on land"
        );
        display.layer_stack = Some(
            Layer::ALL
                .into_iter()
                .map(|layer| LayerBlend {
                    layer,
                    opacity: 0.55,
                })
                .collect(),
        );
        display.trees = true;
        display.height_shading = true;
        display.contours.enabled = true;
        for (x, z) in [(1856, 1147), (5511, -6368), (-14062, 40758)] {
            let mut plain_display = DisplayPrefs {
                height_shading: true,
                ..accurate_display()
            };
            plain_display.contours.enabled = true;
            let plain = raster_color_with_display(&world, Layer::Biomes, x, z, &plain_display);
            let mixed = raster_color_with_display(&world, Layer::Rainfall, x, z, &display);
            assert_eq!(mixed, plain, "physical volcanic surfaces at {x},{z}");
        }
    }

    #[test]
    fn opaque_full_detail_cannot_cover_a_composed_or_translucent_map() {
        let ctx = egui::Context::default();
        let mut atlas = Atlas::new(&ctx, 0, Settings::default());
        atlas.bpp = 0.25;
        atlas.surface.set_options(true, None);
        atlas.spec.display = accurate_display();
        assert!(atlas.surface_visible(true));
        for layers in [
            vec![],
            vec![LayerBlend {
                layer: Layer::Biomes,
                opacity: 0.5,
            }],
            vec![
                LayerBlend {
                    layer: Layer::Biomes,
                    opacity: 1.,
                },
                LayerBlend {
                    layer: Layer::Temperature,
                    opacity: 0.5,
                },
            ],
        ] {
            atlas.spec.display.layer_stack = Some(layers);
            assert!(!atlas.surface_visible(true));
        }
        atlas.spec.display.layer_stack = Some(vec![LayerBlend {
            layer: Layer::Biomes,
            opacity: 1.,
        }]);
        assert!(atlas.surface_visible(true));
        atlas.spec.display.grass_colors = true;
        assert!(!atlas.surface_visible(true));
        atlas.spec.display.grass_colors = false;
        atlas.spec.display.trees = true;
        assert!(!atlas.surface_visible(true));
        atlas.spec.display.tree_opacity = 0.;
        assert!(
            atlas.surface_visible(true),
            "invisible overlays do not mask terrain detail"
        );
        atlas.spec.display.trees = false;
        atlas.spec.display.height_shading = true;
        assert!(
            !atlas.surface_visible(true),
            "opaque detail must not cover relief"
        );
        atlas.spec.display.height_shading = false;
        atlas.spec.display.contours.enabled = true;
        assert!(
            !atlas.surface_visible(true),
            "opaque detail must not cover height lines"
        );
        atlas.spec.display.contours.enabled = false;
        atlas.spec.display.highlighted_biomes.push(1);
        assert!(
            !atlas.surface_visible(true),
            "opaque detail must not cover highlighting"
        );
        atlas.spec.display.highlighted_biomes.clear();
        atlas.spec.display.palette = crate::display::Palette::Colorblind;
        assert!(
            !atlas.surface_visible(true),
            "opaque detail must not cover palette choice"
        );
        atlas.spec.display.palette = crate::display::Palette::Natural;
        atlas.spec.dimension = Dimension::Moon;
        atlas.spec.display.grass_colors = true;
        atlas.spec.display.trees = true;
        assert!(
            atlas.surface_visible(true),
            "Overworld-only options stay inactive on the Moon"
        );
        atlas.spec.display.temperature_enabled = true;
        atlas.spec.display.rainfall_enabled = true;
        atlas.spec.display.highlighted_rocks.push("granite".into());
        assert!(
            atlas.surface_visible(true),
            "Overworld-only filters stay inactive on the Moon"
        );
        atlas.spec.display.highlighted_biomes.push(1);
        assert!(
            !atlas.surface_visible(true),
            "biome highlights still affect planetary maps"
        );
    }

    #[test]
    fn mixed_tree_tiles_and_export_samples_agree_at_block_precision() {
        let world = World::new(0, Settings::default());
        let key = TileKey {
            zoom: 0,
            x: 57,
            z: 55,
            layer: Layer::Biomes,
            detail: true,
        };
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
                LayerBlend {
                    layer: Layer::Height,
                    opacity: 0.25,
                },
                LayerBlend {
                    layer: Layer::Ground,
                    opacity: 0.25,
                },
            ]),
            grass_colors: true,
            trees: true,
            height_shading: true,
            ..accurate_display()
        };
        display.contours.enabled = true;
        let tile = render_tile(&world, key, &display, || false).unwrap();
        let raster = RasterSampler::new(&world, Layer::Geology, &display, [1., 1.]);
        let mut covered = 0;
        for z in (0..DETAIL).step_by(17) {
            for x in (0..DETAIL).step_by(19) {
                let (wx, wz) = tile_sample(key, DETAIL, x as i32, z as i32);
                let actual = tile.pixels[(z + 1) * (DETAIL + 2) + x + 1].to_array();
                assert_eq!(
                    actual,
                    raster.color(wx, wz, &|| false),
                    "mixed tree pixel {wx},{wz}"
                );
                if !world.is_surface_fluid(wx, wz)
                    && crate::vegetation::canopy_at(&world, wx, wz, &|| false).is_some()
                {
                    covered += 1;
                }
            }
        }
        assert!(
            covered > 0,
            "the test must include actual seed canopy cells"
        );
    }
}
