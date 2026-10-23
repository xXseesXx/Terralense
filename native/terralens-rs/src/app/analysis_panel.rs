//! Compact controls and result rows for engine-backed area analysis.
use crate::{
    analysis::{LocationQuery, LocationResults, Query, Report},
    model::{Bounds, Point},
};
use eframe::egui::{self, Color32, RichText};

#[derive(Clone, Debug)]
pub enum Action {
    Analyze(Query),
    Locate(LocationQuery),
}

pub fn show_controls(
    ui: &mut egui::Ui,
    query: &mut LocationQuery,
    bounds: Bounds,
    mode: u8,
) -> Option<Action> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        if mode==0 && ui.button("Analyze area").clicked() {
            action = Some(Action::Analyze(Query {
                bounds,
                sample_budget: 4096,
            }));
        }
        if mode==1 && ui.button("Find locations").on_hover_text("Combine climate, nearby rock and predicted ore requirements; sample the current seed").clicked() {
            action = Some(Action::Locate(query.clone()));
        }
    });
    if mode == 0 {
        return action;
    }
    ui.horizontal(|ui| {
        ui.label("Radius");
        ui.add(
            egui::DragValue::new(&mut query.radius)
                .speed(128.)
                .range(128.0..=32_000.0)
                .suffix(" blocks"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Temperature");
        number(ui, &mut query.min_temperature, "min");
        number(ui, &mut query.max_temperature, "max");
    });
    ui.horizontal(|ui| {
        ui.label("Rainfall");
        number(ui, &mut query.min_rainfall, "min");
        number(ui, &mut query.max_rainfall, "max");
    });
    ui.horizontal(|ui| {
        ui.label("Surface rock");
        let mut s = query.rock.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::singleline(&mut s)
                    .desired_width(112.)
                    .hint_text("optional"),
            )
            .changed()
        {
            query.rock = (!s.trim().is_empty()).then_some(s);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Ore nearby");
        let mut s = query.ore.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::singleline(&mut s)
                    .desired_width(112.)
                    .hint_text("optional"),
            )
            .changed()
        {
            query.ore = (!s.trim().is_empty()).then_some(s);
        }
    });
    if query.rock.as_ref().is_some_and(|s| !s.trim().is_empty()) {
        ui.horizontal(|ui| {
            ui.label("Rock within");
            ui.add(
                egui::DragValue::new(&mut query.rock_distance)
                    .range(0.0..=2048.0)
                    .speed(16.)
                    .suffix(" blocks"),
            );
        });
    }
    if query.ore.as_ref().is_some_and(|s| !s.trim().is_empty()) {
        ui.horizontal(|ui| {
            ui.label("Ore within");
            ui.add(
                egui::DragValue::new(&mut query.ore_distance)
                    .range(128.0..=1024.0)
                    .speed(16.)
                    .suffix(" blocks"),
            );
        });
    }
    action
}
fn number(ui: &mut egui::Ui, v: &mut Option<f64>, hint: &str) {
    let id = ui.next_auto_id().with(hint);
    let mut text = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| v.map(|n| n.to_string()).unwrap_or_default());
    if ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(60.)
                .hint_text(hint),
        )
        .changed()
    {
        *v = text.trim().parse::<f64>().ok().filter(|n| n.is_finite());
        ui.ctx().data_mut(|d| d.insert_temp(id, text));
    }
}

pub fn show_report(ui: &mut egui::Ui, report: Option<&Report>) {
    let Some(r) = report else { return };
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        metric(
            ui,
            "Land / sea",
            format!("{} / {}", r.land_samples, r.ocean_samples),
        );
        metric(ui, "Samples", r.sample_count.to_string());
        metric(ui, "Grid", format!("{:.0} blocks", r.step_blocks));
    });
    if r.land_samples == 0 {
        ui.label(&r.note);
        return;
    }
    ui.horizontal_wrapped(|ui| {
        metric(
            ui,
            "Temp",
            format!(
                "{:.1}–{:.1} · avg {:.1}",
                r.climate.temperature_min, r.climate.temperature_max, r.climate.temperature_mean
            ),
        );
        metric(
            ui,
            "Rain",
            format!(
                "{:.1}–{:.1} · avg {:.1}",
                r.climate.rainfall_min, r.climate.rainfall_max, r.climate.rainfall_mean
            ),
        );
    });
    compact_shares(ui, "Biomes", &r.biomes, 5);
    compact_shares(ui, "Rock", &r.rocks, 5);
    compact_shares(ui, "Forest", &r.forests, 5);
    if !r.tree_species.is_empty() {
        ui.label(
            RichText::new(format!("Trees · ~{:.0}/ha", r.trees_per_hectare))
                .color(Color32::from_rgb(152, 205, 174))
                .size(11.),
        );
        compact_shares(ui, "", &r.tree_species, 4);
    }
    ui.label(
        RichText::new(&r.note)
            .color(Color32::from_gray(135))
            .size(10.),
    );
}
fn metric(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(
        RichText::new(label)
            .color(Color32::from_gray(145))
            .size(10.),
    );
    ui.label(
        RichText::new(value)
            .color(Color32::from_rgb(222, 232, 229))
            .size(11.),
    );
}
fn compact_shares(ui: &mut egui::Ui, label: &str, items: &[crate::analysis::Share], max: usize) {
    if items.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if !label.is_empty() {
            ui.label(
                RichText::new(label)
                    .color(Color32::from_gray(150))
                    .size(10.),
            );
        }
        for v in items.iter().take(max) {
            ui.label(
                RichText::new(format!(
                    "{} {}%",
                    v.name
                        .rsplit('/')
                        .next()
                        .unwrap_or(&v.name)
                        .replace('_', " "),
                    if v.percent < 5. {
                        format!("{:.1}", v.percent)
                    } else {
                        format!("{:.0}", v.percent)
                    }
                ))
                .color(Color32::from_rgb(198, 211, 208))
                .size(10.),
            );
        }
    });
}

pub fn show_results(ui: &mut egui::Ui, results: Option<&LocationResults>) -> Option<Point> {
    let r = results?;
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        metric(
            ui,
            "Land",
            format!(
                "{:.0}%",
                100. * r.land_samples as f64 / r.samples.max(1) as f64
            ),
        );
        metric(ui, "Samples", r.samples.to_string());
        metric(ui, "Grid", format!("{:.0} blocks", r.step_blocks));
    });
    let mut selected = None;
    ui.label(
        RichText::new(
            "Squares mark sampled matches. Pin a site to show its rock and ore evidence.",
        )
        .size(10.)
        .color(Color32::from_gray(150)),
    );
    egui::ScrollArea::vertical()
        .max_height(230.)
        .show(ui, |ui| {
            for c in &r.candidates {
                let text = format!(
                    "{:.0}, {:.0}  ·  {:.1} km  ·  {}",
                    c.point.x,
                    c.point.z,
                    c.distance / 1000.,
                    c.biome
                );
                if ui.selectable_label(false, text)
                    .on_hover_text("Pin this sampled site and show its matching rock and predicted ore on the map")
                    .clicked() {
                    selected = Some(c.point);
                }
                ui.label(
                    RichText::new(format!(
                        "{:.1} °C · {:.0} mm · {}{}",
                        c.temperature,
                        c.rainfall,
                        c.rock,
                        c.ore_distance
                            .map(|d| format!(" · ore ~{d:.0} blocks"))
                            .unwrap_or_default()
                    ))
                    .size(10.)
                    .color(Color32::from_gray(150)),
                );
            }
        });
    ui.label(
        RichText::new(&r.note)
            .size(10.)
            .color(Color32::from_gray(135)),
    );
    selected
}
