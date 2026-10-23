//! Small interactive map legend. Biome counts come from a bounded cache snapshot;
//! this panel never asks the world generator for samples.
use crate::{
    atlas::{self, Layer, LegendSnapshot},
    dimension::Dimension,
    display::DisplayPrefs,
};
use eframe::egui::{self, Align2, Color32, RichText, Ui};

pub struct State {
    open: bool,
    filter: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: true,
            filter: String::new(),
        }
    }
}

impl State {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// Paint the floating legend and return a requested layer change.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        map_rect: egui::Rect,
        snapshot: &LegendSnapshot,
        display: &mut DisplayPrefs,
        dimension: Dimension,
        layer: Layer,
    ) -> Option<Layer> {
        if !self.open {
            egui::Area::new(egui::Id::new("terralens-legend-reopen"))
                .pivot(Align2::RIGHT_BOTTOM)
                .fixed_pos(map_rect.right_bottom() - egui::vec2(12., 12.))
                .show(ctx, |ui| {
                    if ui.small_button("Legend").clicked() {
                        self.open = true;
                    }
                });
            return None;
        }

        let mut layer_change = None;
        let mut open = true;
        egui::Window::new("Legend")
            .id(egui::Id::new("terralens-floating-legend"))
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .collapsible(false)
            .default_width(248.0)
            .max_width(280.0)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(map_rect.right_bottom() - egui::vec2(12., 12.))
            .constrain_to(map_rect.shrink(6.))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if dimension != Dimension::Beneath {
                        let mut selected = layer;
                        egui::ComboBox::from_id_salt("terralens-legend-layer")
                            .selected_text(layer.label())
                            .width(160.0)
                            .show_ui(ui, |ui| {
                                for candidate in Layer::ALL.into_iter().filter(|candidate| {
                                    super::TerraLensApp::layer_supported(dimension, *candidate)
                                }) {
                                    ui.selectable_value(
                                        &mut selected,
                                        candidate,
                                        candidate.label(),
                                    );
                                }
                            });
                        if selected != layer {
                            layer_change = Some(selected);
                        }
                    }
                    if ui.small_button("×").on_hover_text("Hide legend").clicked() {
                        self.open = false;
                    }
                });

                if dimension == Dimension::Beneath || layer == Layer::Biomes {
                    show_biomes(ui, snapshot, display, dimension, &mut self.filter);
                } else if layer == Layer::Geology {
                    show_geology(ui, display, &mut self.filter);
                } else if layer == Layer::Forest {
                    show_forest(ui, display.palette);
                } else if dimension == Dimension::Mars && layer == Layer::Water {
                    ui.horizontal(|ui| {
                        swatch(ui, Color32::from_rgb(78, 119, 139), "River biome");
                    });
                } else if let Some(scale) =
                    atlas::dimension_layer_scale(dimension, layer, display.palette)
                {
                    show_scale(ui, &scale);
                } else {
                    ui.label("No category scale for this layer");
                }
            });
        if !open {
            self.open = false;
        }
        layer_change
    }
}

fn show_biomes(
    ui: &mut Ui,
    snapshot: &LegendSnapshot,
    display: &mut DisplayPrefs,
    dimension: Dimension,
    filter: &mut String,
) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(filter)
                .desired_width(142.0)
                .hint_text("Filter biomes"),
        );
        if ui
            .small_button("Clear")
            .on_hover_text("Clear biome highlighting")
            .clicked()
        {
            display.highlighted_biomes.clear();
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("terralens-legend-biomes")
        .max_height(160.0)
        .show(ui, |ui| {
            let filter = filter.to_ascii_lowercase();
            for entry in snapshot
                .entries
                .iter()
                .filter(|entry| entry.name.to_ascii_lowercase().contains(&filter))
            {
                let name = entry
                    .name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&entry.name)
                    .replace('_', " ");
                let ocean = !dimension.is_planetary()
                    && dimension == Dimension::Overworld
                    && crate::biome::is_ocean(entry.id);
                let percent = (snapshot.loaded > 0)
                    .then(|| format!("{:.1}%", entry.percent(snapshot.loaded)));
                let selected = display.biome_highlighted(entry.id);
                let clicked = category_row(
                    ui,
                    &name,
                    rgb(entry.color),
                    percent.as_deref(),
                    selected,
                    !ocean,
                    ocean.then_some(
                        "Ocean biomes remain visible but cannot be highlighted on land.",
                    ),
                );
                if clicked && !ocean {
                    if selected {
                        display.highlighted_biomes.retain(|id| *id != entry.id);
                    } else if !display.highlighted_biomes.contains(&entry.id) {
                        display.highlighted_biomes.push(entry.id);
                    }
                }
            }
        });
    let coverage = if snapshot.sampled == 0 {
        0.0
    } else {
        snapshot.loaded as f32 * 100.0 / snapshot.sampled as f32
    };
    ui.label(
        RichText::new(format!(
            "Loaded samples · {coverage:.0}% ({}/{})",
            snapshot.loaded, snapshot.sampled
        ))
        .size(10.5)
        .color(Color32::from_gray(180)),
    );
}

