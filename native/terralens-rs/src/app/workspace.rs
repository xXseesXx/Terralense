//! Map actions and compact workspace controls, with no decorative headings.
use super::*;
use crate::{biome, exports};

impl TerraLensApp {
    pub(super) fn current_view(&self) -> navigation::View {
        navigation::View {
            world_id: self.prefs.world_id.clone(),
            seed: self.preview_seed.or_else(|| {
                self.selected
                    .and_then(|i| self.worlds.get(i))
                    .map(|w| w.seed)
            }),
            dimension: self.prefs.dimension,
            slice_y: if self.prefs.dimension == Dimension::Beneath {
                self.prefs.slice_y
            } else {
                64
            },
            center: self.current_center(),
            bpp: self.prefs.blocks_per_point,
        }
    }
    pub(super) fn apply_view(&mut self, view: navigation::View, ctx: &egui::Context) {
        let mut view = view;
        if self.prefs.world_id == view.world_id {
            if let Some(world) = self.selected.and_then(|index| self.worlds.get(index)) {
                if !TerraLensApp::world_supports_dimension(world, view.dimension) {
                    view.dimension = self.prefs.dimension;
                }
            }
        }
        let context_changed = self.prefs.dimension != view.dimension
            || (view.dimension == Dimension::Beneath
                && self.prefs.slice_y != beneath_band_top(view.slice_y));
        let old_key = format!("{}|{}", self.prefs.world_id, self.prefs.dimension.id());
        self.prefs
            .dimension_centers
            .insert(old_key, self.current_center());
        self.prefs.dimension = view.dimension;
        if view.dimension == Dimension::Beneath {
            self.prefs.slice_y = beneath_band_top(view.slice_y);
        }
        let layer = Layer::ALL
            .get(self.prefs.layer)
            .copied()
            .unwrap_or(Layer::Biomes);
        if !TerraLensApp::layer_supported(self.prefs.dimension, layer) {
            self.prefs.layer = 0;
        }
        if !TerraLensApp::scope_supported(self.prefs.dimension, self.prefs.search_scope) {
            self.prefs.search_scope = crate::search::Scope::All;
        }
        if view.world_id != self.prefs.world_id {
            let navigation = std::mem::take(&mut self.prefs.navigation);
            if let Some(index) = self.worlds.iter().position(|w| {
                w.id == view.world_id && TerraLensApp::world_supports_dimension(w, view.dimension)
            }) {
                self.activate_world(index, ctx);
            } else if let Some(seed) = view.seed {
                self.activate_seed(seed, ctx);
            }
            self.prefs.navigation = navigation;
        }
        let new_key = format!("{}|{}", self.prefs.world_id, self.prefs.dimension.id());
        self.prefs.dimension_centers.insert(new_key, view.center);
        if let Some(atlas) = &mut self.atlas {
            atlas.set_dimension(self.prefs.dimension, self.prefs.slice_y);
        }
        self.prefs.center_x = view.center.x.clamp(-30_000_000., 30_000_000.);
        self.prefs.center_z = view.center.z.clamp(-30_000_000., 30_000_000.);
        self.prefs.blocks_per_point = view.bpp.clamp(0.125, 4096.);
        self.search_x = format!("{:.0}", self.prefs.center_x);
        self.search_z = format!("{:.0}", self.prefs.center_z);
        self.tools.clear_selection();
        if context_changed {
            self.selected_marker = None;
            self.search_selected = None;
            self.search_select_after = false;
            self.search_dirty = self.show_search.then(std::time::Instant::now);
        }
    }
    pub(super) fn navigation_controls(&mut self, ui: &mut egui::Ui) {
        let current = self.current_view();
        if let Some(navigation::Action::Navigate(view)) =
            self.prefs.navigation.controls(ui, &current)
        {
            self.apply_view(view, ui.ctx());
        }
    }
    pub(super) fn workspace_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.activate_seed(0, ctx);
            self.seed_input = "0".into();
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            )
        }) {
            self.request_file(file_dialogs::Kind::SaveSession);
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::O,
            )
        }) {
            self.request_file(file_dialogs::Kind::OpenWorld);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::E)) {
            self.request_file(file_dialogs::Kind::Export);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)) {
            self.close_source(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save_session_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.request_file(file_dialogs::Kind::OpenSession);
        }
        let current = self.current_view();
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::D)) {
            self.prefs.navigation.add_bookmark(current.clone(), "");
        }
        let back = ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft));
        let forward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight));
        let view = if back {
            self.prefs.navigation.back(&current)
        } else if forward {
            self.prefs.navigation.forward(&current)
        } else {
            None
        };
        if let Some(view) = view {
            self.apply_view(view, ctx);
        }
        if !ctx.wants_keyboard_input() {
            for (index, key) in [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
                egui::Key::Num6,
                egui::Key::Num7,
                egui::Key::Num8,
            ]
            .into_iter()
            .enumerate()
            {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key))
                    && Layer::ALL.get(index).is_some_and(|layer| {
                        TerraLensApp::layer_supported(self.prefs.dimension, *layer)
                    })
                {
                    self.prefs.layer = index;
                }
            }
        }
    }
    pub(super) fn workspace_popups(&mut self, ctx: &egui::Context) {
        let current = self.current_view();
        if let Some(navigation::Action::Navigate(view)) =
            self.prefs.navigation.show_coordinate_jump(ctx, &current)
        {
            self.prefs.navigation.record(current);
            self.prefs.navigation.record(view.clone());
            self.apply_view(view, ctx);
        }
        if self.session_open {
            egui::Window::new("##session-import")
                .title_bar(false)
                .resizable(true)
                .default_width(400.)
                .show(ctx, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.session_input)
                            .hint_text("Paste a copied session")
                            .desired_width(f32::INFINITY)
                            .desired_rows(5)
                            .char_limit(1_048_576),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Restore pasted session").clicked() {
                            match serde_json::from_str::<serde_json::Value>(&self.session_input) {
                                Ok(value) => self.restore_session(value, ctx),
                                Err(_) => self.session_error = Some("Invalid session".into()),
                            }
                        }
                        if ui.button("×").on_hover_text("Close").clicked() {
                            self.session_open = false;
                        }
                    });
                    if let Some(error) = &self.session_error {
                        ui.colored_label(Color32::LIGHT_RED, error);
                    }
                });
        }
    }
    pub(super) fn session_value(&self) -> serde_json::Value {
        serde_json::json!({"app":"TerraLens","version":1,"preferences":self.prefs,"seed":self.current_view().seed})
    }
    fn restore_session(&mut self, value: serde_json::Value, ctx: &egui::Context) {
        match decode_session(value) {
            Ok(prefs) => self.restore_preferences(prefs, ctx),
            Err(error) => self.session_error = Some(error),
        }
    }
    fn restore_preferences(&mut self, mut prefs: Preferences, ctx: &egui::Context) {
        prefs.upgrade_defaults();
        if !prefs.center_x.is_finite()
            || !prefs.center_z.is_finite()
            || !prefs.blocks_per_point.is_finite()
            || prefs.blocks_per_point <= 0.
        {
            self.session_error = Some("Invalid map coordinates".into());
            return;
        }
        prefs.navigation.normalize();
        prefs.seed_list.normalize();
        let view = navigation::View {
            world_id: prefs.world_id.clone(),
            seed: prefs.seed_preview,
            dimension: prefs.dimension,
            slice_y: prefs.slice_y,
            center: Point {
                x: prefs.center_x,
                z: prefs.center_z,
            },
            bpp: prefs.blocks_per_point,
        };
        if let Some(source) = prefs.source_path.clone() {
            self.open_source_file(source, Some(view));
            self.pending_preferences = Some(prefs);
        } else {
            let old_id = self.prefs.world_id.clone();
            self.prefs = prefs;
            self.legend.set_open(self.prefs.show_legend);
            self.prefs.world_id = old_id;
            self.apply_view(view, ctx);
        }
        self.session_open = false;
    }
    pub(super) fn save_session_file(&mut self) {
        if self.session_path.is_empty() {
            self.request_file(file_dialogs::Kind::SaveSession);
            return;
        }
        let value = self.session_value();
        if let Some(atlas) = &mut self.atlas {
            atlas.export(exports::Request::Data {
                value,
                path: self.session_path.clone().into(),
            });
        }
    }
    pub(super) fn poll_export(&mut self, ctx: &egui::Context) {
        match self.atlas.as_mut().and_then(Atlas::take_export) {
            Some(Ok(outcome)) => {
                if self
                    .pending_session_write
                    .as_ref()
                    .is_some_and(|path| path == &outcome.path)
                {
                    self.session_path = outcome.path.to_string_lossy().into_owned();
                    self.pending_session_write = None;
                }
                if let Some(data) = outcome.data {
                    if let Some(seeds) = data.get("seedList").and_then(serde_json::Value::as_array)
                    {
                        for item in seeds {
                            if let Some(seed) = item["seed"].as_i64() {
                                self.prefs
                                    .seed_list
                                    .add(item["name"].as_str().unwrap_or(""), seed);
                            }
                        }
                        self.set_workspace(Some(Workspace::Seed));
                    } else {
                        self.session_error = None;
                        self.restore_session(data, ctx);
                        if self.pending_preferences.is_some() {
                            self.pending_session_path = Some(outcome.path);
                        } else if self.session_error.is_none() {
                            self.session_path = outcome.path.to_string_lossy().into_owned();
                        } else {
                            self.document_error = self.session_error.clone();
                        }
                    }
                }
            }
            Some(Err(error)) => {
                self.pending_session_write = None;
                self.document_error = Some(error);
            }
            None => {}
        }
    }
    pub(super) fn search_visible(&mut self) {
        let bounds = self.tools.selection().unwrap_or(self.last_bounds);
        let origin = Point {
            x: (bounds.min_x + bounds.max_x) / 2.,
            z: (bounds.min_z + bounds.max_z) / 2.,
        };
        self.search_x = format!("{:.0}", origin.x);
        self.search_z = format!("{:.0}", origin.z);
        self.search_radius = format!(
            "{:.0}",
            ((bounds.max_x - bounds.min_x).hypot(bounds.max_z - bounds.min_z) / 2.)
                .clamp(1., 100_000.)
        );
        self.open_find();
        self.search_dirty = Some(Instant::now());
    }
    pub(super) fn update_display(&mut self, ctx: &egui::Context) {
        if self.prefs.display == self.applied_display {
            self.pending_display = None;
            return;
        }
        match &self.pending_display {
            Some((prefs, since)) if *prefs == self.prefs.display => {
                if since.elapsed() >= Duration::from_millis(180) {
                    if let Some(atlas) = &mut self.atlas {
                        atlas.set_display_options(&self.prefs.display);
                    }
                    self.applied_display = self.prefs.display.clone();
                    self.pending_display = None;
                } else {
                    ctx.request_repaint_after(
                        Duration::from_millis(180).saturating_sub(since.elapsed()),
                    );
                }
            }
            _ => {
                self.pending_display = Some((self.prefs.display.clone(), Instant::now()));
                ctx.request_repaint_after(Duration::from_millis(180));
            }
        }
    }
    pub(super) fn display_controls(&mut self, ui: &mut egui::Ui) {
        if self.prefs.dimension == crate::dimension::Dimension::Beneath {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.prefs.display.palette,
                    crate::display::Palette::Natural,
                    "Natural colors",
                );
                ui.selectable_value(
                    &mut self.prefs.display.palette,
                    crate::display::Palette::Colorblind,
                    "Colorblind colors",
                );
            });
            let catalog = feature_catalog_for(self.prefs.dimension);
            ui.menu_button("Ore visibility", |ui| {
                for (id, label) in &catalog.0 {
                    let mut visible = !self.prefs.display.hidden_ore_ids.contains(id);
                    if ui.checkbox(&mut visible, label).changed() {
                        if visible {
                            self.prefs
                                .display
                                .hidden_ore_ids
                                .retain(|value| value != id);
                        } else if !self.prefs.display.hidden_ore_ids.contains(id) {
                            self.prefs.display.hidden_ore_ids.push(id.clone());
                        }
                    }
                }
            });
            ui.menu_button("Structure visibility", |ui| {
                for (id, label) in &catalog.1 {
                    let mut visible = !self.prefs.display.hidden_structure_ids.contains(id);
                    if ui.checkbox(&mut visible, label).changed() {
                        if visible {
                            self.prefs
                                .display
                                .hidden_structure_ids
                                .retain(|value| value != id);
                        } else if !self.prefs.display.hidden_structure_ids.contains(id) {
                            self.prefs.display.hidden_structure_ids.push(id.clone());
                        }
                    }
                }
            });
            return;
        }
        if self.prefs.dimension.is_planetary() {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.prefs.display.palette,
                    crate::display::Palette::Natural,
                    "Natural colors",
                );
                ui.selectable_value(
                    &mut self.prefs.display.palette,
                    crate::display::Palette::Colorblind,
                    "Colorblind colors",
                );
            });
            let (ores, structures) = feature_catalog_for(self.prefs.dimension);
            ui.menu_button("Ore visibility", |ui| {
                for (id, label) in ores {
                    let mut visible = !self.prefs.display.hidden_ore_ids.contains(id);
                    if ui.checkbox(&mut visible, label).changed() {
                        if visible {
                            self.prefs
                                .display
                                .hidden_ore_ids
                                .retain(|value| value != id);
                        } else if !self.prefs.display.hidden_ore_ids.contains(id) {
                            self.prefs.display.hidden_ore_ids.push(id.clone());
                        }
                    }
                }
            });
            ui.menu_button("Structure visibility", |ui| {
                for (id, label) in structures {
                    let mut visible = !self.prefs.display.hidden_structure_ids.contains(id);
                    if ui.checkbox(&mut visible, label).changed() {
                        if visible {
                            self.prefs
                                .display
                                .hidden_structure_ids
                                .retain(|value| value != id);
                        } else if !self.prefs.display.hidden_structure_ids.contains(id) {
                            self.prefs.display.hidden_structure_ids.push(id.clone());
                        }
                    }
                }
            });
            return;
        }
        let biomes: Vec<_> = biome::ids::BIOME_IDS
            .iter()
            .enumerate()
            .filter(|(i, _)| !biome::is_ocean(*i as u16))
            .map(|(i, name)| {
                (
                    i as u16,
                    name.rsplit('/').next().unwrap_or(name).replace('_', " "),
                )
            })
            .collect();
        let rocks: Vec<_> = [
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
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let catalog = feature_catalog_for(self.prefs.dimension);
        let ores: Vec<_> = catalog
            .0
            .iter()
            .map(|(id, label)| display_panel::FeatureOption { id, label })
            .collect();
        let structures: Vec<_> = catalog
            .1
            .iter()
            .map(|(id, label)| display_panel::FeatureOption { id, label })
            .collect();
        let changes = display_panel::show(
            ui,
            &mut self.prefs.display,
            &biomes,
            &rocks,
            &ores,
            &structures,
        );
        if let Some(layer) = changes.layer {
            self.prefs.layer = Layer::ALL.iter().position(|l| *l == layer).unwrap_or(0);
            match self.prefs.display.preset {
                crate::display::Preset::Prospecting => {
                    self.prefs.show_ores = true;
                    self.prefs.show_structures = false;
                }
                crate::display::Preset::Exploration => self.prefs.show_structures = true,
                crate::display::Preset::Settlement | crate::display::Preset::Forestry => {
                    self.prefs.show_ores = false;
                    self.prefs.show_structures = false;
                }
            }
        }
    }
    pub(super) fn tools_panel(&mut self, ui: &mut egui::Ui) {
        let bounds = self.tools.selection().unwrap_or(self.last_bounds);
        let layer = Layer::ALL
            .get(self.prefs.layer)
            .copied()
            .unwrap_or_default();
        let markers = self.visible_export_markers(bounds);
        let snapshot = self
            .atlas
            .as_mut()
            .map(|atlas| atlas.legend_snapshot(layer).clone());
        self.exporter.show(
            ui,
            bounds,
            self.prefs.dimension,
            layer,
            &markers,
            self.selected_marker.as_ref(),
            snapshot.as_ref(),
            self.prefs.display.palette,
        );
        if let Some(picker) = self.exporter.take_picker() {
            self.request_file(match picker {
                export_panel::Picker::Map => file_dialogs::Kind::Export,
                export_panel::Picker::Markers => file_dialogs::Kind::ExportMarkers,
            });
        }
    }
    pub(super) fn run_qol_scenario(&mut self, ctx: &egui::Context) {
        if !self.qol_smoke || self.discovering {
            return;
        }
        match self.qol_stage {
            0 => {
                if let Some(index) = self
                    .worlds
                    .iter()
                    .position(|w| w.id == "Ganymede" && w.compatible)
                {
                    self.activate_world(index, ctx);
                    self.set_center(Point {
                        x: -2244.,
                        z: -2316.,
                    });
                }
                self.set_workspace(Some(Workspace::Tools));
                self.tools_tab = 0;
                self.prefs.blocks_per_point = 4.;
                self.prefs.show_ores = false;
                self.prefs.show_structures = false;
                self.prefs.tools.coordinate_grid = true;
                let view = self.current_view();
                self.prefs.navigation.add_bookmark(view, "Base candidate");
                let p = self.current_center();
                if let Some(atlas) = &mut self.atlas {
                    atlas.analyze(crate::analysis::Query {
                        bounds: crate::model::Bounds {
                            min_x: p.x - 512.,
                            min_z: p.z - 512.,
                            max_x: p.x + 512.,
                            max_z: p.z + 512.,
                        },
                        sample_budget: 256,
                    });
                }
                self.qol_stage = 1;
            }
            1 => {
                if self
                    .atlas
                    .as_ref()
                    .is_some_and(|a| !a.tasks_busy() && a.analysis_report().is_some())
                {
                    let origin = self.current_center();
                    if let Some(atlas) = &mut self.atlas {
                        atlas.locate(crate::analysis::LocationQuery {
                            origin,
                            radius: 1024.,
                            sample_budget: 256,
                            rock: Some("chalk".into()),
                            rock_distance: 768.,
                            ore: Some("copper".into()),
                            ore_distance: 512.,
                            ..Default::default()
                        });
                    }
                    self.qol_stage = 2;
                }
            }
            2 => {
                if self
                    .atlas
                    .as_ref()
                    .is_some_and(|a| !a.tasks_busy() && a.locations().is_some())
                {
                    if let Some(atlas) = &self.atlas {
                        if let Some(point) = atlas.locations().and_then(|r| r.candidates.first()) {
                            self.selected_marker = Some(point.marker());
                        }
                        if let Some(report) = atlas.analysis_report() {
                            if let Ok(json) = serde_json::to_string_pretty(report) {
                                eprintln!("QoL area report: {json}");
                            }
                        }
                        eprintln!(
                            "QoL locations: {}",
                            atlas.locations().map_or(0, |r| r.candidates.len())
                        );
                    }
                    self.tools_tab = 1;
                    self.qol_stage = 3;
                }
            }
            _ => {}
        }
        ctx.request_repaint_after(Duration::from_millis(20));
    }
    pub(super) fn map_context(
        &mut self,
        _ui: &mut egui::Ui,
        response: &egui::Response,
        rect: Rect,
        center: Point,
    ) {
        if response.secondary_clicked() {
            self.context_point = response
                .interact_pointer_pos()
                .map(|p| screen_to_world(rect, center, self.prefs.blocks_per_point, p));
        }
        let point = self.context_point.unwrap_or(center);
        response.context_menu(|ui| {
            ui.label(format!("{:.0}, {:.0}", point.x, point.z));
            if ui.button("Copy coordinates").clicked() {
                ui.ctx().copy_text(format!(
                    "{}, {}",
                    point.x.floor() as i64,
                    point.z.floor() as i64
                ));
                ui.close();
            }
            if ui.button("Copy teleport command").clicked() {
                ui.ctx().copy_text(format!(
                    "/tp @s {} ~ {}",
                    point.x.floor() as i64,
                    point.z.floor() as i64
                ));
                ui.close();
            }
            if ui.button("Copy chunk / region").clicked() {
                ui.ctx().copy_text(format!(
                    "chunk {}, {} · region {}, {}",
                    (point.x.floor() as i64).div_euclid(16),
                    (point.z.floor() as i64).div_euclid(16),
                    (point.x.floor() as i64).div_euclid(512),
                    (point.z.floor() as i64).div_euclid(512)
                ));
                ui.close();
            }
            if ui.button("Copy seed").clicked() {
                if let Some(seed) = self.current_view().seed {
                    ui.ctx().copy_text(seed.to_string());
                }
                ui.close();
            }
            if ui.button("Save spot").clicked() {
                let mut view = self.current_view();
                view.center = point;
                self.prefs.navigation.add_bookmark(view, "");
                ui.close();
            }
            if ui.button("Search from here").clicked() {
                self.search_x = format!("{:.0}", point.x);
                self.search_z = format!("{:.0}", point.z);
                self.open_find();
                self.search_dirty = Some(Instant::now());
                ui.close();
            }
            if ui.button("Copy Xaero waypoint").clicked() {
                ui.ctx()
                    .copy_text(exports::xaero_share("TerraLens", point, 64, 11));
                ui.close();
            }
        });
    }
    pub(super) fn paint_saved_spots(&mut self, ui: &mut egui::Ui, rect: Rect, center: Point) {
        let painter = ui.painter().with_clip_rect(rect);
        let mut destination = None;
        if let Some(results) = self.atlas.as_ref().and_then(Atlas::locations) {
            for (i, candidate) in results.candidates.iter().enumerate() {
                let pos =
                    world_to_screen(rect, center, self.prefs.blocks_per_point, candidate.point);
                if rect.shrink(8.).contains(pos) {
                    painter.circle_filled(pos, 5., Color32::from_rgb(104, 218, 198));
                    painter.circle_stroke(pos, 6., Stroke::new(1.5_f32, BG));
                    let hit = ui.interact(
                        Rect::from_center_size(pos, Vec2::splat(16.)),
                        Id::new(("location-match", i)),
                        Sense::click(),
                    );
                    hit.clone().on_hover_text(format!(
                        "{} · {:.0}, {:.0}",
                        candidate.biome, candidate.point.x, candidate.point.z
                    ));
                    if hit.clicked() {
                        self.selected_marker = Some(candidate.marker());
                    }
                }
            }
        }
        for spot in self.prefs.navigation.bookmarks.iter().filter(|s| {
            s.view.world_id == self.prefs.world_id
                && s.view.dimension == self.prefs.dimension
                && (self.prefs.dimension != Dimension::Beneath
                    || s.view.slice_y == self.prefs.slice_y)
        }) {
            let pos = world_to_screen(rect, center, self.prefs.blocks_per_point, spot.view.center);
            if !rect.shrink(10.).contains(pos) {
                continue;
            }
            let hit = ui.interact(
                Rect::from_center_size(pos, Vec2::splat(22.)),
                Id::new(("saved-spot", &spot.id)),
                Sense::click(),
            );
            let color = spot
                .color
                .map(|c| Color32::from_rgb(c[0], c[1], c[2]))
                .unwrap_or(Color32::from_rgb(246, 209, 118));
            painter.circle_filled(pos, 8., BG);
            painter.text(
                pos,
                Align2::CENTER_CENTER,
                "★",
                FontId::proportional(16.),
                color,
            );
            if hit.hovered() {
                painter.text(
                    pos + Vec2::new(12., -10.),
                    Align2::LEFT_BOTTOM,
                    &spot.name,
                    FontId::proportional(11.),
                    TEXT,
                );
            }
            hit.clone().on_hover_text(format!(
                "{}\n{}\n{:.0}, {:.0}",
                spot.name, spot.note, spot.view.center.x, spot.view.center.z
            ));
            if hit.clicked() {
                destination = Some(spot.view.clone());
            }
        }
        if let Some(view) = destination {
            self.prefs.navigation.record(self.current_view());
            self.prefs.navigation.record(view.clone());
            self.apply_view(view, ui.ctx());
        }
    }
}

