//! Cubiomes-style desktop shell: menus, a left tabbed control dock, map and feature rail.
use super::*;

#[derive(Default)]
pub(super) struct FeatureEntries {
    key: Option<(String, Dimension, usize)>,
    ores: Vec<feature_bar::Entry>,
    structures: Vec<feature_bar::Entry>,
}

impl TerraLensApp {
    pub(super) fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("desktop-menu").frame(panel_frame()).show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if menu(ui,"New seed", "Ctrl+N") { self.activate_seed(0,ctx); self.seed_input="0".into(); ui.close(); }
                    ui.separator();
                    if menu(ui,"Load session…", "Ctrl+O") { self.request_file(file_dialogs::Kind::OpenSession); ui.close(); }
                    if menu(ui,"Save session…", "Ctrl+S") { self.save_session_file(); ui.close(); }
                    if menu(ui,"Save session as…", "Ctrl+Shift+S") { self.request_file(file_dialogs::Kind::SaveSession); ui.close(); }
                    ui.separator();
                    if menu(ui,"Open world…", "Ctrl+Shift+O") { self.request_file(file_dialogs::Kind::OpenWorld); ui.close(); }
                    if ui.button("Open folder…").on_hover_text("A world, Minecraft saves folder, or Prism instance at any location").clicked() { self.request_file(file_dialogs::Kind::OpenFolder); ui.close(); }
                    ui.menu_button("Recent worlds", |ui| {
                        for path in self.prefs.recent_sources.clone() {
                            if ui.button(documents::source_name(&path)).on_hover_text(path.display().to_string()).clicked() { self.open_source_file(path,None); ui.close(); }
                        }
                        if self.prefs.recent_sources.is_empty() { ui.add_enabled(false,egui::Button::new("No recent worlds")); }
                    });
                    if menu(ui,"Close world", "Ctrl+W") { self.close_source(ctx); ui.close(); }
                    ui.separator();
                    if ui.button("Import seed list…").clicked() { self.request_file(file_dialogs::Kind::ImportSeeds); ui.close(); }
                    if menu(ui,"Export map image…", "Ctrl+E") { self.request_file(file_dialogs::Kind::Export); ui.close(); }
                    if ui.button("Export options…").clicked() { self.export_open=true; ui.close(); }
                    if ui.button("Export visible markers…").clicked() { self.request_file(file_dialogs::Kind::ExportMarkers); ui.close(); }
                    ui.separator();
                    if ui.button("Quit").clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                });
                ui.menu_button("Search", |ui| {
                    if menu(ui,"Find feature…", "Ctrl+F") { self.open_find(); ui.close(); }
                    if ui.button("Find locations").clicked() { self.prefs.dock_tab=0; self.prefs.search_page=1; self.prefs.location_mode=1; self.prefs.control_dock=true; ui.close(); }
                    if ui.button("Seed list").clicked() { self.set_workspace(Some(Workspace::Seed)); ui.close(); }
                    if ui.button("Search visible area").clicked() { self.search_visible(); ui.close(); }
                });
                ui.menu_button("Map", |ui| {
                    ui.menu_button("Previous seeds", |ui| {
                        let history=self.prefs.navigation.history().to_vec();
                        for view in history.into_iter().rev().take(24) {
                            if let Some(seed)=view.seed {if ui.button(seed.to_string()).clicked(){self.apply_view(view,ctx);ui.close();}}
                        }
                    });
                    if menu(ui,"Go to coordinates…", "Ctrl+G") { self.prefs.navigation.open_jump(&self.current_view()); ui.close(); }
                    if ui.button("Go to saved position").clicked() {
                        let point = self.selected.and_then(|index| self.worlds.get(index)).and_then(|world| world.dimensions.iter().find(|dimension|dimension.id==self.prefs.dimension.id()).map(|dimension|dimension.center).or(Some(world.center)));
                        if let Some(point)=point {self.set_center(point);} ui.close();
                    }
                    if ui.button("Copy seed").on_hover_text(format!("{} visited views", self.prefs.navigation.history().len())).clicked() { if let Some(seed)=self.current_view().seed {ctx.copy_text(seed.to_string());} ui.close(); }
                    ui.separator();
                    ui.checkbox(&mut self.prefs.tools.coordinate_grid,"Coordinate grid");
                    ui.checkbox(&mut self.prefs.tools.chunk_grid,"Chunk grid");
                    ui.checkbox(&mut self.prefs.tools.region_grid,"Region grid");
                    ui.checkbox(&mut self.prefs.tools.scale_bar,"Scale bar");
                    ui.checkbox(&mut self.prefs.connected_highlight,"Connected-region highlight");
                    let mut legend=self.legend.is_open();
                    if ui.checkbox(&mut legend,"Layer legend").changed() {self.legend.set_open(legend);self.prefs.show_legend=legend;}
                    ui.checkbox(&mut self.prefs.control_dock,"Control pane");
                    ui.separator();
                    if ui.add_enabled(self.tools.has_overlay(),egui::Button::new("Clear measurement / selection")).clicked() {self.tools.clear_selection();ui.close();}
                });
                ui.menu_button("Layer", |ui| {
                    for (index,layer) in Layer::ALL.iter().enumerate().filter(|(_,layer)|self.prefs.dimension==Dimension::Overworld || **layer==Layer::Biomes) {
                        if ui.selectable_value(&mut self.prefs.layer,index,layer.label()).clicked() {ui.close();}
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if ui.button("Biome analysis").clicked() {self.prefs.dock_tab=1;self.prefs.control_dock=true;ui.close();}
                    if ui.button("Structure analysis").clicked() {self.prefs.dock_tab=2;self.prefs.control_dock=true;ui.close();}
                    if ui.button("Display and tooltip options…").clicked() {self.show_preferences=true;ui.close();}
                    if ui.button("Paste session…").clicked() {self.session_open=true;ui.close();}
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Controls").clicked() {self.help_open=true;ui.close();}
                    ui.menu_button("Licenses", |ui| {
                        ui.label("© 2026 TerraLens contributors");
                        ui.label("Cubiomes Viewer icons · Cubitect and contributors");
                        ui.label("GPL-3.0 combined distribution · no warranty");
                        ui.hyperlink_to("Source, artwork credits and licenses", "https://github.com/xXseesXx/Terralense/blob/main/NOTICE.md");
                        ui.hyperlink_to("GNU General Public License v3", "https://github.com/xXseesXx/Terralense/blob/main/licenses/GPL-3.0.txt");
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if self.file_dialogs.busy() || self.discovering {ui.spinner();}
                    if !self.prefs.control_dock && ui.small_button("Controls").clicked() {self.prefs.control_dock=true;}
                });
            });
        });
    }

    pub(super) fn side_panel(&mut self, ctx: &egui::Context) {
        if !self.prefs.control_dock {
            return;
        }
        egui::SidePanel::left("cubiomes-controls")
            .resizable(true)
            .default_width(340.)
            .width_range(280. ..=520.)
            .frame(panel_frame())
            .show(ctx, |ui| {
                self.world_header(ui, ctx);
                self.navigation_controls(ui);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.prefs.dock_tab, 0, "Search");
                    ui.selectable_value(&mut self.prefs.dock_tab, 1, "Biomes");
                    ui.selectable_value(&mut self.prefs.dock_tab, 2, "Structures");
                });
                ui.separator();
                let pinned = self.selected_marker.is_some();
                egui::ScrollArea::vertical()
                    .id_salt("control-dock-scroll")
                    .max_height((ui.available_height() - if pinned { 155. } else { 0. }).max(120.))
                    .show(ui, |ui| match self.prefs.dock_tab {
                        1 => self.biome_analysis(ui),
                        2 => self.structure_analysis(ui),
                        _ => {
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut self.prefs.search_page, 0, "Seeds");
                                ui.selectable_value(&mut self.prefs.search_page, 1, "Locations");
                            });
                            ui.separator();
                            if self.prefs.search_page == 0 {
                                self.seed_panel(ui, ctx)
                            } else {
                                ui.horizontal(|ui| {
                                    ui.selectable_value(
                                        &mut self.prefs.location_mode,
                                        0,
                                        "Features",
                                    );
                                    if self.prefs.dimension == Dimension::Overworld {
                                        ui.selectable_value(
                                            &mut self.prefs.location_mode,
                                            1,
                                            "Base sites",
                                        );
                                    }
                                });
                                if self.prefs.location_mode == 1
                                    && self.prefs.dimension == Dimension::Overworld
                                {
                                    self.site_analysis(ui);
                                } else {
                                    self.search_panel(ui);
                                }
                            }
                        }
                    });
                if pinned {
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("pinned-inspector")
                        .max_height(165.)
                        .show(ui, |ui| self.selected_marker_panel(ui));
                }
            });
    }

    fn world_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let header_width = ui.available_width();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("generation-profile")
                .width(90.)
                .selected_text("TFG 0.9.25")
                .show_ui(ui, |ui| {
                    ui.label("TFC 3.2.25 · MC 1.20.1");
                });
            let width = (header_width - 164.).max(76.);
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.seed_input)
                    .id(Id::new("world-seed-input"))
                    .font(egui::TextStyle::Monospace)
                    .desired_width(width)
                    .hint_text("Seed"),
            );
            if (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                || ui.small_button("Go").clicked()
            {
                match self.seed_input.trim().parse::<i64>() {
                    Ok(seed) => {
                        self.seed_invalid = false;
                        self.activate_shortlisted_seed(seed, ctx);
                    }
                    Err(_) => self.seed_invalid = true,
                }
            }
        });
        if self.seed_invalid {
            ui.colored_label(Color32::LIGHT_RED, "Enter a signed 64-bit seed.");
        }
        ui.horizontal(|ui| {
            if ui.small_button("Random").clicked() {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let mut random = crate::random::Xoroshiro::from_state(
                    stamp as u64,
                    (stamp >> 64) as u64 ^ 0x9e3779b97f4a7c15,
                );
                let seed = random.next_long();
                self.seed_input = seed.to_string();
                self.activate_shortlisted_seed(seed, ctx);
            }
            let before = self.prefs.dimension;
            egui::ComboBox::from_id_salt("active-dimension")
                .width(93.)
                .selected_text(before.label())
                .show_ui(ui, |ui| {
                    for dimension in Dimension::ALL {
                        let supported = self
                            .selected
                            .and_then(|index| self.worlds.get(index))
                            .is_none_or(|world| Self::world_supports_dimension(world, dimension));
                        ui.add_enabled_ui(supported, |ui| {
                            ui.selectable_value(
                                &mut self.prefs.dimension,
                                dimension,
                                dimension.label(),
                            );
                        });
                    }
                });
            if before != self.prefs.dimension {
                self.switch_dimension(before);
            }
            if self.prefs.dimension == Dimension::Beneath {
                let (low, top) = beneath_band_range(self.prefs.slice_y);
                let mut requested = top;
                egui::ComboBox::from_id_salt("active-y-band")
                    .width(83.)
                    .selected_text(format!("Y {low}–{top}"))
                    .show_ui(ui, |ui| {
                        for band in 0..16 {
                            ui.selectable_value(
                                &mut requested,
                                band * 16 + 15,
                                format!("{}–{}", band * 16, band * 16 + 15),
                            );
                        }
                    });
                if requested != top {
                    self.set_beneath_band(requested);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .small_button("Open…")
                .on_hover_text("Open a world or Prism folder at any location")
                .clicked()
            {
                self.request_file(file_dialogs::Kind::OpenFolder);
            }
            let label = self
                .selected
                .and_then(|index| self.worlds.get(index))
                .map(|world| world.name.clone())
                .unwrap_or_else(|| "Seed preview".into());
            egui::ComboBox::from_id_salt("opened-world-selector")
                .width((ui.available_width() - 24.).max(90.))
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (index, world) in self.worlds.clone().iter().enumerate() {
                        if ui
                            .selectable_label(self.selected == Some(index), &world.name)
                            .on_hover_text(world.path.display().to_string())
                            .clicked()
                        {
                            self.activate_world(index, ctx);
                        }
                    }
                    if self.worlds.is_empty() && ui.button("Open world…").clicked() {
                        self.request_file(file_dialogs::Kind::OpenWorld);
                    }
                });
        });
    }

    pub(super) fn feature_rail(&mut self, ctx: &egui::Context) {
        let saved = self
            .atlas
            .as_ref()
            .map_or(0, |atlas| atlas.saved_structures().len());
        let key = (self.prefs.world_id.clone(), self.prefs.dimension, saved);
        if self.feature_entries.key.as_ref() != Some(&key) {
            let (all_ores, all_structures) = workspace::feature_catalog();
            let beneath = crate::features::beneath_feature_catalog();
            let ore_items = if self.prefs.dimension == Dimension::Beneath {
                beneath.0.iter().collect::<Vec<_>>()
            } else {
                all_ores
                    .iter()
                    .filter(|(id, _)| !id.starts_with("beneath_"))
                    .collect()
            };
            self.feature_entries.ores = ore_items
                .into_iter()
                .map(|(id, label)| feature_bar::Entry {
                    id: id.clone(),
                    label: label.clone(),
                })
                .collect();
            let structure_items = if self.prefs.dimension == Dimension::Beneath {
                beneath.1.iter().collect::<Vec<_>>()
            } else {
                all_structures
                    .iter()
                    .filter(|(id, _)| {
                        !id.contains("beneath")
                            && !id.starts_with("ad_astra:")
                            && !id.starts_with("minecraft:nether")
                            && !id.contains("bastion")
                            && !id.contains("fortress")
                    })
                    .collect()
            };
            let mut entries = structure_items
                .into_iter()
                .map(|(id, label)| (id.clone(), label.clone()))
                .collect::<std::collections::BTreeMap<_, _>>();
            if let Some(atlas) = &self.atlas {
                for marker in atlas.saved_structures() {
                    entries.insert(
                        marker.id.split('@').next().unwrap_or(&marker.id).to_owned(),
                        marker.label.clone(),
                    );
                }
            }
            self.feature_entries.structures = entries
                .into_iter()
                .map(|(id, label)| feature_bar::Entry {
                    id,
                    label: tooltip::display_marker_name(&label),
                })
                .collect();
            self.feature_entries.key = Some(key);
        }
        self.feature_bar.show(
            ctx,
            self.prefs.dimension,
            &mut self.prefs.display,
            &mut self.prefs.show_ores,
            &mut self.prefs.show_structures,
            &mut self.ore_icons,
            &mut self.structure_icons,
            &self.feature_entries.ores,
            &self.feature_entries.structures,
        );
    }

    fn biome_analysis(&mut self, ui: &mut egui::Ui) {
        let mut bounds = self.tools.selection().unwrap_or(self.last_bounds);
        ui.horizontal(|ui| {
            if ui.button("From visible").clicked() {
                bounds = self.last_bounds;
                self.tools.set_selection(bounds);
            }
            let available = self
                .atlas
                .as_ref()
                .is_some_and(|atlas| atlas.analysis_report().is_some());
            if ui
                .add_enabled(available, egui::Button::new("Export…"))
                .clicked()
            {
                self.request_file(file_dialogs::Kind::ExportData);
            }
        });
        let mut edited = false;
        egui::Grid::new("analysis-area-coordinates")
            .num_columns(4)
            .show(ui, |ui| {
                for (x_label, x, z_label, z) in [
                    ("X1", &mut bounds.min_x, "Z1", &mut bounds.min_z),
                    ("X2", &mut bounds.max_x, "Z2", &mut bounds.max_z),
                ] {
                    ui.label(x_label);
                    edited |= ui
                        .add(
                            egui::DragValue::new(x)
                                .speed(16.)
                                .range(-30_000_000. ..=30_000_000.)
                                .max_decimals(0),
                        )
                        .changed();
                    ui.label(z_label);
                    edited |= ui
                        .add(
                            egui::DragValue::new(z)
                                .speed(16.)
                                .range(-30_000_000. ..=30_000_000.)
                                .max_decimals(0),
                        )
                        .changed();
                    ui.end_row();
                }
            });
        if edited {
            self.tools.set_selection(bounds);
            bounds = self.tools.selection().unwrap_or(bounds);
        }
        if self.prefs.dimension == Dimension::Beneath {
            ui.label("Regional analysis is available for Overworld climate and terrain.");
            return;
        }
        if let Some(analysis_panel::Action::Analyze(query)) =
            analysis_panel::show_controls(ui, &mut self.location_query, bounds, 0)
        {
            if let Some(atlas) = &mut self.atlas {
                atlas.analyze(query);
            }
        }
        if let Some(atlas) = &mut self.atlas {
            analysis_panel::show_report(ui, atlas.analysis_report());
            if atlas.tasks_busy() && ui.button("Stop").clicked() {
                atlas.cancel_tasks();
            }
        }
    }

    fn site_analysis(&mut self, ui: &mut egui::Ui) {
        let bounds = self.tools.selection().unwrap_or(self.last_bounds);
        egui::CollapsingHeader::new("Conditions")
            .default_open(true)
            .show(ui, |ui| {
                if let Some(analysis_panel::Action::Locate(mut query)) =
                    analysis_panel::show_controls(ui, &mut self.location_query, bounds, 1)
                {
                    query.origin = self.current_center();
                    if let Some(atlas) = &mut self.atlas {
                        atlas.locate(query);
                    }
                }
            });
        let destination = self
            .atlas
            .as_ref()
            .and_then(|atlas| analysis_panel::show_results(ui, atlas.locations()));
        if let Some(point) = destination {
            self.selected_marker = self
                .atlas
                .as_ref()
                .and_then(Atlas::locations)
                .and_then(|locations| {
                    locations
                        .candidates
                        .iter()
                        .find(|candidate| candidate.point == point)
                })
                .map(|candidate| candidate.marker());
            self.set_center(point);
        }
        if let Some(atlas) = &mut self.atlas {
            if atlas.tasks_busy() && ui.button("Stop").clicked() {
                atlas.cancel_tasks();
            }
        }
    }

    fn structure_analysis(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.structure_whole_save, false, "Visible area");
            ui.selectable_value(&mut self.structure_whole_save, true, "Whole save");
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.structure_query)
                .hint_text("Filter structures")
                .desired_width(ui.available_width()),
        );
        let export = ui
            .add_enabled(!self.file_dialogs.busy(), egui::Button::new("Export…"))
            .clicked();
        let Some(atlas) = &self.atlas else { return };
        let mut markers = if self.structure_whole_save {
            atlas.saved_structures().iter().collect::<Vec<_>>()
        } else {
            atlas.visible_markers(self.last_bounds, false, true)
        };
        let query = self.structure_query.to_ascii_lowercase();
        markers.retain(|marker| {
            marker.kind == MarkerKind::Structure
                && (query.is_empty()
                    || marker.label.to_ascii_lowercase().contains(&query)
                    || marker.id.to_ascii_lowercase().contains(&query))
        });
        let origin = self.current_center();
        markers.sort_by(|a, b| {
            a.point()
                .distance(origin)
                .total_cmp(&b.point().distance(origin))
        });
        ui.label(
            RichText::new(format!("{} structures", markers.len()))
                .size(10.)
                .color(MUTED),
        );
        let mut selected = None;
        egui::ScrollArea::vertical()
            .id_salt("structure-analysis-results")
            .max_height(400.)
            .show_rows(ui, 36., markers.len(), |ui, range| {
                for index in range {
                    let marker = markers[index];
                    ui.horizontal(|ui| {
                        let texture = self.structure_icons.get(ui.ctx(), &marker.id);
                        ui.image((texture.id(), Vec2::splat(24.)));
                        let text = format!(
                            "{} · {:.1} km",
                            tooltip::display_marker_name(&marker.label),
                            marker.point().distance(origin) / 1000.
                        );
                        if ui
                            .add(egui::Button::new(text).truncate())
                            .on_hover_text(&marker.detail)
                            .clicked()
                        {
                            selected = Some(marker.clone());
                        }
                    });
                }
            });
        let export_markers =
            export.then(|| markers.iter().map(|marker| (*marker).clone()).collect());
        if let Some(markers) = export_markers {
            self.request_file(file_dialogs::Kind::ExportMarkers);
            if self.file_dialogs.busy() {
                self.pending_marker_export = Some(markers);
            }
        }
        if let Some(marker) = selected {
            self.set_center(marker_destination(&marker));
            self.selected_marker = Some(marker);
        }
    }

    pub(super) fn shell_windows(&mut self, ctx: &egui::Context) {
        if self.show_preferences {
            let mut open = true;
            egui::Window::new("##display-options")
                .title_bar(false)
                .default_width(350.)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut self.prefs.allow_detail_rendering,
                            "Block detail rendering",
                        );
                        if ui.small_button("Close").clicked() {
                            self.show_preferences = false;
                        }
                    });
                    ui.checkbox(&mut self.prefs.surface_detail, "Detail at close zoom");
                    self.prefs.tooltip.show_preferences(ui);
                    ui.separator();
                    self.display_controls(ui);
                });
            self.show_preferences &= open;
        }
        if self.export_open {
            let mut open = true;
            egui::Window::new("##export-options")
                .title_bar(false)
                .default_width(370.)
                .open(&mut open)
                .show(ctx, |ui| {
                    self.tools_panel(ui);
                    if ui.button("Close").clicked() {
                        self.export_open = false;
                    }
                });
            self.export_open &= open;
        }
        if self.help_open {
            let mut open = true;
            egui::Window::new("##controls")
                .title_bar(false)
                .default_width(380.)
                .open(&mut open)
                .show(ctx, |ui| {
                    for (keys, action) in [
                        ("Drag / scroll", "Pan / zoom"),
                        ("Ctrl+F", "Find feature"),
                        ("Ctrl+G", "Go to coordinates"),
                        ("Ctrl+O / Ctrl+S", "Load / save session"),
                        ("Ctrl+Shift+O", "Open world"),
                        ("Shift-drag / Alt-drag", "Measure / select area"),
                        ("Page Up / Down", "Beneath layer"),
                        ("Shift-click icon", "Show only this feature type"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.monospace(keys);
                            ui.label(action);
                        });
                    }
                    if ui.button("Close").clicked() {
                        self.help_open = false;
                    }
                });
            self.help_open &= open;
        }
        if let Some(error) = self.document_error.clone() {
            egui::Window::new("##file-error")
                .title_bar(false)
                .default_width(380.)
                .show(ctx, |ui| {
                    ui.label(RichText::new(error).color(Color32::LIGHT_RED));
                    if ui.button("Close").clicked() {
                        self.document_error = None;
                    }
                });
        }
    }
}

fn menu(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    ui.add(egui::Button::new(label).shortcut_text(shortcut))
        .clicked()
}
