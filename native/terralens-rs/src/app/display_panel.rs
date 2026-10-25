//! Compact map controls. Labels remain direct controls; this panel adds no headings.
use crate::{
    atlas::Layer,
    contours::ContourPrefs,
    dimension::Dimension,
    display::{DisplayPrefs, LoadingFilter, MapDetail, Palette, Preset, TerrainPreset},
};
use eframe::egui::{self, Ui};

#[derive(Clone, Copy)]
pub struct FeatureOption<'a> {
    pub id: &'a str,
    pub label: &'a str,
}

#[derive(Default)]
pub struct DisplayChanges {
    pub layer: Option<Layer>,
    pub preset: bool,
}

/// Compact terrain sampling presets and independent detail gates for the Map menu.
pub(super) fn terrain_detail_controls(ui: &mut Ui, prefs: &mut DisplayPrefs) {
    ui.horizontal_wrapped(|ui| {
        for (preset, label, help) in [
            (TerrainPreset::Fast, "Fast", "Biome colors and structures; skips optional high-cost terrain sampling."),
            (TerrainPreset::Balanced, "Balanced", "Adds physical surface materials, detailed river and lake surfaces, volcanic surfaces, and large surface features."),
            (TerrainPreset::Full, "Full", "Samples all terrain detail, including block biome edges, caves, springs, dikes, and litter."),
        ] {
            if ui.selectable_label(prefs.terrain_preset() == preset, label).on_hover_text(help).clicked() {
                prefs.apply_terrain_preset(preset);
            }
        }
        if prefs.terrain_preset() == TerrainPreset::Custom {
            ui.weak("Custom");
        }
    });
    ui.separator();
    let options = &mut prefs.terrain;
    ui.checkbox(
        &mut options.precise_biome_boundaries,
        "Block-precise biome edges",
    )
    .on_hover_text(
        "Samples biome boundaries at block resolution instead of the faster quart grid.",
    );
    ui.checkbox(&mut options.physical_surface, "Physical terrain and ground")
        .on_hover_text("Finds actual surface height and ground materials. Enables the terrain feature controls below. Three-dimensional cave and carver columns are sampled only when Surface caves and ravines is enabled.");
    ui.add_enabled_ui(options.physical_surface, |ui| {
    ui.checkbox(&mut options.caves, "Surface caves and ravines")
        .on_hover_text("Checks surface openings and flooded cave or ravine floors. This is one of the more expensive terrain checks.");
    ui.checkbox(&mut options.rivers_lakes, "Detailed river/lake surfaces")
        .on_hover_text("Samples detailed river and lake surfaces and water coverage rather than using the fast biome-based estimate.");
    ui.checkbox(&mut options.volcanoes, "Volcanic surfaces")
        .on_hover_text("Samples volcanic surface shape, lava, and magma. Base biome height remains part of ordinary biome sampling.");
    ui.checkbox(&mut options.springs, "Hot springs")
        .on_hover_text("Checks generated hot spring surfaces, including dry spring terrain.");
    ui.checkbox(&mut options.exposed_dikes, "Exposed dikes")
        .on_hover_text(
            "Samples exposed igneous intrusions. This adds geology work near suitable terrain.",
        );
    ui.checkbox(
        &mut options.large_rocks_special_soils,
        "Large rocks and special soils",
    )
    .on_hover_text("Checks large surface rocks and TFG special soil or ground patches.");
    });
    ui.checkbox(&mut options.surface_litter, "Surface litter")
        .on_hover_text("Samples sticks, loose rocks, ground leaves, dry grass, salt, and other small surface decorations.");
}