pub(super) fn feature_catalog() -> &'static (Vec<(String, String)>, Vec<(String, String)>) {
    static CATALOG: std::sync::OnceLock<(Vec<(String, String)>, Vec<(String, String)>)> =
        std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let veins: serde_json::Value =
            serde_json::from_str(include_str!("../../assets/data/tfg/veins.json"))
                .expect("embedded veins");
        let beneath: serde_json::Value =
            serde_json::from_str(include_str!("../beneath/ore-features.json"))
                .expect("embedded Beneath veins");
        let label = |id: &str| id.rsplit(':').next().unwrap_or(id).replace(['_', '/'], " ");
        let mut ores: Vec<_> = veins["veins"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.keys())
            .map(|id| (id.clone(), label(id)))
            .collect();
        ores.extend(
            beneath["veins"]
                .as_object()
                .into_iter()
                .flat_map(|m| m.keys())
                .map(|id| (id.clone(), label(id))),
        );
        let structures = Dimension::ALL
            .into_iter()
            .flat_map(|dimension| crate::structure_support::catalog(dimension).iter().cloned())
            .collect::<std::collections::BTreeMap<_, _>>();
        (ores, structures.into_iter().collect())
    })
}

pub(super) fn feature_catalog_for(
    dimension: Dimension,
) -> &'static (Vec<(String, String)>, Vec<(String, String)>) {
    static CATALOGS: std::sync::OnceLock<
        std::collections::HashMap<Dimension, (Vec<(String, String)>, Vec<(String, String)>)>,
    > = std::sync::OnceLock::new();
    &CATALOGS.get_or_init(|| {
        Dimension::ALL
            .into_iter()
            .map(|dimension| {
                let ores = match dimension {
                    Dimension::Overworld => feature_catalog()
                        .0
                        .iter()
                        .filter(|(id, _)| {
                            !id.starts_with("beneath_")
                                && !id.starts_with("moon:")
                                && !id.starts_with("mars:")
                        })
                        .cloned()
                        .collect(),
                    Dimension::Beneath => crate::features::beneath_feature_catalog().0.clone(),
                    Dimension::Moon | Dimension::Mars => {
                        crate::planetary::features::feature_catalog(dimension)
                            .0
                            .clone()
                    }
                };
                let structures = crate::structure_support::catalog(dimension).clone();
                (dimension, (ores, structures))
            })
            .collect()
    })[&dimension]
}

