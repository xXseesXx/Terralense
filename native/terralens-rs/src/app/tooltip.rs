//! Compact hover content and persisted display preferences.
use crate::model::{Marker, Probe, TreeEstimate};
use eframe::egui::{self, Color32, CornerRadius, Frame, RichText, Stroke};
use serde::{Deserialize, Serialize};

const CARD: Color32 = Color32::from_rgb(18, 27, 34);
const BORDER: Color32 = Color32::from_rgb(54, 76, 82);
const MUTED: Color32 = Color32::from_rgb(151, 171, 174);
const TEAL: Color32 = Color32::from_rgb(103, 217, 198);

/// The hover card starts deliberately small. Extra world-generation layers are opt-in.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct TooltipPrefs {
    pub height: bool,
    pub geology: bool,
    pub ground: bool,
    pub forest: bool,
    pub trees: bool,
    pub region: bool,
}

impl TooltipPrefs {
    pub fn show_preferences(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.height, "Elevation");
        ui.checkbox(&mut self.geology, "Rock layers");
        ui.checkbox(&mut self.ground, "Ground material");
        ui.checkbox(&mut self.forest, "Forest density");
        ui.checkbox(&mut self.trees, "Tree estimates");
        ui.checkbox(&mut self.region, "Region details");
    }
}

pub fn show_surface(
    ui: &mut egui::Ui,
    column: &crate::prism::SurfaceColumn,
    point: crate::model::Point,
    saved: bool,
    dimension: crate::dimension::Dimension,
    slice_y: i32,
) {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(13, 11))
        .show(ui, |ui| {
            ui.set_max_width(270.);
            ui.label(
                RichText::new(display_marker_name(&column.block))
                    .strong()
                    .color(Color32::from_rgb(232, 241, 239)),
            );
            ui.label(
                RichText::new(format!(
                    "{:.0}, {}, {:.0}",
                    point.x.floor(),
                    column.y,
                    point.z.floor()
                ))
                .monospace()
                .size(11.)
                .color(MUTED),
            );
            if dimension == crate::dimension::Dimension::Beneath {
                let (base, top) = dimension.band(slice_y);
                value_row(
                    ui,
                    "SLICE",
                    format!("Y {base}–{top} · block Y={}", column.y),
                );
            }
            if dimension == crate::dimension::Dimension::Overworld
                && let Some(tree) = &column.tree
            {
                value_row(ui, "TREE", display_marker_name(tree));
            }
            if dimension == crate::dimension::Dimension::Overworld
                && let Some(crop) = &column.crop
            {
                value_row(ui, "CROP", display_marker_name(crop));
            }
            if saved {
                ui.label(RichText::new("Saved blocks").size(10.).color(MUTED));
            }
        });
}

