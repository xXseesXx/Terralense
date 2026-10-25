//! Keep setting menus open for repeated edits; explicit actions can still close them.
use eframe::egui::{self, Ui, containers::menu::MenuConfig};

fn config() -> MenuConfig {
    MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}

pub(super) fn bar() -> egui::MenuBar {
    egui::MenuBar::new().config(config())
}

pub(super) fn settings<R>(
    ui: &mut Ui,
    label: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let (response, inner) = egui::containers::menu::MenuButton::new(label)
        .config(config())
        .ui(ui, content);
    egui::InnerResponse::new(inner.map(|inner| inner.inner), response)
}