fn decode_session(value: serde_json::Value) -> Result<Preferences, String> {
    if value["app"] == "TerraLens" && value["version"] == 1 {
        serde_json::from_value(value["preferences"].clone())
            .map_err(|e| format!("Invalid session: {e}"))
    } else if value.get("world_id").is_some() && value.get("blocks_per_point").is_some() {
        serde_json::from_value(value).map_err(|e| format!("Invalid session: {e}"))
    } else {
        Err("This is not a TerraLens session".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_format_rejects_other_json_and_preserves_views() {
        assert!(decode_session(serde_json::json!({"bounds":{"min_x":0}})).is_err());
        let prefs = Preferences {
            world_id: "Ganymede".into(),
            center_x: -2244.,
            center_z: -2316.,
            ..Default::default()
        };
        let decoded =
            decode_session(serde_json::json!({"app":"TerraLens","version":1,"preferences":prefs}))
                .unwrap();
        assert_eq!(decoded.world_id, "Ganymede");
        assert_eq!(decoded.center_x, -2244.);
    }

    #[test]
    fn feature_catalogs_do_not_leak_between_dimensions() {
        let overworld = feature_catalog_for(Dimension::Overworld);
        assert!(
            overworld
                .0
                .iter()
                .all(|(id, _)| !id.starts_with("beneath_"))
        );
        assert!(
            overworld
                .1
                .iter()
                .all(|(id, _)| crate::structure_support::enabled(Dimension::Overworld, id)),
            "Overworld controls must have Overworld biome-profile support"
        );

        let beneath = feature_catalog_for(Dimension::Beneath);
        assert!(beneath.0.iter().all(|(id, _)| id.starts_with("beneath_")));
        assert!(
            beneath
                .1
                .iter()
                .all(|(id, _)| crate::structure_support::enabled(Dimension::Beneath, id))
        );

        for (dimension, prefix) in [(Dimension::Moon, "moon:"), (Dimension::Mars, "mars:")] {
            let catalog = feature_catalog_for(dimension);
            assert!(catalog.0.iter().all(|(id, _)| id.starts_with(prefix)));
            assert!(
                catalog
                    .1
                    .iter()
                    .all(|(id, _)| crate::structure_support::enabled(dimension, id))
            );
            let other = if dimension == Dimension::Moon {
                "mars"
            } else {
                "moon"
            };
            assert!(catalog.1.iter().all(|(id, _)| !id.contains(other)));
        }

        let contains = |catalog: &[(String, String)], id: &str| {
            catalog.iter().any(|(candidate, _)| candidate == id)
        };
        for id in [
            "minecraft:village_plains",
            "minecraft:village_desert",
            "minecraft:village_savanna",
            "minecraft:village_snowy",
            "minecraft:village_taiga",
            "wan_ancient_beasts:abandoned_village",
            "tfg:illagers/arabic_village_living",
            "tfg:illagers/malay_village_living",
            "tfg:illagers/norse_village_living",
            "tfg:illagers/norse_village_ruined",
            "tfg:illagers/yurts_village_living",
            "tfg:temperate/plains_temperate_house_0",
            "tfc_ruined_world:towerhouse_1",
        ] {
            assert!(contains(&overworld.1, id), "Overworld catalog missing {id}");
        }
        for id in ["tfc_ruined_world:towerhouse_1", "tfg:moon/moonbase"] {
            assert!(!contains(&beneath.1, id), "Beneath catalog leaked {id}");
        }
        assert!(contains(&beneath.1, "minecraft:fortress"));
        assert!(contains(&beneath.1, "minecraft:bastion_remnant"));
        assert!(contains(&beneath.1, "minecraft:nether_fossil"));
        assert!(!contains(&beneath.1, "tfg:beneath/outpost"));

        let moon = feature_catalog_for(Dimension::Moon);
        let mars = feature_catalog_for(Dimension::Mars);
        assert!(contains(&moon.1, "ad_astra:moon_dungeon"));
        assert!(!contains(&mars.1, "ad_astra:moon_dungeon"));
        assert!(contains(&mars.1, "ad_astra:mars_temple"));
        assert!(!contains(&moon.1, "ad_astra:mars_temple"));
        assert!(contains(&moon.1, "tfg:moon/moonbase"));
        assert!(!contains(&overworld.1, "tfg:moon/moonbase"));
    }
}