/// Draw a quiet, concise hover card. The core facts remain visible at all times.
pub fn show_probe(
    ui: &mut egui::Ui,
    probe: &Probe,
    trees: Option<&TreeEstimate>,
    region_summary: Option<(f64, bool)>,
    continent_scale: bool,
    dimension: crate::dimension::Dimension,
    layer: crate::atlas::Layer,
    slice_y: i32,
    prefs: &TooltipPrefs,
) {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(13, 11))
        .show(ui, |ui| {
            ui.set_max_width(252.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if dimension.is_planetary() {
                        dimension.label().to_ascii_uppercase()
                    } else if dimension == crate::dimension::Dimension::Beneath {
                        "BENEATH".to_owned()
                    } else if probe.surface_magma.is_some() {
                        "MAGMA".to_owned()
                    } else if probe.surface_lava.is_some() {
                        "LAVA".to_owned()
                    } else if probe.surface_water_y.is_some() {
                        "WATER".to_owned()
                    } else if probe.ocean {
                        "SEA".to_owned()
                    } else if probe.surface_water {
                        "WATER".to_owned()
                    } else {
                        "LAND".to_owned()
                    })
                    .size(9.0)
                    .strong()
                    .color(TEAL),
                );
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(clean_biome_name(&probe.biome))
                            .size(14.0)
                            .strong()
                            .color(Color32::from_rgb(232, 241, 239)),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{}, {}",
                            compact_coord(probe.point.x),
                            compact_coord(probe.point.z)
                        ))
                        .size(11.0)
                        .monospace()
                        .color(MUTED),
                    );
                });
            });
            if dimension == crate::dimension::Dimension::Beneath {
                ui.add_space(5.0);
                let (base, top) = dimension.band(slice_y);
                metric(ui, "SLICE", format!("{base}–{top}"));
            } else if !probe.is_bare_surface() {
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    metric(ui, "TEMP", format!("{:.1}°C", probe.climate.temperature));
                    ui.add_space(15.0);
                    metric(ui, "RAIN", format!("{:.0} mm", probe.climate.rainfall));
                });
            }

            if dimension == crate::dimension::Dimension::Overworld
                && (prefs.ground || layer == crate::atlas::Layer::Ground)
                && let Some(block) = probe.ground_block.as_deref()
            {
                ui.add_space(6.0);
                let row = ui.horizontal(|ui| {
                    metric(ui, "GROUND", display_marker_name(block));
                    if let Some(y) = probe.ground_y {
                        ui.add_space(8.0);
                        metric(ui, "Y", y.to_string());
                    }
                });
                let source = if probe.ground_saved {
                    "Read from saved blocks"
                } else {
                    "Estimated from TFG surface rules"
                };
                row.response.on_hover_text(format!("{block} · {source}"));
            }

            if (dimension == crate::dimension::Dimension::Overworld || dimension.is_planetary())
                && prefs.height
                && let Some(height) = probe
                    .surface_lava
                    .or(probe.surface_magma)
                    .or(probe.surface_water_y)
                    .map(f64::from)
                    .or(probe.height)
            {
                ui.add_space(6.0);
                metric(ui, "Y", format!("{height:.0}"));
            }
            if dimension == crate::dimension::Dimension::Overworld
                && prefs.geology
                && !probe.rocks.iter().all(String::is_empty)
            {
                ui.add_space(6.0);
                value_row(
                    ui,
                    "ROCK",
                    probe
                        .rocks
                        .iter()
                        .filter(|v| !v.is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" · "),
                );
            }
            if dimension == crate::dimension::Dimension::Overworld
                && prefs.forest
                && !probe.is_bare_surface()
            {
                ui.add_space(6.0);
                value_row(
                    ui,
                    "FOREST",
                    crate::search::forest_name(probe.forest).to_owned(),
                );
            }
            if dimension == crate::dimension::Dimension::Overworld
                && prefs.region
                && !probe.is_bare_surface()
            {
                ui.add_space(6.0);
                if let Some((area_km2, clipped)) = region_summary {
                    let level = if continent_scale {
                        "CONTINENT"
                    } else {
                        "BIOME"
                    };
                    let suffix = if clipped { " · edge clipped" } else { "" };
                    value_row(ui, level, format!("~{area_km2:.1} km²{suffix}"));
                } else {
                    value_row(
                        ui,
                        "HOTSPOT",
                        format!("{:.2} · age class {:.0}", probe.hotspot, probe.hotspot_age),
                    );
                }
            }
            if dimension == crate::dimension::Dimension::Overworld
                && prefs.trees
                && !probe.is_bare_surface()
            {
                ui.add_space(6.0);
                if let Some(trees) = trees {
                    value_row(
                        ui,
                        "TREE ATTEMPTS",
                        format!("~{:.0}  ·  {:.0}/ha", trees.total, trees.per_hectare),
                    );
                    if !trees.species.is_empty() {
                        let names = trees
                            .species
                            .iter()
                            .take(4)
                            .map(|s| format!("{} {:.0}", s.name, s.count))
                            .collect::<Vec<_>>()
                            .join("  ·  ");
                        ui.label(RichText::new(names).size(10.0).color(MUTED));
                    }
                } else {
                    ui.label(
                        RichText::new("Zoom in and hover a land biome for an estimate")
                            .size(10.0)
                            .color(MUTED),
                    );
                }
            }
        });
}

/// Expanded marker facts shown beneath the compact pinned summary.
pub fn show_marker(ui: &mut egui::Ui, marker: &Marker) {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(13, 11))
        .show(ui, |ui| {
            ui.set_min_width(205.0);
            if let Some(bounds) = marker.bounds {
                value_row(
                    ui,
                    "BOUNDS X/Z",
                    format!(
                        "{}–{}, {}–{}",
                        bounds.min_x, bounds.max_x, bounds.min_z, bounds.max_z
                    ),
                );
                value_row(ui, "BOUNDS Y", format!("{}–{}", bounds.min_y, bounds.max_y));
            }
            if let Some(y) = marker.y {
                value_row(ui, "NATIVE Y", format!("{y:.0}"));
            }
            if let (Some(low), Some(high)) = (marker.min_y, marker.max_y)
                && low != high
            {
                value_row(ui, "Y RANGE", format!("{low:.0}–{high:.0}"));
            }
            if !marker.detail.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&marker.detail).size(11.0).color(MUTED));
            }
        });
}

fn metric(ui: &mut egui::Ui, label: &str, value: String) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(9.0).strong().color(MUTED));
        ui.label(
            RichText::new(value)
                .size(12.0)
                .color(Color32::from_rgb(224, 236, 232)),
        );
    });
}

fn value_row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(label).size(9.0).strong().color(MUTED));
        ui.label(
            RichText::new(value)
                .size(11.0)
                .color(Color32::from_rgb(224, 236, 232)),
        );
    });
}

fn compact_coord(v: f64) -> String {
    if v.fract().abs() < 0.01 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

fn clean_biome_name(name: &str) -> String {
    let name = name
        .rsplit([':', '/'])
        .next()
        .unwrap_or(name)
        .replace('_', " ");
    name.split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn display_marker_name(name: &str) -> String {
    name.rsplit(':')
        .next()
        .unwrap_or(name)
        .split('/')
        .filter(|part| !part.is_empty() && *part != "predicted_surface")
        .map(clean_biome_name)
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn marker_name(marker: &Marker) -> String {
    crate::structure_loot::display_name(&marker.id)
        .map(str::to_owned)
        .unwrap_or_else(|| display_marker_name(&marker.label))
}
