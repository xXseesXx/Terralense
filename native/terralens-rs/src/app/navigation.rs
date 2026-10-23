//! Coordinate navigation, view history, and saved map locations.
//!
//! This module deliberately has no panel headings. `show_coordinate_jump` can be
//! called once per frame; it owns the Ctrl+G popup and returns a view to apply.
use crate::dimension::Dimension;
use crate::model::Point;
use eframe::egui::{self, Key, Modifiers};
use serde::{Deserialize, Serialize};

const HISTORY_LIMIT: usize = 128;
const MAX_BOOKMARKS: usize = 512;
const WORLD_BORDER: f64 = 29_999_984.0;

fn default_slice_y() -> i32 {
    96
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct View {
    pub world_id: String,
    pub seed: Option<i64>,
    #[serde(default)]
    pub dimension: Dimension,
    #[serde(default = "default_slice_y")]
    pub slice_y: i32,
    pub center: Point,
    pub bpp: f64,
}

impl Default for View {
    fn default() -> Self {
        Self {
            world_id: String::new(),
            seed: None,
            dimension: Dimension::Overworld,
            slice_y: 96,
            center: Point::default(),
            bpp: 32.0,
        }
    }
}

impl View {
    fn valid(&self) -> bool {
        self.center.x.is_finite()
            && self.center.z.is_finite()
            && self.center.x.abs() <= WORLD_BORDER
            && self.center.z.abs() <= WORLD_BORDER
            && self.bpp.is_finite()
            && self.bpp > 0.0
            && self.slice_y == self.dimension.clamp_y(self.slice_y)
    }

    fn same_location(&self, other: &Self) -> bool {
        self.world_id == other.world_id
            && self.seed == other.seed
            && self.dimension == other.dimension
            && (self.dimension != crate::dimension::Dimension::Beneath
                || self.slice_y == other.slice_y)
            && self.center.x == other.center.x
            && self.center.z == other.center.z
            && self.bpp == other.bpp
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct Bookmark {
    pub id: String,
    pub name: String,
    pub note: String,
    pub view: View,
    /// Optional RGB accent, kept as plain data for portable preferences.
    pub color: Option<[u8; 3]>,
    pub collection: String,
}

impl Default for Bookmark {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            note: String::new(),
            view: View::default(),
            color: None,
            collection: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct Collection {
    pub id: String,
    pub name: String,
}

impl Default for Collection {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
        }
    }
}

/// Persist this with the app's existing eframe preferences. UI-only state is
/// skipped so opening a popup never changes the saved session data.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Navigation {
    history: Vec<View>,
    cursor: usize,
    pub bookmarks: Vec<Bookmark>,
    pub collections: Vec<Collection>,
    next_id: u64,
    #[serde(skip)]
    jump_open: bool,
    #[serde(skip)]
    selected_collection: String,
    #[serde(skip)]
    coordinate_input: String,
    #[serde(skip)]
    coordinate_error: Option<String>,
    #[serde(skip)]
    bookmark_name_input: String,
    #[serde(skip)]
    collection_name_input: String,
    #[serde(skip)]
    editing_bookmark: Option<String>,
}

impl Default for Navigation {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            cursor: 0,
            bookmarks: Vec::new(),
            collections: vec![Collection {
                id: "default".into(),
                name: "Default".into(),
            }],
            next_id: 1,
            jump_open: false,
            selected_collection: String::new(),
            coordinate_input: String::new(),
            coordinate_error: None,
            bookmark_name_input: String::new(),
            collection_name_input: String::new(),
            editing_bookmark: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Action {
    Navigate(View),
}

impl Navigation {
    /// Record deliberate destinations, not each pan or zoom frame. Nearby
    /// points are treated as the same visit unless the zoom changes materially.
    pub fn record(&mut self, view: View) {
        self.clamp_cursor();
        if !view.valid() {
            return;
        }
        if self.history.is_empty() {
            self.history.push(view);
            self.cursor = 0;
            return;
        }
        let current = &self.history[self.cursor.min(self.history.len() - 1)];
        if !is_meaningful_visit(current, &view) {
            return;
        }
        self.history.truncate(self.cursor + 1);
        self.history.push(view);
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.cursor = self.history.len() - 1;
    }

    pub fn back(&mut self, current: &View) -> Option<View> {
        self.clamp_cursor();
        self.align_current(current);
        if self.cursor == 0 || self.history.is_empty() {
            return None;
        }
        self.cursor -= 1;
        self.history.get(self.cursor).cloned()
    }

    pub fn forward(&mut self, current: &View) -> Option<View> {
        self.clamp_cursor();
        self.align_current(current);
        if self.history.is_empty() || self.cursor + 1 >= self.history.len() {
            return None;
        }
        self.cursor += 1;
        self.history.get(self.cursor).cloned()
    }

    fn align_current(&mut self, current: &View) {
        if !current.valid() {
            return;
        }
        if self.history.is_empty() {
            self.history.push(current.clone());
            self.cursor = 0;
        } else if !self.history[self.cursor.min(self.history.len() - 1)].same_location(current) {
            self.record(current.clone());
        }
    }

    pub fn add_bookmark(&mut self, view: View, name: impl Into<String>) -> Option<String> {
        if !view.valid() || self.bookmarks.len() >= MAX_BOOKMARKS {
            return None;
        }
        let id = format!("spot-{}", self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let mut name = name.into().trim().to_owned();
        if name.is_empty() {
            name = format!(
                "{}, {}",
                compact_coord(view.center.x),
                compact_coord(view.center.z)
            );
        }
        self.bookmarks.push(Bookmark {
            id: id.clone(),
            name,
            note: String::new(),
            view,
            color: None,
            collection: "default".into(),
        });
        Some(id)
    }

    pub fn remove_bookmark(&mut self, id: &str) -> bool {
        let before = self.bookmarks.len();
        self.bookmarks.retain(|bookmark| bookmark.id != id);
        self.bookmarks.len() != before
    }

    pub fn update_bookmark(&mut self, id: &str, name: String, note: String) -> bool {
        let Some(bookmark) = self.bookmarks.iter_mut().find(|bookmark| bookmark.id == id) else {
            return false;
        };
        if !name.trim().is_empty() {
            bookmark.name = name.trim().to_owned();
        }
        bookmark.note = note;
        true
    }

    pub fn create_collection(&mut self, name: impl Into<String>) -> Option<String> {
        let name = name.into().trim().to_owned();
        if name.is_empty() || self.collections.len() >= 128 {
            return None;
        }
        let id = format!("collection-{}", self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.collections.push(Collection {
            id: id.clone(),
            name,
        });
        Some(id)
    }

    pub fn move_bookmark(&mut self, bookmark_id: &str, collection_id: &str) -> bool {
        if !self
            .collections
            .iter()
            .any(|collection| collection.id == collection_id)
        {
            return false;
        }
        let Some(bookmark) = self
            .bookmarks
            .iter_mut()
            .find(|bookmark| bookmark.id == bookmark_id)
        else {
            return false;
        };
        bookmark.collection = collection_id.to_owned();
        true
    }

    pub fn remove_collection(&mut self, collection_id: &str) -> bool {
        if collection_id == "default" {
            return false;
        }
        if self.selected_collection == collection_id {
            self.selected_collection.clear();
        }
        let before = self.collections.len();
        self.collections
            .retain(|collection| collection.id != collection_id);
        if self.collections.len() == before {
            return false;
        }
        for bookmark in &mut self.bookmarks {
            if bookmark.collection == collection_id {
                bookmark.collection = "default".into();
            }
        }
        true
    }

    #[cfg(test)]
    pub fn find_bookmark(&self, id: &str) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|bookmark| bookmark.id == id)
    }

    pub fn history(&self) -> &[View] {
        &self.history
    }

    /// Upgrade old folder-name identities after an explicit, unambiguous world import.
    pub fn remap_world(&mut self, old: &str, new: &str) {
        for view in self
            .history
            .iter_mut()
            .chain(self.bookmarks.iter_mut().map(|bookmark| &mut bookmark.view))
        {
            if view.world_id == old {
                view.world_id = new.to_owned();
            }
        }
    }

    pub fn can_back(&self) -> bool {
        !self.history.is_empty() && self.cursor.min(self.history.len() - 1) > 0
    }

    pub fn can_forward(&self) -> bool {
        !self.history.is_empty() && self.cursor.min(self.history.len() - 1) + 1 < self.history.len()
    }

    /// Clamp persisted indices and discard malformed or excess saved locations.
    pub fn normalize(&mut self) {
        self.history.truncate(HISTORY_LIMIT);
        self.cursor = self.cursor.min(self.history.len().saturating_sub(1));
        self.history.retain(View::valid);
        if self.history.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(self.history.len() - 1);
        }
        self.bookmarks.truncate(MAX_BOOKMARKS);
        self.bookmarks.retain(|bookmark| bookmark.view.valid());
        for view in self
            .history
            .iter_mut()
            .chain(self.bookmarks.iter_mut().map(|b| &mut b.view))
        {
            view.slice_y = if view.dimension == crate::dimension::Dimension::Beneath {
                view.dimension.band(view.slice_y).1
            } else {
                64
            };
        }
        if !self
            .collections
            .iter()
            .any(|collection| collection.id == "default")
        {
            self.collections.insert(
                0,
                Collection {
                    id: "default".into(),
                    name: "Default".into(),
                },
            );
        }
        self.collections.truncate(128);
        let ids: std::collections::HashSet<_> =
            self.collections.iter().map(|c| c.id.as_str()).collect();
        for bookmark in &mut self.bookmarks {
            if !ids.contains(bookmark.collection.as_str()) {
                bookmark.collection = "default".into();
            }
        }
    }

    fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.history.len().saturating_sub(1));
    }

    pub fn open_jump(&mut self, current: &View) {
        self.jump_open = true;
        self.coordinate_error = None;
        self.coordinate_input = format!(
            "{} {}",
            compact_coord(current.center.x),
            compact_coord(current.center.z)
        );
    }

    /// Compact toolbar controls, with no title or section heading. The returned
    /// navigation action should be applied after drawing the toolbar.
    pub fn controls(&mut self, ui: &mut egui::Ui, current: &View) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled_ui(self.can_back(), |ui| {
                    super::icons::button(ui, super::icons::Icon::Back)
                })
                .inner
                .on_hover_text("Back through map visits")
                .clicked()
            {
                action = self.back(current).map(Action::Navigate);
            }
            if ui
                .add_enabled_ui(self.can_forward(), |ui| {
                    super::icons::button(ui, super::icons::Icon::Forward)
                })
                .inner
                .on_hover_text("Forward through map visits")
                .clicked()
            {
                action = self.forward(current).map(Action::Navigate);
            }
            if super::icons::button(ui, super::icons::Icon::Locate)
                .on_hover_text("Go to coordinates (Ctrl+G)")
                .clicked()
            {
                self.open_jump(current);
            }
            ui.menu_button("☆", |ui| {
                ui.set_min_width(240.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.bookmark_name_input)
                            .hint_text("Name this map spot")
                            .char_limit(128)
                            .desired_width(170.0),
                    );
                    if ui.button("+").on_hover_text("Save current view").clicked() {
                        self.add_bookmark(current.clone(), self.bookmark_name_input.clone());
                        self.bookmark_name_input.clear();
                        ui.close();
                    }
                });
                egui::ComboBox::from_id_salt("saved-spots-collection")
                    .selected_text(
                        self.collections
                            .iter()
                            .find(|c| c.id == self.selected_collection)
                            .map(|c| c.name.as_str())
                            .unwrap_or("All saved spots"),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.selected_collection,
                            String::new(),
                            "All saved spots",
                        );
                        for collection in &self.collections {
                            ui.selectable_value(
                                &mut self.selected_collection,
                                collection.id.clone(),
                                &collection.name,
                            );
                        }
                    });
                ui.separator();
                let rows: Vec<_> = self
                    .bookmarks
                    .iter()
                    .filter(|b| {
                        self.selected_collection.is_empty()
                            || b.collection == self.selected_collection
                    })
                    .take(256)
                    .cloned()
                    .collect();
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for bookmark in rows {
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(false, &bookmark.name)
                                    .on_hover_text(format!(
                                        "{}, {}{}",
                                        compact_coord(bookmark.view.center.x),
                                        compact_coord(bookmark.view.center.z),
                                        if bookmark.note.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" · {}", bookmark.note)
                                        }
                                    ))
                                    .clicked()
                                {
                                    self.record(current.clone());
                                    self.record(bookmark.view.clone());
                                    action = Some(Action::Navigate(bookmark.view.clone()));
                                    ui.close();
                                }
                                if ui
                                    .small_button("Edit")
                                    .on_hover_text("Edit name and note")
                                    .clicked()
                                {
                                    self.editing_bookmark = Some(bookmark.id.clone());
                                }
                                if ui
                                    .small_button("×")
                                    .on_hover_text("Remove saved spot")
                                    .clicked()
                                {
                                    self.remove_bookmark(&bookmark.id);
                                    if self.editing_bookmark.as_deref() == Some(&bookmark.id) {
                                        self.editing_bookmark = None;
                                    }
                                }
                            });
                            if self.editing_bookmark.as_deref() == Some(&bookmark.id) {
                                let mut name = bookmark.name.clone();
                                let mut note = bookmark.note.clone();
                                ui.add(
                                    egui::TextEdit::singleline(&mut name)
                                        .hint_text("Name")
                                        .char_limit(128)
                                        .desired_width(220.0),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut note)
                                        .hint_text("Note")
                                        .char_limit(512)
                                        .desired_width(220.0),
                                );
                                self.update_bookmark(&bookmark.id, name, note);
                                ui.horizontal(|ui| {
                                    ui.menu_button("Collection", |ui| {
                                        let collections = self.collections.clone();
                                        for collection in collections {
                                            if ui
                                                .selectable_label(
                                                    bookmark.collection == collection.id,
                                                    &collection.name,
                                                )
                                                .clicked()
                                            {
                                                self.move_bookmark(&bookmark.id, &collection.id);
                                                ui.close();
                                            }
                                        }
                                    });
                                    if ui
                                        .small_button("Done")
                                        .on_hover_text("Done editing")
                                        .clicked()
                                    {
                                        self.editing_bookmark = None;
                                    }
                                });
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.collection_name_input)
                            .hint_text("New collection")
                            .char_limit(128)
                            .desired_width(170.0),
                    );
                    if ui.button("+").on_hover_text("Create collection").clicked() {
                        self.create_collection(self.collection_name_input.clone());
                        self.collection_name_input.clear();
                    }
                });
                let removable: Vec<_> = self
                    .collections
                    .iter()
                    .filter(|collection| collection.id != "default")
                    .cloned()
                    .collect();
                if !removable.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for collection in removable {
                            if ui
                                .small_button(format!("{} ×", collection.name))
                                .on_hover_text("Remove collection; saved spots move to Default")
                                .clicked()
                            {
                                self.remove_collection(&collection.id);
                            }
                        }
                    });
                }
            });
        });
        action
    }

    /// Opens from Ctrl+G and accepts coordinates as `X Z`, `X, Z`, `/X,Z`,
    /// or labeled forms such as `x:123 z:-456`. Only the first two numbers are
    /// used, allowing pasted Minecraft-style coordinate strings.
    pub fn show_coordinate_jump(&mut self, ctx: &egui::Context, current: &View) -> Option<Action> {
        let open_shortcut = ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::G));
        if open_shortcut {
            self.jump_open = true;
            self.coordinate_error = None;
            self.coordinate_input = format!(
                "{} {}",
                compact_coord(current.center.x),
                compact_coord(current.center.z)
            );
        }
        if !self.jump_open {
            return None;
        }
        let mut result = None;
        egui::Window::new("##coordinate_jump")
            .id(egui::Id::new("coordinate_jump"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_width(290.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("X/Z");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.coordinate_input)
                            .hint_text("X Z or X, Z")
                            .desired_width(190.0),
                    );
                    response.request_focus();
                    let enter =
                        response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                    if ui
                        .button("Go")
                        .on_hover_text("Go to coordinates (Ctrl+G)")
                        .clicked()
                        || enter
                    {
                        match parse_coordinates(&self.coordinate_input) {
                            Some(center) => {
                                let mut view = current.clone();
                                view.center = center;
                                self.jump_open = false;
                                self.coordinate_error = None;
                                result = Some(Action::Navigate(view));
                            }
                            None => {
                                self.coordinate_error =
                                    Some("Enter two finite X and Z coordinates".into())
                            }
                        }
                    }
                    if ui.button("×").on_hover_text("Close").clicked() {
                        self.jump_open = false;
                    }
                });
                if let Some(error) = &self.coordinate_error {
                    ui.colored_label(egui::Color32::from_rgb(238, 130, 115), error);
                }
                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    self.jump_open = false;
                }
            });
        result
    }
}

