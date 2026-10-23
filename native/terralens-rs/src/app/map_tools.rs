//! Small map overlays and pointer tools. Drawing work is bounded by the viewport,
//! so a very wide zoom never creates an unbounded grid or overlay workload.

use crate::model::{Bounds, Point};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};

const TEAL: Color32 = Color32::from_rgb(104, 218, 198);
const GRID: Color32 = Color32::from_rgb(190, 215, 215);
const MAX_GRID_LINES: usize = 112;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct MapToolsPrefs {
    pub coordinate_grid: bool,
    pub chunk_grid: bool,
    pub region_grid: bool,
    pub scale_bar: bool,
}

impl Default for MapToolsPrefs {
    fn default() -> Self {
        Self {
            coordinate_grid: true,
            chunk_grid: false,
            region_grid: false,
            scale_bar: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DragTool {
    Ruler(Point),
    Area(Point),
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ToolOutput {
    /// True while a ruler or area selection owns the pointer drag.
    pub consumes_drag: bool,
    /// The current or last completed rectangular selection in world coordinates.
    pub selection: Option<Bounds>,
}

#[derive(Default)]
pub(super) struct MapTools {
    drag: Option<DragTool>,
    ruler: Option<(Point, Point)>,
    selection: Option<Bounds>,
}

impl MapTools {
    pub fn has_overlay(&self) -> bool {
        self.selection.is_some() || self.ruler.is_some()
    }
    pub fn selection(&self) -> Option<Bounds> {
        self.selection
    }

    pub fn set_selection(&mut self, bounds: Bounds) {
        self.clear_selection();
        self.selection = Some(Bounds {
            min_x: bounds.min_x.min(bounds.max_x),
            max_x: bounds.min_x.max(bounds.max_x),
            min_z: bounds.min_z.min(bounds.max_z),
            max_z: bounds.min_z.max(bounds.max_z),
        });
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.ruler = None;
        self.drag = None;
    }

    pub fn paint(
        &mut self,
        ui: &Ui,
        rect: Rect,
        center: Point,
        blocks_per_point: f64,
        response: &Response,
        prefs: &MapToolsPrefs,
    ) -> ToolOutput {
        if !blocks_per_point.is_finite() || blocks_per_point <= 0.0 || rect.is_negative() {
            return ToolOutput {
                consumes_drag: self.drag.is_some(),
                selection: self.selection,
            };
        }

        let painter = ui.painter().with_clip_rect(rect);
        let world_rect = visible_world_bounds(rect, center, blocks_per_point);
        if prefs.region_grid {
            paint_grid(
                &painter,
                rect,
                world_rect,
                center,
                blocks_per_point,
                512.0,
                30.0,
                Color32::from_rgba_unmultiplied(139, 187, 197, 95),
                1.1,
                false,
            );
        }
        if prefs.chunk_grid {
            paint_grid(
                &painter,
                rect,
                world_rect,
                center,
                blocks_per_point,
                16.0,
                12.0,
                Color32::from_rgba_unmultiplied(155, 191, 194, 58),
                0.7,
                false,
            );
        }
        if prefs.coordinate_grid {
            let desired = nice_step(blocks_per_point * 92.0);
            let max_step = ((world_rect.max_x - world_rect.min_x)
                .max(world_rect.max_z - world_rect.min_z)
                / MAX_GRID_LINES as f64)
                .max(desired);
            let step = nice_step(max_step);
            paint_grid(
                &painter,
                rect,
                world_rect,
                center,
                blocks_per_point,
                step,
                24.0,
                GRID.gamma_multiply(0.14),
                0.8,
                true,
            );
        }
        if prefs.scale_bar {
            paint_scale_bar(&painter, rect, blocks_per_point);
        }

        let (pointer, modifiers, started, stopped, dragged) = ui.input(|input| {
            (
                input.pointer.interact_pos(),
                input.modifiers,
                response.drag_started(),
                response.drag_stopped(),
                response.dragged(),
            )
        });
        if started {
            if let Some(pos) = pointer.filter(|p| rect.contains(*p)) {
                let origin = screen_to_world(pos, rect, center, blocks_per_point);
                if modifiers.shift {
                    self.drag = Some(DragTool::Ruler(origin));
                    self.ruler = Some((origin, origin));
                } else if modifiers.alt {
                    self.drag = Some(DragTool::Area(origin));
                    self.selection = Some(bounds_between(origin, origin));
                }
            }
        }

        if let (Some(tool), Some(pos)) = (self.drag, pointer) {
            let pos = rect.clamp(pos);
            let current = screen_to_world(pos, rect, center, blocks_per_point);
            match tool {
                DragTool::Ruler(origin) => {
                    self.ruler = Some((origin, current));
                    paint_ruler(&painter, rect, center, blocks_per_point, origin, current);
                }
                DragTool::Area(origin) => {
                    let bounds = bounds_between(origin, current);
                    self.selection = Some(bounds);
                    paint_selection(&painter, rect, center, blocks_per_point, bounds);
                }
            }
        } else if let Some(bounds) = self.selection {
            paint_selection(&painter, rect, center, blocks_per_point, bounds);
        } else if let Some((origin, end)) = self.ruler {
            paint_ruler(&painter, rect, center, blocks_per_point, origin, end);
        }

        let consumes_drag = self.drag.is_some();
        if stopped {
            self.drag = None;
        }

        ToolOutput {
            consumes_drag: consumes_drag || (dragged && (modifiers.shift || modifiers.alt)),
            selection: self.selection,
        }
    }
}

/// Draw requested search radius and the radius actually covered by available data.
pub(super) fn paint_search_area(
    ui: &Ui,
    rect: Rect,
    center: Point,
    blocks_per_point: f64,
    origin: Point,
    requested_radius: f64,
    actual_radius: f64,
) {
    if blocks_per_point <= 0.0 || !blocks_per_point.is_finite() || requested_radius <= 0.0 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    let origin_px = world_to_screen(origin, rect, center, blocks_per_point);
    let radius = (requested_radius / blocks_per_point) as f32;
    let covered = actual_radius.clamp(0.0, requested_radius);
    let covered_px = (covered / blocks_per_point) as f32;
    painter.circle_filled(
        origin_px,
        radius,
        Color32::from_rgba_unmultiplied(104, 218, 198, 15),
    );
    painter.circle_stroke(
        origin_px,
        radius,
        Stroke::new(1.0_f32, TEAL.gamma_multiply(0.68)),
    );
    if covered > 0.0 && covered < requested_radius {
        painter.circle_stroke(
            origin_px,
            covered_px,
            Stroke::new(1.4_f32, Color32::from_rgb(238, 194, 112)),
        );
    }
    let coverage = format!(
        "{} / {} km",
        format_distance(covered),
        format_distance(requested_radius)
    );
    let label_at = Pos2::new(origin_px.x + 8.0, origin_px.y - radius.min(38.0) - 8.0);
    painter.text(
        rect.clamp(label_at),
        Align2::LEFT_BOTTOM,
        coverage,
        FontId::proportional(11.0),
        Color32::from_rgb(220, 230, 220),
    );
}

fn visible_world_bounds(rect: Rect, center: Point, bpp: f64) -> Bounds {
    let half_w = rect.width() as f64 * bpp * 0.5;
    let half_h = rect.height() as f64 * bpp * 0.5;
    Bounds {
        min_x: center.x - half_w,
        max_x: center.x + half_w,
        min_z: center.z - half_h,
        max_z: center.z + half_h,
    }
}

fn screen_to_world(pos: Pos2, rect: Rect, center: Point, bpp: f64) -> Point {
    Point {
        x: center.x + (pos.x - rect.center().x) as f64 * bpp,
        z: center.z + (pos.y - rect.center().y) as f64 * bpp,
    }
}

fn world_to_screen(point: Point, rect: Rect, center: Point, bpp: f64) -> Pos2 {
    Pos2::new(
        rect.center().x + ((point.x - center.x) / bpp) as f32,
        rect.center().y + ((point.z - center.z) / bpp) as f32,
    )
}

fn bounds_between(a: Point, b: Point) -> Bounds {
    Bounds {
        min_x: a.x.min(b.x),
        min_z: a.z.min(b.z),
        max_x: a.x.max(b.x),
        max_z: a.z.max(b.z),
    }
}

fn paint_selection(painter: &egui::Painter, rect: Rect, center: Point, bpp: f64, bounds: Bounds) {
    let a = world_to_screen(
        Point {
            x: bounds.min_x,
            z: bounds.min_z,
        },
        rect,
        center,
        bpp,
    );
    let b = world_to_screen(
        Point {
            x: bounds.max_x,
            z: bounds.max_z,
        },
        rect,
        center,
        bpp,
    );
    let box_rect = Rect::from_two_pos(a, b).intersect(rect);
    painter.rect_filled(
        box_rect,
        2.0,
        Color32::from_rgba_unmultiplied(104, 218, 198, 22),
    );
    painter.rect_stroke(
        box_rect,
        2.0,
        Stroke::new(1.3_f32, TEAL),
        egui::StrokeKind::Inside,
    );
    let width = (bounds.max_x - bounds.min_x).abs();
    let height = (bounds.max_z - bounds.min_z).abs();
    let label = format!(
        "{} × {} blocks",
        width.round() as i64,
        height.round() as i64
    );
    painter.text(
        rect.clamp(Pos2::new(box_rect.left() + 5.0, box_rect.top() - 4.0)),
        Align2::LEFT_BOTTOM,
        label,
        FontId::proportional(11.0),
        Color32::from_rgb(224, 235, 233),
    );
}

fn paint_ruler(painter: &egui::Painter, rect: Rect, center: Point, bpp: f64, a: Point, b: Point) {
    let a_px = world_to_screen(a, rect, center, bpp);
    let b_px = world_to_screen(b, rect, center, bpp);
    painter.line_segment(
        [a_px, b_px],
        Stroke::new(2.0_f32, Color32::from_rgb(248, 205, 121)),
    );
    painter.circle_filled(a_px, 3.0, Color32::from_rgb(248, 205, 121));
    painter.circle_filled(b_px, 3.0, Color32::from_rgb(248, 205, 121));
    let distance = a.distance(b);
    let mid = Pos2::new((a_px.x + b_px.x) * 0.5, (a_px.y + b_px.y) * 0.5);
    painter.text(
        rect.clamp(mid + Vec2::new(6.0, -8.0)),
        Align2::LEFT_BOTTOM,
        format!(
            "{} blocks · {} km",
            distance.round() as u64,
            format_distance(distance)
        ),
        FontId::proportional(11.0),
        Color32::from_rgb(248, 220, 164),
    );
}

fn paint_scale_bar(painter: &egui::Painter, rect: Rect, bpp: f64) {
    let step = nice_step(112.0 * bpp);
    let width = (step / bpp) as f32;
    let right = rect.left() + 18.0 + width;
    let bottom = rect.bottom() - 18.0;
    let left = right - width;
    if width < 28.0 || left < rect.left() + 8.0 || !width.is_finite() {
        return;
    }
    let color = Color32::from_rgb(224, 235, 233);
    painter.line_segment(
        [Pos2::new(left, bottom), Pos2::new(right, bottom)],
        Stroke::new(2.0_f32, color),
    );
    for x in [left, right] {
        painter.line_segment(
            [Pos2::new(x, bottom - 4.0), Pos2::new(x, bottom + 1.0)],
            Stroke::new(1.5_f32, color),
        );
    }
    let label = if step >= 1000.0 {
        format!("{} km", format_distance(step))
    } else {
        format!("{} blocks", step.round() as u64)
    };
    painter.text(
        Pos2::new((left + right) * 0.5, bottom - 5.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::proportional(10.0),
        color,
    );
}

fn paint_grid(
    painter: &egui::Painter,
    rect: Rect,
    world: Bounds,
    center: Point,
    bpp: f64,
    requested_step: f64,
    min_screen_spacing: f64,
    color: Color32,
    width: f32,
    labels: bool,
) {
    if requested_step <= 0.0 || !requested_step.is_finite() {
        return;
    }
    let x_step = bounded_step(
        requested_step,
        world.max_x - world.min_x,
        bpp,
        min_screen_spacing,
    );
    let z_step = bounded_step(
        requested_step,
        world.max_z - world.min_z,
        bpp,
        min_screen_spacing,
    );
    let x_first = (world.min_x / x_step).ceil() as i64;
    let x_last = (world.max_x / x_step).floor() as i64;
    for index in x_first..=x_last.min(x_first + MAX_GRID_LINES as i64 - 1) {
        let x = index as f64 * x_step;
        let sx = world_to_screen(Point { x, z: center.z }, rect, center, bpp).x;
        painter.line_segment(
            [Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())],
            Stroke::new(width, color),
        );
        if labels && ((x_step / bpp) >= 72.0) {
            let label = format_coord(x);
            painter.text(
                Pos2::new(sx + 3.0, rect.top() + 3.0),
                Align2::LEFT_TOP,
                label,
                FontId::proportional(10.0),
                Color32::from_rgb(151, 176, 180),
            );
        }
    }
    let z_first = (world.min_z / z_step).ceil() as i64;
    let z_last = (world.max_z / z_step).floor() as i64;
    for index in z_first..=z_last.min(z_first + MAX_GRID_LINES as i64 - 1) {
        let z = index as f64 * z_step;
        let sy = world_to_screen(Point { x: center.x, z }, rect, center, bpp).y;
        painter.line_segment(
            [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
            Stroke::new(width, color),
        );
        if labels && ((z_step / bpp) >= 72.0) {
            painter.text(
                Pos2::new(rect.left() + 3.0, sy + 3.0),
                Align2::LEFT_TOP,
                format_coord(z),
                FontId::proportional(10.0),
                Color32::from_rgb(151, 176, 180),
            );
        }
    }
}

fn bounded_step(step: f64, span: f64, bpp: f64, min_spacing: f64) -> f64 {
    let pixels = step / bpp;
    let count = span / step;
    if pixels >= min_spacing && count <= MAX_GRID_LINES as f64 {
        return step;
    }
    nice_step(
        step.max(span / MAX_GRID_LINES as f64)
            .max(bpp * min_spacing),
    )
}

fn nice_step(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return 1.0;
    }
    let power = 10f64.powf(value.log10().floor());
    let fraction = value / power;
    let multiple = if fraction <= 1.0 {
        1.0
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 5.0 {
        5.0
    } else {
        10.0
    };
    multiple * power
}

fn format_coord(value: f64) -> String {
    format!("{}", value.round() as i64)
}

fn format_distance(blocks: f64) -> String {
    let km = blocks / 1000.0;
    if km < 10.0 {
        format!("{km:.2}")
    } else if km < 100.0 {
        format!("{km:.1}")
    } else {
        format!("{km:.0}")
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_GRID_LINES, bounded_step, nice_step};

    #[test]
    fn adaptive_steps_use_readable_one_two_five_spacing() {
        assert_eq!(nice_step(1.2), 2.0);
        assert_eq!(nice_step(4.9), 5.0);
        assert_eq!(nice_step(13.0), 20.0);
        assert_eq!(nice_step(0.0), 1.0);
    }

    #[test]
    fn overlay_line_spacing_and_count_stay_bounded() {
        for (step, span, bpp, min_px) in [
            (16.0, 5000.0, 0.01, 12.0),
            (512.0, 8_000_000.0, 8000.0, 30.0),
            (100.0, 100_000.0, 0.2, 24.0),
        ] {
            let result = bounded_step(step, span, bpp, min_px);
            assert!(span / result <= MAX_GRID_LINES as f64 + 1.0);
            assert!(result / bpp >= min_px || span / result <= MAX_GRID_LINES as f64);
        }
    }

    #[test]
    fn grid_indexing_keeps_negative_world_lines_aligned() {
        let step = 512.0_f64;
        let min = -1700.0;
        let max = -500.0;
        let first = (min / step).ceil() as i64;
        let last = (max / step).floor() as i64;
        let lines: Vec<_> = (first..=last).map(|index| index as f64 * step).collect();
        assert_eq!(lines, vec![-1536.0, -1024.0, -512.0]);
    }
}
