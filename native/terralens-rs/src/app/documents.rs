//! Native file workflows. The window handles paths; the engine reads and writes their contents.
use super::*;
use crate::{exports, model::Bounds};

impl TerraLensApp {
    pub(super) fn reload_world(&mut self) {
        if let Some(path) = self
            .selected
            .and_then(|index| self.worlds.get(index))
            .map(|world| world.path.clone())
        {
            self.open_source_file(path, Some(self.current_view()));
        }
    }

    pub(super) fn request_file(&mut self, kind: file_dialogs::Kind) {
        if kind == file_dialogs::Kind::ExportMarkers && !self.file_dialogs.busy() {
            self.pending_marker_export = None;
        }
        if let Err(error) = self
            .file_dialogs
            .request(kind, self.prefs.last_directory.clone())
        {
            self.document_error = Some(error);
        }
    }

    /// A newer world/seed selection wins over unfinished filesystem discovery.
    pub(super) fn cancel_pending_source(&mut self) {
        if self.pending_source.take().is_some() {
            self.source_serial = self.source_serial.wrapping_add(1);
            self.pending_source_view = None;
            self.pending_preferences = None;
            self.pending_session_path = None;
            self.discovering = false;
        }
    }

    pub(super) fn open_source_file(&mut self, path: PathBuf, view: Option<navigation::View>) {
        self.pending_preferences = None;
        self.pending_session_path = None;
        self.pending_session_write = None;
        self.source_serial = self.source_serial.wrapping_add(1);
        self.pending_source = Some((self.source_serial, path.clone()));
        self.pending_source_view = view;
        self.discovering = true;
        self.document_error = None;
        if let Some(atlas) = &mut self.atlas {
            atlas.cancel_tasks();
            atlas.open_source(path, self.source_serial);
        }
    }

    pub(super) fn poll_source(&mut self, ctx: &egui::Context) {
        let Some((serial, result)) = self.atlas.as_mut().and_then(Atlas::take_opened_source) else {
            if self.pending_source.is_some() {
                ctx.request_repaint_after(Duration::from_millis(60));
            }
            return;
        };
        if self
            .pending_source
            .as_ref()
            .is_none_or(|(current, _)| *current != serial)
        {
            return;
        }
        self.pending_source = None;
        self.discovering = false;
        match result {
            Ok(source) => {
                let mut restore = self.pending_source_view.take();
                if let Some(prefs) = self.pending_preferences.take() {
                    self.prefs = prefs;
                    self.legend.set_open(self.prefs.show_legend);
                }
                if let Some(path) = self.pending_session_path.take() {
                    self.session_path = path.to_string_lossy().into_owned();
                }
                self.instance = source.source.clone();
                self.prefs.source_path = Some(source.source.clone());
                remember_source(&mut self.prefs.recent_sources, source.source);
                self.worlds = source.worlds;
                for world in &self.worlds {
                    let old = world.path.file_name().unwrap_or_default().to_string_lossy();
                    if self
                        .worlds
                        .iter()
                        .filter(|candidate| candidate.path.file_name() == world.path.file_name())
                        .count()
                        == 1
                    {
                        self.prefs.navigation.remap_world(&old, &world.id);
                        if let Some(view) = &mut restore
                            && view.world_id == old
                        {
                            view.world_id = world.id.clone();
                        }
                    }
                }
                let selected = restore
                    .as_ref()
                    .and_then(|view| {
                        self.worlds
                            .iter()
                            .position(|world| world.id == view.world_id)
                    })
                    .or_else(|| {
                        self.worlds.iter().position(|world| {
                            Self::world_supports_dimension(world, self.prefs.dimension)
                        })
                    })
                    .unwrap_or(0);
                if !Self::world_supports_dimension(&self.worlds[selected], self.prefs.dimension) {
                    self.prefs.dimension = Dimension::ALL
                        .into_iter()
                        .find(|dimension| {
                            Self::world_supports_dimension(&self.worlds[selected], *dimension)
                        })
                        .unwrap_or(Dimension::Overworld);
                    self.prefs.layer = 0;
                }
                if let Some(view) = &mut restore
                    && !Self::world_supports_dimension(&self.worlds[selected], view.dimension)
                {
                    view.dimension = self.prefs.dimension;
                }
                self.activate_world(selected, ctx);
                if let Some(view) = restore {
                    self.apply_view(view, ctx);
                }
            }
            Err(error) => {
                self.pending_source_view = None;
                self.pending_preferences = None;
                self.pending_session_path = None;
                self.document_error = Some(error);
            }
        }
    }