fn is_meaningful_visit(a: &View, b: &View) -> bool {
    if a.world_id != b.world_id
        || a.seed != b.seed
        || a.dimension != b.dimension
        || (a.dimension == crate::dimension::Dimension::Beneath && a.slice_y != b.slice_y)
    {
        return true;
    }
    if !a.valid() || !b.valid() {
        return false;
    }
    let span = a.bpp.max(b.bpp) * 8.0;
    a.center.distance(b.center) >= span.max(128.0) || (a.bpp / b.bpp).max(b.bpp / a.bpp) >= 1.75
}

pub(super) fn parse_coordinates(input: &str) -> Option<Point> {
    let mut values = Vec::with_capacity(2);
    let mut token = String::new();
    let mut in_number = false;
    for c in input.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && !in_number) {
            token.push(c);
            in_number = true;
        } else {
            if in_number {
                let value = token.parse::<f64>().ok()?;
                if !value.is_finite() {
                    return None;
                }
                values.push(value);
                token.clear();
                in_number = false;
                if values.len() > 3 {
                    return None;
                }
            }
        }
    }
    let (x, z) = if values.len() >= 3 {
        (values[0], values[2])
    } else if values.len() == 2 {
        (values[0], values[1])
    } else {
        return None;
    };
    (x.abs() <= WORLD_BORDER && z.abs() <= WORLD_BORDER).then_some(Point { x, z })
}

