//! Persisted, named seed shortlist and its compact editor.
use eframe::egui;
use serde::{Deserialize, Serialize};

const MAX_SEEDS: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct NamedSeed {
    pub name: String,
    pub seed: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct SeedShortlist {
    pub entries: Vec<NamedSeed>,
}

impl SeedShortlist {
    pub fn normalize(&mut self) {
        self.entries.truncate(MAX_SEEDS);
        for entry in &mut self.entries {
            entry.name = clean_name(&entry.name, entry.seed);
        }
    }

    pub fn add(&mut self, name: &str, seed: i64) -> usize {
        if let Some(index) = self.entries.iter().position(|entry| entry.seed == seed) {
            self.entries[index].name = clean_name(name, seed);
            return index;
        }
        if self.entries.len() == MAX_SEEDS {
            self.entries.remove(0);
        }
        self.entries.push(NamedSeed {
            name: clean_name(name, seed),
            seed,
        });
        self.entries.len() - 1
    }

    pub fn rename(&mut self, index: usize, name: &str) -> bool {
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        entry.name = clean_name(name, entry.seed);
        true
    }

    pub fn remove(&mut self, index: usize) -> Option<NamedSeed> {
        (index < self.entries.len()).then(|| self.entries.remove(index))
    }

    pub fn adjacent(&self, current: Option<i64>, forward: bool) -> Option<i64> {
        if self.entries.is_empty() {
            return None;
        }
        let at = current.and_then(|seed| self.entries.iter().position(|entry| entry.seed == seed));
        let index = match (at, forward) {
            (Some(i), true) => (i + 1) % self.entries.len(),
            (Some(i), false) => (i + self.entries.len() - 1) % self.entries.len(),
            (None, true) => 0,
            (None, false) => self.entries.len() - 1,
        };
        Some(self.entries[index].seed)
    }
}

fn clean_name(name: &str, seed: i64) -> String {
    let name = name.trim().chars().take(64).collect::<String>();
    if name.is_empty() {
        format!("Seed {seed}")
    } else {
        name
    }
}

#[derive(Default)]
pub(super) struct Editor {
    new_name: String,
    rename_index: Option<usize>,
    rename_text: String,
}

pub(super) enum Action {
    AddCurrent(String),
    AddInput(String),
    Activate(i64),
    Remove(usize),
    Rename(usize, String),
    Previous,
    Next,
}

impl Editor {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        list: &SeedShortlist,
        current_seed: Option<i64>,
        seed_input: &str,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        ui.add(
            egui::TextEdit::singleline(&mut self.new_name)
                .hint_text("Name this seed")
                .desired_width(ui.available_width()),
        );
        ui.horizontal(|ui| {
            if ui
                .add_enabled(current_seed.is_some(), egui::Button::new("Add current"))
                .clicked()
            {
                actions.push(Action::AddCurrent(self.new_name.clone()));
            }
            if ui
                .add_enabled(
                    seed_input.trim().parse::<i64>().is_ok(),
                    egui::Button::new("Add input"),
                )
                .clicked()
            {
                actions.push(Action::AddInput(self.new_name.clone()));
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("‹ Previous")
                .on_hover_text("Previous saved seed")
                .clicked()
            {
                actions.push(Action::Previous);
            }
            if ui
                .button("Next ›")
                .on_hover_text("Next saved seed")
                .clicked()
            {
                actions.push(Action::Next);
            }
        });

        let mut activate = None;
        egui::ScrollArea::vertical()
            .max_height(220.)
            .show(ui, |ui| {
                for (index, entry) in list.entries.iter().enumerate() {
                    egui::Frame::new()
                        .fill(ui.visuals().faint_bg_color)
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let menu_width = 30.0;
                                let name_width =
                                    (ui.available_width() - menu_width - 8.0).max(80.0);
                                ui.vertical(|ui| {
                                    let selected = current_seed == Some(entry.seed);
                                    if ui
                                        .add_sized(
                                            [name_width, 21.0],
                                            egui::Button::selectable(
                                                selected,
                                                compact_name(&entry.name),
                                            ),
                                        )
                                        .on_hover_text(&entry.name)
                                        .clicked()
                                    {
                                        activate = Some(entry.seed);
                                    }
                                    ui.label(
                                        egui::RichText::new(entry.seed.to_string())
                                            .monospace()
                                            .size(10.5)
                                            .color(ui.visuals().weak_text_color()),
                                    );
                                });
                                ui.menu_button("…", |ui| {
                                    if ui.button("Rename").clicked() {
                                        self.rename_index = Some(index);
                                        self.rename_text = entry.name.clone();
                                        ui.close();
                                    }
                                    if ui.button("Remove").clicked() {
                                        actions.push(Action::Remove(index));
                                        ui.close();
                                    }
                                });
                            });
                            if self.rename_index == Some(index) {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.rename_text)
                                            .desired_width((ui.available_width() - 44.0).max(80.0)),
                                    );
                                    if ui.small_button("✓").clicked() {
                                        actions
                                            .push(Action::Rename(index, self.rename_text.clone()));
                                        self.rename_index = None;
                                    }
                                    if ui.small_button("×").clicked() {
                                        self.rename_index = None;
                                    }
                                });
                            }
                        });
                }
            });
        if let Some(seed) = activate {
            actions.push(Action::Activate(seed));
        }
        actions
    }
}

fn compact_name(name: &str) -> String {
    let mut chars = name.chars();
    let short: String = chars.by_ref().take(24).collect();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortlist_bounds_names_and_deduplicates_seeds() {
        let mut list = SeedShortlist::default();
        assert_eq!(list.add("  Home  ", 42), 0);
        assert_eq!(list.add("Renamed", 42), 0);
        assert_eq!(list.entries[0].name, "Renamed");
        assert_eq!(list.add("", 43), 1);
        assert_eq!(list.entries[1].name, "Seed 43");
        for seed in 100..230 {
            list.add("candidate", seed);
        }
        assert_eq!(list.entries.len(), MAX_SEEDS);
    }

    #[test]
    fn previous_next_wrap_and_choose_nearest_end_when_unselected() {
        let mut list = SeedShortlist::default();
        list.add("a", 1);
        list.add("b", 2);
        list.add("c", 3);
        assert_eq!(list.adjacent(Some(3), true), Some(1));
        assert_eq!(list.adjacent(Some(1), false), Some(3));
        assert_eq!(list.adjacent(None, true), Some(1));
        assert_eq!(list.adjacent(None, false), Some(3));
    }
}