pub fn show(
    ui: &mut Ui,
    prefs: &mut DisplayPrefs,
    biomes: &[(u16, String)],
    rocks: &[String],
    ores: &[FeatureOption<'_>],
    structures: &[FeatureOption<'_>],
    dimension: Dimension,
    fallback: Layer,
    allow_detail_rendering: &mut bool,
    surface_detail: &mut bool,
) -> DisplayChanges {
    let mut changes = DisplayChanges::default();

    if dimension == Dimension::Overworld {
        ui.horizontal_wrapped(|ui| {
            for (preset, label) in [
                (Preset::Exploration, "Explore"),
                (Preset::Prospecting, "Prospect"),
                (Preset::Settlement, "Settle"),
                (Preset::Forestry, "Forests"),
            ] {
                if ui.selectable_label(prefs.preset == preset, label).clicked() {
                    let layer = prefs.apply_preset(preset);
                    prefs.set_layer_enabled(layer, true, fallback);
                    changes.layer = Some(layer);
                    changes.preset = true;
                }
            }
        });
    }

    super::menus::settings(ui, "Rendering", |ui| {
        ui.checkbox(allow_detail_rendering, "Allow block detail")
            .on_hover_text("Show full terrain blocks at close zoom on an unstyled biome map.");
        ui.add_enabled(
            *allow_detail_rendering,
            egui::Checkbox::new(surface_detail, "Detail at close zoom"),
        )
        .on_hover_text("Composed layers, grass colors, trees, relief and filters use the raster map at close zoom.");
        egui::ComboBox::from_id_salt("terralens-display-palette")
            .selected_text(match prefs.palette {
                Palette::Natural => "Natural colors",
                Palette::Colorblind => "Colorblind colors",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut prefs.palette, Palette::Natural, "Natural colors");
                ui.selectable_value(&mut prefs.palette, Palette::Colorblind, "Colorblind colors");
            });
        egui::ComboBox::from_id_salt("terralens-loading-filter")
            .selected_text(format!("Loading filter · {}", prefs.loading_filter.label()))
            .show_ui(ui, |ui| {
                for filter in LoadingFilter::ALL {
                    ui.selectable_value(&mut prefs.loading_filter, filter, filter.label());
                }
            })
            .response
            .on_hover_text("Choose how map images are enlarged while sharper tiles load.");
        egui::ComboBox::from_id_salt("terralens-map-maximum-detail")
            .selected_text(format!("Maximum detail · {}", prefs.max_detail.label()))
            .show_ui(ui, |ui| {
                for detail in MapDetail::ALL {
                    ui.selectable_value(&mut prefs.max_detail, detail, detail.label());
                }
            })
            .response
            .on_hover_text("Limits the finest map sampling and block detail. One block gives the most detail and uses the most processing.");
        if dimension != Dimension::Beneath {
            terrain_shading_controls(ui, prefs);
            contour_controls(ui, &mut prefs.contours);
        }
    });

    if dimension == Dimension::Overworld {
        super::menus::settings(ui, "Climate and biome filters", |ui| {
            ui.menu_button(
                format!("Highlight biomes · {}", prefs.highlighted_biomes.len()),
                |ui| {
                    selection_list(
                        ui,
                        biomes.iter().map(|(id, name)| (*id, name.as_str())),
                        &mut prefs.highlighted_biomes,
                        240.,
                    );
                },
            );
            ui.menu_button(
                format!("Highlight rock · {}", prefs.highlighted_rocks.len()),
                |ui| {
                    selection_list(
                        ui,
                        rocks.iter().map(|name| (name.clone(), name.as_str())),
                        &mut prefs.highlighted_rocks,
                        200.,
                    );
                },
            );
            ui.checkbox(&mut prefs.temperature_enabled, "Temperature range");
            if prefs.temperature_enabled {
                ui.add(egui::Slider::new(&mut prefs.temperature_min, -30.0..=30.0).text("min"));
                ui.add(egui::Slider::new(&mut prefs.temperature_max, -30.0..=30.0).text("max"));
            }
            if prefs.temperature_min > prefs.temperature_max {
                std::mem::swap(&mut prefs.temperature_min, &mut prefs.temperature_max);
            }
            ui.checkbox(&mut prefs.rainfall_enabled, "Rainfall range");
            if prefs.rainfall_enabled {
                ui.add(egui::Slider::new(&mut prefs.rainfall_min, 0.0..=500.0).text("min"));
                ui.add(egui::Slider::new(&mut prefs.rainfall_max, 0.0..=500.0).text("max"));
            }
            if prefs.rainfall_min > prefs.rainfall_max {
                std::mem::swap(&mut prefs.rainfall_min, &mut prefs.rainfall_max);
            }
        });
    }

    super::menus::settings(ui, "Feature visibility", |ui| {
        ui.menu_button("Ores", |ui| {
            visibility_list(ui, ores, &mut prefs.hidden_ore_ids)
        });
        ui.menu_button("Structures", |ui| {
            visibility_list(ui, structures, &mut prefs.hidden_structure_ids)
        });
    });

    changes
}

/// The single control for the raster composition. Active rows are shown in
/// render order, from foreground to background; the arrows change that order.
pub(super) fn layer_composition(
    ui: &mut Ui,
    prefs: &mut DisplayPrefs,
    dimension: Dimension,
    fallback: Layer,
) -> Option<Layer> {
    layer_composition_recorded(ui, prefs, dimension, fallback, |_, _, _, _| {})
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LayerControl {
    Toggle,
    Opacity,
    Foreground,
    Background,
}

fn layer_composition_recorded(
    ui: &mut Ui,
    prefs: &mut DisplayPrefs,
    dimension: Dimension,
    fallback: Layer,
    mut record: impl FnMut(Layer, LayerControl, egui::Rect, bool),
) -> Option<Layer> {
    let mut focus = None;
    let active = prefs.active_layers(fallback, dimension);

    for (visual_index, blend) in active.iter().rev().enumerate() {
        let layer = blend.layer;
        let synthetic_fallback = !prefs.layer_enabled(layer, fallback);
        let can_foreground = visual_index > 0;
        let can_background = visual_index + 1 < active.len();
        let mut controls = Vec::new();
        ui.horizontal(|ui| {
            if synthetic_fallback {
                let mut checked = true;
                let response = ui
                    .add_enabled_ui(false, |ui| {
                        ui.add_sized(
                            [112.0, ui.spacing().interact_size.y],
                            egui::Checkbox::new(&mut checked, layer.label()),
                        )
                    })
                    .inner
                    .on_hover_text("No selected layer is available in this dimension.");
                controls.push((LayerControl::Toggle, response, false));
            } else {
                let mut enabled = true;
                let response = ui
                    .add_sized(
                        [112.0, ui.spacing().interact_size.y],
                        egui::Checkbox::new(&mut enabled, layer.label()),
                    )
                    .on_hover_text(layer_help(layer));
                if response.changed() {
                    prefs.set_layer_enabled(layer, false, fallback);
                }
                controls.push((LayerControl::Toggle, response, true));
            }

            if !synthetic_fallback {
                let mut opacity = blend.opacity;
                let slider = ui
                    .add_sized(
                        [58.0, ui.spacing().interact_size.y],
                        egui::Slider::new(&mut opacity, 0.0..=1.0)
                            .show_value(false)
                            .clamping(egui::SliderClamping::Always),
                    )
                    .on_hover_text(format!(
                        "{} opacity · {:.0}%",
                        layer.label(),
                        blend.opacity * 100.0
                    ));
                if slider.changed() {
                    prefs.set_layer_opacity(layer, opacity, fallback);
                }
                controls.push((LayerControl::Opacity, slider, true));
                ui.add_sized(
                    [32.0, ui.spacing().interact_size.y],
                    egui::Label::new(format!("{:.0}%", opacity * 100.0)),
                );
                let foreground = ui
                    .add_enabled_ui(can_foreground, |ui| {
                        ui.add_sized([22.0, ui.spacing().interact_size.y], egui::Button::new("↑"))
                            .on_hover_text("Move to foreground")
                    })
                    .inner;
                if foreground.clicked() {
                    prefs.move_layer(layer, 1, fallback, dimension);
                    focus = Some(layer);
                }
                controls.push((LayerControl::Foreground, foreground, can_foreground));
                let background = ui
                    .add_enabled_ui(can_background, |ui| {
                        ui.add_sized([22.0, ui.spacing().interact_size.y], egui::Button::new("↓"))
                            .on_hover_text("Move to background")
                    })
                    .inner;
                if background.clicked() {
                    prefs.move_layer(layer, -1, fallback, dimension);
                    focus = Some(layer);
                }
                controls.push((LayerControl::Background, background, can_background));
            } else {
                ui.add_sized([58.0, ui.spacing().interact_size.y], egui::Label::new(""));
                ui.add_sized([32.0, ui.spacing().interact_size.y], egui::Label::new(""));
                ui.allocate_space(egui::vec2(22.0, ui.spacing().interact_size.y));
                ui.allocate_space(egui::vec2(22.0, ui.spacing().interact_size.y));
            }
        });
        for (control, response, enabled) in controls {
            record(layer, control, response.rect, enabled);
        }
        if focus.is_some() {
            break;
        }
    }

    let inactive: Vec<_> = Layer::ALL
        .into_iter()
        .filter(|layer| {
            DisplayPrefs::supports_layer(dimension, *layer)
                && !active.iter().any(|blend| blend.layer == *layer)
        })
        .collect();
    let has_inactive = !inactive.is_empty();
    if !inactive.is_empty() {
        if !active.is_empty() {
            ui.separator();
        }
        for layer in inactive {
            let mut enabled = false;
            let mut response = None;
            ui.horizontal(|ui| {
                let toggle = ui
                    .add_sized(
                        [112.0, ui.spacing().interact_size.y],
                        egui::Checkbox::new(&mut enabled, layer.label()),
                    )
                    .on_hover_text(layer_help(layer));
                if toggle.changed() {
                    response = Some(toggle.clone());
                    prefs.set_layer_enabled(layer, true, fallback);
                    focus = Some(layer);
                }
                ui.add_sized([58.0, ui.spacing().interact_size.y], egui::Label::new(""));
                ui.add_sized([32.0, ui.spacing().interact_size.y], egui::Label::new(""));
                ui.allocate_space(egui::vec2(22.0, ui.spacing().interact_size.y));
                ui.allocate_space(egui::vec2(22.0, ui.spacing().interact_size.y));
                if response.is_none() {
                    response = Some(toggle);
                }
            });
            if let Some(response) = response {
                record(layer, LayerControl::Toggle, response.rect, true);
            }
            if focus.is_some() {
                break;
            }
        }
    }

    if dimension == Dimension::Overworld {
        if !active.is_empty() || has_inactive {
            ui.separator();
        }
        ui.horizontal(|ui| {
            ui.add_sized(
                [112.0, ui.spacing().interact_size.y],
                egui::Checkbox::new(&mut prefs.grass_colors, "Grass colors"),
            )
            .on_hover_text(
                "Uses climate-tinted grass and ground material colors. Active thematic layers render above them.",
            );
            if prefs.grass_colors {
                ui.add_sized(
                    [58.0, ui.spacing().interact_size.y],
                    egui::Slider::new(&mut prefs.grass_opacity, 0.0..=1.0)
                        .show_value(false)
                        .clamping(egui::SliderClamping::Always),
                ).on_hover_text(format!("Grass color opacity · {:.0}%", prefs.grass_opacity * 100.0));
                ui.label(format!("{:.0}%", prefs.grass_opacity * 100.0));
            } else {
                ui.add_sized([58.0, ui.spacing().interact_size.y], egui::Label::new(""));
                ui.add_sized([32.0, ui.spacing().interact_size.y], egui::Label::new(""));
            }
            ui.add_sized([22.0, ui.spacing().interact_size.y], egui::Label::new(""));
            ui.add_sized([22.0, ui.spacing().interact_size.y], egui::Label::new(""));
        });
        ui.horizontal(|ui| {
            ui.add_sized(
                [112.0, ui.spacing().interact_size.y],
                egui::Checkbox::new(&mut prefs.trees, "Trees"),
            )
            .on_hover_text(
                "Trees render above map layers, ground relief and contour lines. Uses saved tree surfaces in generated chunks and installed tree shapes elsewhere, including exposed branches and dead trees. Available closer than 4 blocks per pixel, independently of full terrain detail.",
            );
            if prefs.trees {
                ui.add_sized(
                    [58.0, ui.spacing().interact_size.y],
                    egui::Slider::new(&mut prefs.tree_opacity, 0.0..=1.0)
                        .show_value(false)
                        .clamping(egui::SliderClamping::Always),
                )
                .on_hover_text(format!("Tree opacity · {:.0}%", prefs.tree_opacity * 100.0));
                ui.label(format!("{:.0}%", prefs.tree_opacity * 100.0));
            } else {
                ui.add_sized([58.0, ui.spacing().interact_size.y], egui::Label::new(""));
                ui.add_sized([32.0, ui.spacing().interact_size.y], egui::Label::new(""));
            }
            ui.add_sized([22.0, ui.spacing().interact_size.y], egui::Label::new(""));
            ui.add_sized([22.0, ui.spacing().interact_size.y], egui::Label::new(""));
        });
    }
    if dimension != Dimension::Beneath {
        ui.separator();
        terrain_shading_controls(ui, prefs);
    }
    focus
}

fn terrain_shading_controls(ui: &mut Ui, prefs: &mut DisplayPrefs) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut prefs.height_shading, "Terrain shading")
            .on_hover_text(
                "Xaero-style terrain lighting and depth on every map layer. Trees use their own crown heights. Use 100% for Xaero's terrain strength.",
            );
        if prefs.height_shading {
            ui.add_sized(
                [78., ui.spacing().interact_size.y],
                egui::Slider::new(&mut prefs.hillshade_strength, 0.0..=1.0)
                    .show_value(false)
                    .clamping(egui::SliderClamping::Always),
            )
            .on_hover_text("Terrain shading strength");
            ui.label(format!("{:.0}%", prefs.hillshade_strength * 100.));
        }
    });
}

