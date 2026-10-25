//! Small code-drawn controls that do not depend on font glyph coverage.
use eframe::egui::{self, Pos2, Stroke, Vec2};
pub(super) enum Icon {
    Back,
    Forward,
    Locate,
    Season,
}
pub(super) fn button(ui: &mut egui::Ui, icon: Icon) -> egui::Response {
    let response = ui.add(egui::Button::new("").min_size(Vec2::splat(24.)));
    let c = response.rect.center();
    let stroke = Stroke::new(1.5_f32, ui.style().interact(&response).fg_stroke.color);
    let painter = ui.painter();
    match icon {
        Icon::Back | Icon::Forward => {
            let d = if matches!(icon, Icon::Back) { -1. } else { 1. };
            painter.line_segment(
                [
                    Pos2::new(c.x - d * 2., c.y - 4.),
                    Pos2::new(c.x + d * 2., c.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + d * 2., c.y),
                    Pos2::new(c.x - d * 2., c.y + 4.),
                ],
                stroke,
            );
        }
        Icon::Locate => {
            painter.circle_stroke(c, 4., stroke);
            for offset in [
                Vec2::new(-7., 0.),
                Vec2::new(7., 0.),
                Vec2::new(0., -7.),
                Vec2::new(0., 7.),
            ] {
                painter.line_segment([c + offset, c + offset * 0.65], stroke);
            }
        }
        Icon::Season => {
            painter.circle_stroke(c, 3.2, stroke);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.;
                let direction = Vec2::new(angle.cos(), angle.sin());
                painter.line_segment([c + direction * 5.2, c + direction * 7.4], stroke);
            }
        }
    }
    response
}
