//! Native file pickers dispatched away from the egui frame thread.
use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use std::{path::PathBuf, thread};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    OpenWorld,
    OpenFolder,
    OpenSession,
    ImportSeeds,
    SaveSession,
    Export,
    ExportMarkers,
    ExportData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Selection {
    pub kind: Kind,
    pub path: PathBuf,
}

#[derive(Debug)]
enum Event {
    Selected(u64, Selection),
    Cancelled(u64),
    Failed(u64, String),
}

pub(super) struct Dialogs {
    active: Option<Kind>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    error: Option<String>,
    generation: u64,
}

impl Default for Dialogs {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        Self {
            active: None,
            tx,
            rx,
            error: None,
            generation: 0,
        }
    }
}

impl Dialogs {
    /// Start one OS dialog. Completion is retrieved later through [`Self::poll`].
    pub fn request(&mut self, kind: Kind, start_dir: Option<PathBuf>) -> Result<(), String> {
        if self.active.is_some() {
            return Err("A file dialog is already open".into());
        }
        self.error = None;
        self.active = Some(kind);
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let tx = self.tx.clone();
        let thread_result = thread::Builder::new()
            .name("terralens-file-dialog".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pollster::block_on(async move {
                        let mut dialog = rfd::AsyncFileDialog::new();
                        if let Some(dir) = start_dir.filter(|path| path.is_dir()) {
                            dialog = dialog.set_directory(dir);
                        }
                        let dialog = match kind {
                            Kind::OpenWorld => dialog
                                .set_title("Open Minecraft world (select level.dat)")
                                .add_filter("Minecraft world data", &["dat"]),
                            Kind::OpenSession => dialog
                                .set_title("Open TerraLens session")
                                .add_filter("TerraLens session", &["json"]),
                            Kind::ImportSeeds => dialog
                                .set_title("Import a seed list")
                                .add_filter("Seed lists", &["txt", "csv", "json"]),
                            Kind::OpenFolder => {
                                dialog.set_title("Open a world or Prism instance folder")
                            }
                            Kind::SaveSession => dialog
                                .set_title("Save TerraLens session")
                                .add_filter("TerraLens session", &["json"])
                                .set_file_name("terralens-session.json"),
                            Kind::Export => dialog
                                .set_title("Export map")
                                .add_filter("PNG image", &["png"])
                                .set_file_name("terralens-map.png"),
                            Kind::ExportMarkers => dialog
                                .set_title("Export map markers")
                                .add_filter("CSV file", &["csv"])
                                .set_file_name("markers.csv"),
                            Kind::ExportData => dialog
                                .set_title("Export analysis data")
                                .add_filter("JSON file", &["json"])
                                .set_file_name("analysis.json"),
                        };
                        match kind {
                            Kind::OpenFolder => dialog
                                .pick_folder()
                                .await
                                .map(|file| file.path().to_owned()),
                            Kind::SaveSession => {
                                dialog.save_file().await.map(|file| file.path().to_owned())
                            }
                            Kind::ExportMarkers | Kind::ExportData => {
                                dialog.save_file().await.map(|file| file.path().to_owned())
                            }
                            Kind::Export => {
                                dialog.save_file().await.map(|file| file.path().to_owned())
                            }
                            Kind::OpenWorld | Kind::OpenSession | Kind::ImportSeeds => {
                                dialog.pick_file().await.map(|file| file.path().to_owned())
                            }
                        }
                    })
                }));
                let event = match result {
                    Ok(Some(path)) => Event::Selected(generation, Selection { kind, path }),
                    Ok(None) => Event::Cancelled(generation),
                    Err(_) => {
                        Event::Failed(generation, "The operating system file picker failed".into())
                    }
                };
                let _ = tx.send(event);
            });
        if let Err(error) = thread_result {
            self.active = None;
            self.error = Some(format!("Could not start file picker: {error}"));
            return Err(self.error.clone().unwrap_or_default());
        }
        Ok(())
    }

    /// Return a completed selection without waiting for the OS dialog.
    pub fn poll(&mut self) -> Option<Selection> {
        loop {
            match self.rx.try_recv() {
                Ok(Event::Selected(generation, selection)) => {
                    if generation == self.generation && self.active == Some(selection.kind) {
                        self.active = None;
                        return Some(selection);
                    }
                }
                Ok(Event::Cancelled(generation)) => {
                    if generation == self.generation {
                        self.active = None;
                    }
                }
                Ok(Event::Failed(generation, error)) => {
                    if generation == self.generation {
                        self.active = None;
                        self.error = Some(error);
                    }
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    self.active = None;
                    self.error = Some("The file picker stopped unexpectedly".into());
                    return None;
                }
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.active.is_some()
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// Ignore any eventual result from a dialog whose owning UI was closed.
    pub fn cancel(&mut self) {
        self.active = None;
        self.generation = self.generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_idle_and_has_no_error() {
        let mut dialogs = Dialogs::default();
        assert!(!dialogs.busy());
        assert!(dialogs.poll().is_none());
        assert!(dialogs.take_error().is_none());
    }

    #[test]
    fn cancelled_dialog_result_cannot_escape_into_a_later_ui() {
        let mut dialogs = Dialogs::default();
        dialogs.active = Some(Kind::OpenSession);
        dialogs.generation = 8;
        dialogs.cancel();
        dialogs.active = Some(Kind::OpenSession);
        dialogs.generation = 10;
        dialogs
            .tx
            .send(Event::Selected(
                8,
                Selection {
                    kind: Kind::OpenSession,
                    path: PathBuf::from("stale.json"),
                },
            ))
            .unwrap();
        assert!(dialogs.poll().is_none());
        assert!(dialogs.busy());
    }
}