fn layer_help(layer: Layer) -> &'static str {
    if layer == Layer::Ground {
        "Show exposed TFC soil, sand variants, gravel and rock. Saved blocks are used where available; unexplored terrain follows TFG surface rules."
    } else {
        "Add this map layer to the composition."
    }
}

pub fn contour_controls(ui: &mut Ui, prefs: &mut ContourPrefs) {
    ui.checkbox(&mut prefs.enabled, "Terrain height lines");
    if !prefs.enabled {
        return;
    }
    ui.horizontal(|ui| {
        ui.label("Interval");
        ui.add(
            egui::DragValue::new(&mut prefs.interval)
                .range(1..=512)
                .suffix(" blocks"),
        )
        .on_hover_text("Draw a contour at each multiple of this elevation interval.");
    });
    ui.horizontal(|ui| {
        ui.label("Bold every");
        ui.add(
            egui::DragValue::new(&mut prefs.major_every)
                .range(0..=20)
                .suffix(" lines"),
        )
        .on_hover_text("Emphasize every nth contour. Zero turns off bold lines.");
    });
    ui.add(egui::Slider::new(&mut prefs.width, 0.3..=2.0).text("Thickness"));
    ui.add(egui::Slider::new(&mut prefs.opacity, 0.0..=1.0).text("Opacity"));
    ui.horizontal(|ui| {
        ui.label("Line color");
        ui.color_edit_button_srgb(&mut prefs.color);
    });
    ui.checkbox(&mut prefs.underwater, "Underwater lines");
}