fn compact_coord(value: f64) -> String {
    format!("{value:.0}")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(world: &str, x: f64) -> View {
        View {
            world_id: world.into(),
            center: Point { x, z: 0. },
            ..View::default()
        }
    }

    #[test]
    fn dimension_and_slice_are_separate_navigation_locations() {
        let mut nav = Navigation::default();
        let overworld = view("world", 0.);
        nav.record(overworld.clone());
        let mut beneath = overworld.clone();
        beneath.dimension = Dimension::Beneath;
        beneath.slice_y = 120;
        nav.record(beneath.clone());
        assert_eq!(nav.back(&beneath), Some(overworld));
        assert_eq!(nav.forward(&view("world", 0.)), Some(beneath));
    }

    #[test]
    fn old_saved_view_defaults_to_overworld() {
        let saved = serde_json::json!({
            "world_id": "world",
            "seed": null,
            "center": { "x": 2.0, "z": 3.0 },
            "bpp": 8.0
        });
        let view: View = serde_json::from_value(saved).unwrap();
        assert_eq!(view.dimension, Dimension::Overworld);
        assert_eq!(view.slice_y, 96);
    }
    #[test]
    fn pasted_coordinates_accept_pairs_triplets_and_negative_chunks() {
        for input in [
            "-123, 456",
            "x:-123 z:456",
            "-123 64 456",
            "/tp @s -123 64 456",
        ] {
            assert_eq!(
                parse_coordinates(input),
                Some(Point { x: -123., z: 456. }),
                "{input}"
            );
        }
        assert!(parse_coordinates("30000001 0").is_none());
        assert!(parse_coordinates("not coordinates").is_none());
        assert!(parse_coordinates("1 2 3 4").is_none());
    }
    #[test]
    fn navigation_keeps_world_context_and_branches_after_back() {
        let mut nav = Navigation::default();
        nav.record(view("a", 0.));
        nav.record(view("a", 500.));
        nav.record(view("b", 500.));
        assert_eq!(nav.back(&view("b", 500.)).unwrap().world_id, "a");
        assert_eq!(nav.back(&view("a", 500.)).unwrap().center.x, 0.);
        assert_eq!(nav.forward(&view("a", 0.)).unwrap().center.x, 500.);
        nav.record(view("a", 900.));
        assert!(nav.forward(&view("a", 900.)).is_none());
        assert_eq!(nav.history().len(), 3);
    }
    #[test]
    fn bookmarks_normalize_after_persistence_without_losing_notes() {
        let mut nav = Navigation::default();
        let id = nav.add_bookmark(view("a", -512.), "Copper").unwrap();
        nav.update_bookmark(&id, "Copper".into(), "Prospect this ridge".into());
        let mut restored: Navigation =
            serde_json::from_str(&serde_json::to_string(&nav).unwrap()).unwrap();
        restored.normalize();
        assert_eq!(
            restored.find_bookmark(&id).unwrap().note,
            "Prospect this ridge"
        );
        assert_eq!(restored.find_bookmark(&id).unwrap().view.world_id, "a");
    }
}
