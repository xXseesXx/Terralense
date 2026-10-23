//! Compact map controls. Labels remain direct controls; this panel adds no headings.
use crate::{
    atlas::Layer,
    display::{DisplayPrefs, Palette, Preset},
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
}

pub fn show(
    ui: &mut Ui,
    prefs: &mut DisplayPrefs,
    biomes: &[(u16, String)],
    rocks: &[String],
    ores: &[FeatureOption<'_>],
    structures: &[FeatureOption<'_>],
) -> DisplayChanges {
    let mut changes = DisplayChanges::default();

    ui.horizontal_wrapped(|ui| {
        for (preset, label) in [
            (Preset::Exploration, "Explore"),
            (Preset::Prospecting, "Prospect"),
            (Preset::Settlement, "Settle"),
            (Preset::Forestry, "Forests"),
        ] {
            if ui.selectable_label(prefs.preset == preset, label).clicked() {
                changes.layer = Some(prefs.apply_preset(preset));
            }
        }
    });

    egui::ComboBox::from_id_salt("terralens-display-palette")
        .selected_text(match prefs.palette {
            Palette::Natural => "Natural colors",
            Palette::Colorblind => "Colorblind colors",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut prefs.palette, Palette::Natural, "Natural colors");
            ui.selectable_value(&mut prefs.palette, Palette::Colorblind, "Colorblind colors");
        });

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

    ui.vertical(|ui| {
        ui.checkbox(&mut prefs.temperature_enabled, "Temperature range");
        if prefs.temperature_enabled {
            ui.add(egui::Slider::new(&mut prefs.temperature_min, -30.0..=30.0).text("min"));
            ui.add(egui::Slider::new(&mut prefs.temperature_max, -30.0..=30.0).text("max"));
        }
    });
    if prefs.temperature_min > prefs.temperature_max {
        std::mem::swap(&mut prefs.temperature_min, &mut prefs.temperature_max);
    }
    ui.vertical(|ui| {
        ui.checkbox(&mut prefs.rainfall_enabled, "Rainfall range");
        if prefs.rainfall_enabled {
            ui.add(egui::Slider::new(&mut prefs.rainfall_min, 0.0..=500.0).text("min"));
            ui.add(egui::Slider::new(&mut prefs.rainfall_max, 0.0..=500.0).text("max"));
        }
    });
    if prefs.rainfall_min > prefs.rainfall_max {
        std::mem::swap(&mut prefs.rainfall_min, &mut prefs.rainfall_max);
    }

    ui.menu_button("Ore visibility", |ui| {
        visibility_list(ui, ores, &mut prefs.hidden_ore_ids);
    });
    ui.menu_button("Structure visibility", |ui| {
        visibility_list(ui, structures, &mut prefs.hidden_structure_ids);
    });

    changes
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