fn show_geology(ui: &mut Ui, display: &mut DisplayPrefs, filter: &mut String) {
    const ROCKS: [&str; 20] = [
        "granite",
        "diorite",
        "gabbro",
        "shale",
        "claystone",
        "limestone",
        "conglomerate",
        "dolomite",
        "chert",
        "chalk",
        "rhyolite",
        "basalt",
        "andesite",
        "dacite",
        "quartzite",
        "slate",
        "phyllite",
        "schist",
        "gneiss",
        "marble",
    ];
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(filter)
                .desired_width(142.0)
                .hint_text("Filter rocks"),
        );
        if ui
            .small_button("Clear")
            .on_hover_text("Clear rock highlighting")
            .clicked()
        {
            display.highlighted_rocks.clear();
        }
    });
    egui::ScrollArea::vertical()
        .max_height(156.0)
        .show(ui, |ui| {
            for rock in ROCKS
                .into_iter()
                .filter(|name| name.contains(&filter.to_ascii_lowercase()))
            {
                let selected = display.highlighted_rocks.iter().any(|value| value == rock);
                let clicked = category_row(
                    ui,
                    &rock.replace('_', " "),
                    rgb(atlas::rock_legend_color(rock, display.palette)),
                    None,
                    selected,
                    true,
                    None,
                );
                if clicked {
                    if !selected {
                        display.highlighted_rocks.push(rock.to_owned());
                    } else {
                        display.highlighted_rocks.retain(|value| value != rock);
                    }
                }
            }
        });
}

fn show_forest(ui: &mut Ui, palette: crate::display::Palette) {
    for id in 0..=4 {
        swatch(
            ui,
            rgb(atlas::forest_legend_color(id, palette)),
            crate::search::forest_name(id),
        );
    }
}

fn show_scale(ui: &mut Ui, scale: &atlas::LayerScale) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 12.0), egui::Sense::hover());
    let painter = ui.painter();
    for x in 0..rect.width().ceil() as usize {
        let t = x as f32 / rect.width().max(1.0);
        let scaled = t * (scale.stops.len() - 1) as f32;
        let i = (scaled.floor() as usize).min(scale.stops.len() - 2);
        let f = scaled - i as f32;
        let a = scale.stops[i];
        let b = scale.stops[i + 1];
        let c = Color32::from_rgb(
            (a[0] as f32 * (1.0 - f) + b[0] as f32 * f) as u8,
            (a[1] as f32 * (1.0 - f) + b[1] as f32 * f) as u8,
            (a[2] as f32 * (1.0 - f) + b[2] as f32 * f) as u8,
        );
        painter.line_segment(
            [
                egui::pos2(rect.left() + x as f32, rect.top()),
                egui::pos2(rect.left() + x as f32, rect.bottom()),
            ],
            egui::Stroke::new(1.0_f32, c),
        );
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(scale.low).size(10.5));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(scale.high).size(10.5));
        });
    });
    response.on_hover_ui(|ui| {
        let pointer = ui
            .input(|input| input.pointer.hover_pos())
            .unwrap_or(rect.center());
        let t = ((pointer.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        let value = scale.min + t * (scale.max - scale.min);
        ui.label(if scale.unit.is_empty() {
            format!("{value:.2}")
        } else {
            format!("{value:.1} {}", scale.unit)
        });
    });
}

fn swatch(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.label(RichText::new(label).size(11.5));
    });
}

fn category_row(
    ui: &mut Ui,
    name: &str,
    color: Color32,
    percent: Option<&str>,
    selected: bool,
    selectable: bool,
    tooltip: Option<&str>,
) -> bool {
    let sense = if selectable {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), sense);
    if selected {
        ui.painter().rect_filled(
            rect,
            3.0,
            ui.visuals().selection.bg_fill.gamma_multiply(0.45),
        );
    }
    let swatch = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(5.0, 4.0),
        egui::vec2(12.0, 12.0),
    );
    ui.painter().rect_filled(swatch, 2.0, color);
    let text_color = if selectable || !selected {
        ui.visuals().text_color()
    } else {
        Color32::GRAY
    };
    let font = egui::FontId::proportional(11.5);
    let text_max = percent.map_or(rect.right() - 8.0, |_| rect.right() - 54.0);
    let galley = ui.painter().layout_no_wrap(
        ellipsize(
            ui,
            name,
            (text_max - (swatch.right() + 7.0)).max(8.0),
            &font,
        ),
        font,
        text_color,
    );
    let text_pos = egui::pos2(
        swatch.right() + 7.0,
        rect.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, text_color);
    if let Some(percent) = percent {
        let galley = ui.painter().layout_no_wrap(
            percent.to_owned(),
            egui::FontId::proportional(10.5),
            ui.visuals().weak_text_color(),
        );
        ui.painter().galley(
            egui::pos2(
                rect.right() - galley.size().x - 7.0,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            ui.visuals().weak_text_color(),
        );
    }
    let response = if let Some(tooltip) = tooltip {
        response.on_hover_text(tooltip)
    } else {
        response.on_hover_text(if selected {
            format!("Stop highlighting {name}")
        } else {
            format!("Highlight {name}")
        })
    };
    response.clicked()
}

fn ellipsize(ui: &Ui, text: &str, max_width: f32, font: &egui::FontId) -> String {
    if ui.fonts_mut(|fonts| fonts.glyph_width(font, '…')) > max_width {
        return String::new();
    }
    let mut result = String::new();
    for ch in text.chars() {
        let candidate = format!("{result}{ch}…");
        if ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(candidate, font.clone(), Color32::WHITE)
                .size()
                .x
        }) > max_width
        {
            break;
        }
        result.push(ch);
    }
    if result.chars().count() < text.chars().count() {
        result.push('…');
    } else {
        result = text.to_owned();
    }
    result
}

fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}