fn selection_list<'a, T: Clone + Eq>(
    ui: &mut Ui,
    values: impl Iterator<Item = (T, &'a str)>,
    selected: &mut Vec<T>,
    height: f32,
) {
    let values: Vec<(T, String)> = values.map(|(id, label)| (id, label.to_owned())).collect();
    ui.horizontal(|ui| {
        if ui.small_button("All").clicked() {
            *selected = values.iter().map(|(id, _)| id.clone()).collect();
        }
        if ui.small_button("Clear").clicked() {
            selected.clear();
        }
    });
    egui::ScrollArea::vertical()
        .max_height(height)
        .show(ui, |ui| {
            for (id, label) in &values {
                let mut checked = selected.contains(id);
                if ui.checkbox(&mut checked, label).changed() {
                    if checked {
                        selected.push(id.clone());
                    } else {
                        selected.retain(|value| value != id);
                    }
                }
            }
        });
}

fn visibility_list(ui: &mut Ui, values: &[FeatureOption<'_>], hidden: &mut Vec<String>) {
    ui.horizontal(|ui| {
        if ui.small_button("Show all").clicked() {
            hidden.clear();
        }
        if ui.small_button("Hide all").clicked() {
            *hidden = values.iter().map(|entry| entry.id.to_owned()).collect();
        }
    });
    egui::ScrollArea::vertical()
        .max_height(190.)
        .show(ui, |ui| {
            for entry in values {
                let mut visible = !hidden.iter().any(|id| id == entry.id);
                if ui.checkbox(&mut visible, entry.label).changed() {
                    if visible {
                        hidden.retain(|id| id != entry.id);
                    } else if !hidden.iter().any(|id| id == entry.id) {
                        hidden.push(entry.id.to_owned());
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::LayerBlend;
    use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};

    #[derive(Clone, Copy)]
    struct Hit {
        layer: Layer,
        control: LayerControl,
        rect: Rect,
        enabled: bool,
    }

    fn layer_frame(
        ctx: &egui::Context,
        prefs: &mut DisplayPrefs,
        dimension: Dimension,
        events: Vec<Event>,
    ) -> (Vec<Hit>, Rect) {
        let mut hits = Vec::new();
        let mut menu_rect = Rect::NOTHING;
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    super::super::menus::bar().ui(ui, |ui| {
                        menu_rect = ui
                            .menu_button("Layers", |ui| {
                                layer_composition_recorded(
                                    ui,
                                    prefs,
                                    dimension,
                                    Layer::Biomes,
                                    |layer, control, rect, enabled| {
                                        hits.push(Hit {
                                            layer,
                                            control,
                                            rect,
                                            enabled,
                                        });
                                    },
                                );
                            })
                            .response
                            .rect;
                    });
                });
            },
        );
        (hits, menu_rect)
    }

    fn draw_layers(
        ctx: &egui::Context,
        prefs: &mut DisplayPrefs,
        dimension: Dimension,
    ) -> Vec<Hit> {
        layer_frame(ctx, prefs, dimension, Vec::new()).0
    }

    fn open_layers(
        ctx: &egui::Context,
        prefs: &mut DisplayPrefs,
        dimension: Dimension,
    ) -> Vec<Hit> {
        let (_, rect) = layer_frame(ctx, prefs, dimension, Vec::new());
        click_layer_control(ctx, prefs, dimension, rect);
        draw_layers(ctx, prefs, dimension)
    }

    fn click_layer_control(
        ctx: &egui::Context,
        prefs: &mut DisplayPrefs,
        dimension: Dimension,
        rect: Rect,
    ) {
        let pos = rect.center();
        for pressed in [true, false] {
            layer_frame(
                ctx,
                prefs,
                dimension,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn layer_rows_add_remove_and_reorder_without_clearing_other_layers() {
        let ctx = egui::Context::default();
        let mut prefs = DisplayPrefs {
            layer_stack: Some(vec![
                LayerBlend {
                    layer: Layer::Biomes,
                    opacity: 1.0,
                },
                LayerBlend {
                    layer: Layer::Temperature,
                    opacity: 0.55,
                },
            ]),
            ..Default::default()
        };
        let hits = open_layers(&ctx, &mut prefs, Dimension::Overworld);
        let top_arrow = hits
            .iter()
            .find(|hit| hit.layer == Layer::Temperature && hit.control == LayerControl::Foreground)
            .unwrap();
        assert!(!top_arrow.enabled);
        let add = hits
            .iter()
            .find(|hit| hit.layer == Layer::Rainfall && hit.control == LayerControl::Toggle)
            .unwrap();
        click_layer_control(&ctx, &mut prefs, Dimension::Overworld, add.rect);
        assert!(prefs.layer_enabled(Layer::Rainfall, Layer::Biomes));
        assert!(prefs.layer_enabled(Layer::Biomes, Layer::Biomes));
        assert!(prefs.layer_enabled(Layer::Temperature, Layer::Biomes));

        let hits = draw_layers(&ctx, &mut prefs, Dimension::Overworld);
        let move_temperature = hits
            .iter()
            .find(|hit| hit.layer == Layer::Temperature && hit.control == LayerControl::Foreground)
            .unwrap();
        assert!(move_temperature.enabled);
        click_layer_control(
            &ctx,
            &mut prefs,
            Dimension::Overworld,
            move_temperature.rect,
        );
        let order: Vec<_> = prefs
            .active_layers(Layer::Biomes, Dimension::Overworld)
            .into_iter()
            .map(|blend| blend.layer)
            .collect();
        assert_eq!(order, [Layer::Biomes, Layer::Rainfall, Layer::Temperature]);

        let hits = draw_layers(&ctx, &mut prefs, Dimension::Overworld);
        let disable_rain = hits
            .iter()
            .find(|hit| hit.layer == Layer::Rainfall && hit.control == LayerControl::Toggle)
            .unwrap();
        click_layer_control(&ctx, &mut prefs, Dimension::Overworld, disable_rain.rect);
        assert!(!prefs.layer_enabled(Layer::Rainfall, Layer::Biomes));
        assert!(prefs.layer_enabled(Layer::Biomes, Layer::Biomes));
        assert!(prefs.layer_enabled(Layer::Temperature, Layer::Biomes));

        assert!(!draw_layers(&ctx, &mut prefs, Dimension::Overworld).is_empty());
        click_layer_control(
            &ctx,
            &mut prefs,
            Dimension::Overworld,
            Rect::from_center_size(Pos2::new(750., 550.), Vec2::splat(1.)),
        );
        assert!(draw_layers(&ctx, &mut prefs, Dimension::Overworld).is_empty());
        assert!(!open_layers(&ctx, &mut prefs, Dimension::Overworld).is_empty());
        layer_frame(
            &ctx,
            &mut prefs,
            Dimension::Overworld,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(draw_layers(&ctx, &mut prefs, Dimension::Overworld).is_empty());
    }

    #[test]
    fn unavailable_dimension_layer_has_a_disabled_fallback_and_quality_settings_render() {
        let ctx = egui::Context::default();
        let mut prefs = DisplayPrefs {
            layer_stack: Some(vec![LayerBlend {
                layer: Layer::Temperature,
                opacity: 1.0,
            }]),
            ..Default::default()
        };
        let hits = open_layers(&ctx, &mut prefs, Dimension::Moon);
        assert!(hits.iter().any(|hit| {
            hit.layer == Layer::Biomes && hit.control == LayerControl::Toggle && !hit.enabled
        }));
        assert!(
            prefs
                .active_layers(Layer::Temperature, Dimension::Moon)
                .iter()
                .any(|entry| entry.layer == Layer::Biomes)
        );

        let ctx = egui::Context::default();
        let mut prefs = DisplayPrefs::default();
        let mut allow_detail = true;
        let mut surface_detail = true;
        let output = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(
                        ui,
                        &mut prefs,
                        &[],
                        &[],
                        &[],
                        &[],
                        Dimension::Moon,
                        Layer::Biomes,
                        &mut allow_detail,
                        &mut surface_detail,
                    );
                });
            },
        );
        let rendering_pos = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                Shape::Text(text) if text.galley.job.text == "Rendering" => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()).center())
                }
                _ => None,
            })
            .expect("rendering submenu is visible in the Moon display options");
        for pressed in [true, false] {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    events: vec![
                        Event::PointerMoved(rendering_pos),
                        Event::PointerButton {
                            pos: rendering_pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show(
                            ui,
                            &mut prefs,
                            &[],
                            &[],
                            &[],
                            &[],
                            Dimension::Moon,
                            Layer::Biomes,
                            &mut allow_detail,
                            &mut surface_detail,
                        );
                    });
                },
            );
        }
        let opened = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(
                        ui,
                        &mut prefs,
                        &[],
                        &[],
                        &[],
                        &[],
                        Dimension::Moon,
                        Layer::Biomes,
                        &mut allow_detail,
                        &mut surface_detail,
                    );
                });
            },
        );
        let labels: Vec<_> = opened
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.iter().any(|label| label.contains("Maximum detail")));
        assert!(
            labels
                .iter()
                .any(|label| label.contains("Nearest neighbor"))
        );
        assert!(labels.iter().any(|label| label.contains("Terrain shading")));
    }
}
