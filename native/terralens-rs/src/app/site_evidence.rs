//! Cached evidence from the completed site search; no generation in the window.
use super::*;
use crate::analysis::LocationResults;

pub(super) fn paint(
    ui: &egui::Ui,
    rect: Rect,
    center: Point,
    bpp: f64,
    results: Option<&LocationResults>,
    selected: Option<&Marker>,
    show_search: bool,
) {
    let Some(results) = results else { return };
    let painter = ui.painter().with_clip_rect(rect);
    if show_search {
        if let Some(query) = &results.query {
            let origin = world_to_screen(rect, center, bpp, query.origin);
            let radius = (results.actual_radius / bpp) as f32;
            // Avoid enormous tessellated circles when inspecting a single block.
            if radius <= rect.size().length() * 8. {
                painter.circle_stroke(
                    origin,
                    radius,
                    Stroke::new(1_f32, MUTED.gamma_multiply(0.7)),
                );
            }
        }
        // These are sample glyphs, not a claim that whole cells satisfy the filters.
        let half_cell = (results.step_blocks / bpp * 0.12).clamp(3., 8.) as f32;
        for candidate in &results.candidates {
            let point = world_to_screen(rect, center, bpp, candidate.point);
            let cell = Rect::from_center_size(point, Vec2::splat(half_cell * 2.));
            if !cell.intersects(rect) {
                continue;
            }
            painter.rect_filled(cell, 2., TEAL.gamma_multiply(0.08));
            painter.rect_stroke(
                cell,
                2.,
                Stroke::new(1_f32, TEAL.gamma_multiply(0.25)),
                egui::StrokeKind::Inside,
            );
        }
    }
    let Some(selected) = selected.filter(|m| m.id == "terralens:location") else {
        return;
    };
    let Some(candidate) = results
        .candidates
        .iter()
        .find(|c| c.point == selected.point())
    else {
        return;
    };
    let origin = world_to_screen(rect, center, bpp, candidate.point);
    let mut endpoints = Vec::with_capacity(2);
    if let Some(point) = candidate.nearest_rock_point {
        endpoints.push((
            point,
            format!(
                "{} · sampled {:.0} blocks",
                candidate.nearest_rock.as_deref().unwrap_or("Rock"),
                candidate.rock_distance.unwrap_or(0.)
            ),
            Color32::from_rgb(215, 190, 147),
        ));
    }
    if let Some(point) = candidate.nearest_ore_point {
        endpoints.push((
            point,
            format!(
                "{} · predicted {:.0} blocks",
                candidate.nearest_ore.as_deref().unwrap_or("Ore"),
                candidate.ore_distance.unwrap_or(0.)
            ),
            Color32::from_rgb(239, 184, 106),
        ));
    }
    for (point, label, color) in endpoints {
        let endpoint = world_to_screen(rect, center, bpp, point);
        // The clipped line still communicates the direction to off-screen evidence.
        painter.line_segment(
            [origin, endpoint],
            Stroke::new(1.5_f32, color.gamma_multiply(0.8)),
        );
        if !rect.shrink(8.).contains(endpoint) {
            continue;
        }
        painter.circle_filled(endpoint, 4., color);
        painter.circle_stroke(endpoint, 7., Stroke::new(1_f32, color));
        let galley = painter.layout_no_wrap(label, FontId::proportional(11.), TEXT);
        let width = galley.size().x.min(rect.width() - 16.);
        let at = Pos2::new(
            (endpoint.x + 11.).clamp(rect.left() + 8., rect.right() - width - 8.),
            (endpoint.y + 10.).clamp(rect.top() + 8., rect.bottom() - galley.size().y - 8.),
        );
        painter.rect_filled(
            Rect::from_min_size(at, galley.size()).expand(4.),
            3.,
            BG.gamma_multiply(0.95),
        );
        painter.galley(at, galley, TEXT);
    }
}
