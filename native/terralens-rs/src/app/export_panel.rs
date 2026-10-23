//! Compact export controls with functional labels and no decorative headings.
use crate::{
    atlas::{self, Layer, LegendSnapshot},
    display::Palette,
    exports::{self, Composition, LegendChip, Request, ScaleLegend},
    model::{Bounds, Marker},
};
use eframe::egui::{self, Ui};
use std::path::PathBuf;
pub struct State {
    dir: String,
    width: u32,
    height: u32,
    keep_aspect: bool,
    compose: bool,
    coordinates: bool,
    scale_bar: bool,
    layer_legend: bool,
    status: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            dir: exports::default_directory().to_string_lossy().into_owned(),
            width: 1600,
            height: 1000,
            keep_aspect: true,
            compose: true,
            coordinates: true,
            scale_bar: true,
            layer_legend: true,
            status: None,
        }
    }
}
impl State {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        bounds: Bounds,
        layer: Layer,
        markers: &[Marker],
        selected: Option<&Marker>,
        legend_snapshot: Option<&LegendSnapshot>,
        palette: Palette,
    ) -> Option<Request> {
        let mut req = None;
        if self.keep_aspect && bounds.max_x > bounds.min_x && bounds.max_z > bounds.min_z {
            self.height = (self.width as f64 * (bounds.max_z - bounds.min_z)
                / (bounds.max_x - bounds.min_x))
                .round()
                .clamp(1., 4096.) as u32;
        }
        ui.checkbox(&mut self.keep_aspect, "Keep map proportions");
        ui.checkbox(&mut self.compose, "Compose markers and map details");
        if self.compose {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.coordinates, "Coordinates");
                ui.checkbox(&mut self.scale_bar, "Scale bar");
                ui.checkbox(&mut self.layer_legend, "Layer legend");
            });
        }
        ui.horizontal(|ui| {
            ui.label("Save to");
            ui.add(egui::TextEdit::singleline(&mut self.dir).desired_width(ui.available_width()));
        });
        ui.horizontal(|ui| {
            ui.label("PNG size");
            ui.add(
                egui::DragValue::new(&mut self.width)
                    .range(1..=4096)
                    .suffix(" px"),
            );
            ui.add_enabled(
                !self.keep_aspect,
                egui::DragValue::new(&mut self.height)
                    .range(1..=4096)
                    .suffix(" px"),
            );
        });
        if ui
            .button("Export map PNG")
            .on_hover_text(
                "Regenerate this seed preview and export it with optional share-ready overlays",
            )
            .clicked()
        {
            let file = if self.compose {
                format!("map-{}-composed.png", layer_name(layer))
            } else {
                format!("map-{}.png", layer_name(layer))
            };
            let path = PathBuf::from(&self.dir).join(file);
            req = Some(if self.compose {
                let legend = if self.layer_legend {
                    match layer {
                        Layer::Biomes => legend_snapshot
                            .filter(|s| s.layer == layer)
                            .map(|s| {
                                s.entries
                                    .iter()
                                    .take(5)
                                    .map(|e| LegendChip {
                                        label: e.name.clone(),
                                        color: e.color,
                                        percent: None,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        Layer::Geology => geology_names()
                            .into_iter()
                            .take(5)
                            .map(|name| LegendChip {
                                color: atlas::rock_legend_color(&name, palette),
                                label: name.replace('_', " "),
                                percent: None,
                            })
                            .collect(),
                        Layer::Forest => (0..=4)
                            .map(|category| LegendChip {
                                color: atlas::forest_legend_color(category, palette),
                                label: crate::search::forest_name(category).into(),
                                percent: None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    }
                } else {
                    Vec::new()
                };
                let scale = if self.layer_legend {
                    atlas::layer_scale(layer, palette).map(|s| ScaleLegend {
                        min: s.min,
                        max: s.max,
                        unit: s.unit.into(),
                        low: s.low.into(),
                        high: s.high.into(),
                        stops: s.stops,
                    })
                } else {
                    None
                };
                Request::ComposedMap {
                    bounds,
                    layer,
                    width: self.width,
                    height: self.height,
                    path,
                    markers: markers.to_vec(),
                    composition: Composition {
                        coordinates: self.coordinates,
                        scale_bar: self.scale_bar,
                        legend,
                        scale,
                    },
                }
            } else {
                Request::Map {
                    bounds,
                    layer,
                    width: self.width,
                    height: self.height,
                    path,
                }
            });
        }
        ui.small("PNG uses fresh seed predictions; it does not include saved-block cutaways.");
        if ui
            .button("Export markers CSV")
            .on_hover_text("Export markers currently shown on the map")
            .clicked()
        {
            req = Some(Request::Markers {
                markers: markers.to_vec(),
                path: PathBuf::from(&self.dir).join("markers.csv"),
            });
        }
        if let Some(m) = selected {
            if ui
                .button("Copy Xaero share text")
                .on_hover_text("Paste this in Minecraft chat; Xaero players can add the waypoint")
                .clicked()
            {
                let y = m.y.unwrap_or(64.).round().clamp(-2048., 4096.) as i32;
                ui.ctx()
                    .copy_text(exports::xaero_share(&m.label, m.point(), y, color_for(m)));
                self.status = Some("Xaero share copied".into());
            }
        }
        if let Some(s) = &self.status {
            ui.label(s);
        }
        req
    }
}
fn layer_name(l: Layer) -> &'static str {
    match l {
        Layer::Biomes => "biomes",
        Layer::Temperature => "temperature",
        Layer::Rainfall => "rainfall",
        Layer::Height => "height",
        Layer::Water => "water",
        Layer::Geology => "geology",
        Layer::Forest => "forest",
        Layer::Hotspots => "hotspots",
    }
}
fn geology_names() -> Vec<String> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../assets/data/tfg/rock-layers.json"))
            .expect("bundled TFG rock layers are valid");
    let mut names = std::collections::BTreeSet::new();
    for group in ["bottom", "land", "oceanFloor", "volcanic", "uplift"] {
        if let Some(rocks) = value[group].as_array() {
            names.extend(
                rocks
                    .iter()
                    .filter_map(|rock| rock.as_str())
                    .map(str::to_owned),
            );
        }
    }
    if let Some(layers) = value["layers"].as_array() {
        for layer in layers {
            if let Some(entries) = layer["entries"].as_array() {
                names.extend(
                    entries
                        .iter()
                        .filter_map(|entry| entry.as_array()?.first()?.as_str().map(str::to_owned)),
                );
            }
        }
    }
    names.into_iter().collect()
}
fn color_for(m: &Marker) -> u8 {
    match &m.kind {
        crate::model::MarkerKind::Structure => 14,
        crate::model::MarkerKind::Ore => 6,
        crate::model::MarkerKind::Biome => 11,
        crate::model::MarkerKind::Rock => 3,
        crate::model::MarkerKind::Forest => 13,
        crate::model::MarkerKind::Tree => 10,
    }
}
