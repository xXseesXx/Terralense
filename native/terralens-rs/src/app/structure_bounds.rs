//! Saved structure footprints, projected from their inclusive Minecraft bounding boxes.
use super::*;

/// Inspection is independent of mod/structure type; only Beneath has vertical bands.
pub(super) fn inspection_view(
    marker: &Marker,
    dimension: Dimension,
    slice_y: i32,
) -> Option<(Point, Option<i32>)> {
    if marker.kind != MarkerKind::Structure {
        return None;
    }
    let bounds = marker.bounds?;
    let center = marker_destination(marker);
    let band = if dimension == Dimension::Beneath {
        let (min, max) = dimension.band(slice_y);
        (!bounds.intersects_band(min, max))
            .then(|| dimension.band(slice_y.clamp(bounds.min_y, bounds.max_y)).1)
    } else {
        None
    };
    Some((center, band))
}

pub(super) fn paint(
    ui: &egui::Ui,
    rect: Rect,
    center: Point,
    bpp: f64,
    dimension: Dimension,
    slice_y: i32,
    markers: &[&Marker],
    selected: Option<&Marker>,
    pointer: Option<Pos2>,
) -> Option<Marker> {
    let painter = ui.painter().with_clip_rect(rect);
    let band = dimension.band(slice_y);
    let mut hit: Option<(f32, &Marker)> = None;
    for &marker in markers {
        if marker.kind != MarkerKind::Structure {
            continue;
        }
        let Some(bounds) = marker.bounds else {
            continue;
        };
        let selected = selected.is_some_and(|s| s.id == marker.id && s.point() == marker.point());
        if bpp > 8. && !selected {
            continue;
        }
        let active = dimension == Dimension::Overworld || bounds.intersects_band(band.0, band.1);
        let footprint = Rect::from_min_max(
            world_to_screen(
                rect,
                center,
                bpp,
                Point {
                    x: bounds.min_x as f64,
                    z: bounds.min_z as f64,
                },
            ),
            world_to_screen(
                rect,
                center,
                bpp,
                Point {
                    x: bounds.max_x as f64 + 1.,
                    z: bounds.max_z as f64 + 1.,
                },
            ),
        );
        if !footprint.intersects(rect) {
            continue;
        }
        let hover = pointer.is_some_and(|p| {
            let visible = footprint.intersect(rect);
            visible.contains(p) && (selected || edge_distance(footprint, p) < 7.)
        });
        let alpha = if !active {
            60
        } else if selected || hover {
            235
        } else {
            130
        };
        let color = Color32::from_rgba_unmultiplied(235, 190, 107, alpha);
        if active && (selected || hover) {
            painter.rect_filled(
                footprint,
                0.,
                Color32::from_rgba_unmultiplied(235, 190, 107, 12),
            );
        }
        painter.rect_stroke(
            footprint,
            0.,
            Stroke::new(if selected || hover { 2_f32 } else { 1_f32 }, color),
            egui::StrokeKind::Inside,
        );
        if hover {
            let area = footprint.width() * footprint.height();
            if hit.is_none_or(|(old, _)| area < old) {
                hit = Some((area, marker));
            }
        }
    }
    hit.map(|(_, marker)| marker.clone())
}

fn edge_distance(rect: Rect, point: Pos2) -> f32 {
    (point.x - rect.left())
        .abs()
        .min((point.x - rect.right()).abs())
        .min((point.y - rect.top()).abs())
        .min((point.y - rect.bottom()).abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::StructureBounds;

    #[test]
    fn all_structure_types_inspect_in_overworld_without_mutating_beneath_band() {
        for id in [
            "minecraft:village",
            "primal_life:ruin",
            "tfg:large_tower",
            "another_mod:underground",
        ] {
            let marker = structure(id);
            let (center, band) = inspection_view(&marker, Dimension::Overworld, 111).unwrap();
            assert_eq!(center, Point { x: 10., z: -20. });
            assert_eq!(band, None);
        }
    }

    #[test]
    fn beneath_inspection_retains_intersecting_band_and_chooses_nearest_occupied_band() {
        let marker = structure("any_mod:structure");
        assert_eq!(
            inspection_view(&marker, Dimension::Beneath, 111).unwrap().1,
            None
        );
        assert_eq!(
            inspection_view(&marker, Dimension::Beneath, 15).unwrap().1,
            Some(111)
        );
        assert_eq!(
            inspection_view(&marker, Dimension::Beneath, 255).unwrap().1,
            Some(127)
        );
    }

    fn structure(id: &str) -> Marker {
        Marker {
            id: id.into(),
            label: id.into(),
            namespace: id.split(':').next().unwrap().into(),
            x: -100.,
            z: 90.,
            y: Some(0.),
            kind: MarkerKind::Structure,
            confirmed: true,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [255, 200, 90],
            bounds: Some(StructureBounds {
                min_x: 0,
                min_z: -40,
                max_x: 20,
                max_z: 0,
                min_y: 96,
                max_y: 127,
            }),
        }
    }
}