    pub(super) fn poll_file_dialogs(&mut self, ctx: &egui::Context) {
        let was_busy = self.file_dialogs.busy();
        if let Some(selection) = self.file_dialogs.poll() {
            use file_dialogs::Kind;
            self.prefs.last_directory = match selection.kind {
                Kind::OpenFolder => Some(selection.path.clone()),
                _ => selection.path.parent().map(std::path::Path::to_path_buf),
            };
            match selection.kind {
                Kind::OpenWorld | Kind::OpenFolder => self.open_source_file(selection.path, None),
                Kind::OpenSession => {
                    if let Some(atlas) = &mut self.atlas {
                        atlas.export(exports::Request::ReadSession {
                            path: selection.path,
                        });
                    }
                }
                Kind::SaveSession => {
                    let value = self.session_value();
                    self.pending_session_write = Some(selection.path.clone());
                    if let Some(atlas) = &mut self.atlas {
                        atlas.export(exports::Request::Data {
                            value,
                            path: selection.path,
                        });
                    }
                }
                Kind::ImportSeeds => {
                    if let Some(atlas) = &mut self.atlas {
                        atlas.export(exports::Request::ReadSeeds {
                            path: selection.path,
                        });
                    }
                }
                Kind::Export | Kind::ExportMarkers => {
                    if selection.kind == Kind::ExportMarkers
                        && let Some(markers) = self.pending_marker_export.take()
                    {
                        if let Some(atlas) = &mut self.atlas {
                            atlas.export(exports::Request::Markers {
                                markers,
                                path: selection.path,
                            });
                        }
                        return;
                    }
                    let picker = if selection.kind == Kind::Export {
                        export_panel::Picker::Map
                    } else {
                        export_panel::Picker::Markers
                    };
                    self.export_to(picker, selection.path);
                }
                Kind::ExportData => {
                    let data = self
                        .atlas
                        .as_ref()
                        .and_then(Atlas::analysis_report)
                        .and_then(|report| serde_json::to_value(report).ok());
                    if let (Some(value), Some(atlas)) = (data, &mut self.atlas) {
                        atlas.export(exports::Request::Data {
                            value,
                            path: selection.path,
                        });
                    }
                }
            }
        }
        if was_busy && !self.file_dialogs.busy() {
            self.pending_marker_export = None;
        }
        if let Some(error) = self.file_dialogs.take_error() {
            self.document_error = Some(error);
        }
        if self.file_dialogs.busy() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }

    pub(super) fn visible_export_markers(&self, bounds: Bounds) -> Vec<Marker> {
        let Some(atlas) = &self.atlas else {
            return Vec::new();
        };
        atlas
            .visible_markers(bounds, self.prefs.show_ores, self.prefs.show_structures)
            .into_iter()
            .filter(|marker| {
                self.prefs.display.marker_visible_in_view(
                    marker,
                    self.prefs.blocks_per_point,
                    self.prefs.dimension,
                    self.prefs.slice_y,
                )
            })
            .cloned()
            .collect()
    }

    fn export_to(&mut self, picker: export_panel::Picker, path: PathBuf) {
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
        let request = self.exporter.request_for_path(
            picker,
            path,
            bounds,
            self.prefs.dimension,
            layer,
            &markers,
            snapshot.as_ref(),
            self.prefs.display.palette,
        );
        if let Some(atlas) = &mut self.atlas {
            atlas.export(request);
        }
    }

    pub(super) fn close_source(&mut self, ctx: &egui::Context) {
        self.file_dialogs.cancel();
        self.pending_session_write = None;
        self.pending_marker_export = None;
        self.source_serial = self.source_serial.wrapping_add(1);
        self.pending_source = None;
        self.pending_source_view = None;
        self.pending_preferences = None;
        self.pending_session_path = None;
        self.discovering = false;
        self.prefs.source_path = None;
        self.worlds.clear();
        self.activate_seed(0, ctx);
    }
}

fn remember_source(recent: &mut Vec<PathBuf>, source: PathBuf) {
    recent.retain(|path| path != &source);
    recent.insert(0, source);
    recent.truncate(12);
}

pub(super) fn source_name(path: &std::path::Path) -> String {
    let path = if path.file_name().is_some_and(|name| name == "level.dat") {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recent_sources_are_deduplicated_and_bounded() {
        let mut recent = Vec::new();
        for index in 0..20 {
            remember_source(&mut recent, PathBuf::from(format!("/world/{index}")));
        }
        assert_eq!(recent.len(), 12);
        remember_source(&mut recent, PathBuf::from("/world/10"));
        assert_eq!(recent.len(), 12);
        assert_eq!(recent[0], PathBuf::from("/world/10"));
        assert_eq!(
            source_name(std::path::Path::new("/backup/My world/level.dat")),
            "My world"
        );
    }
}
