//! Cached map rendering and compact inventory controls for one selected structure.
use crate::structure_preview as data;
use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle, TextureOptions, Vec2};

const MAP_TEXTURE_SIZE: u32 = 1024;

#[derive(Default)]
pub(super) struct State {
    key: Option<String>,
    texture: Option<TextureHandle>,
}

impl State {
    /// Upload only when the selected cached inspection changes; panning remains draw-only.
    pub fn update_texture(
        &mut self,
        ctx: &egui::Context,
        preview: Option<&data::StructurePreview>,
        revision: u64,
        cut_y: Option<i32>,
    ) {
        let Some(preview) = preview else {
            self.key = None;
            self.texture = None;
            return;
        };
        let key = format!(
            "{revision}:{}:{:?}:{}:{}:{cut_y:?}",
            preview.structure_id,
            preview.bounds,
            preview.blocks.len(),
            preview.source as u8
        );
        if self.key.as_deref() == Some(&key) {
            return;
        }
        self.key = Some(key);
        self.texture = rasterize(preview, cut_y)
            .map(|image| ctx.load_texture("structure-inspection", image, TextureOptions::NEAREST));
    }

    pub fn paint_shape(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        center: crate::model::Point,
        bpp: f64,
        preview: &data::StructurePreview,
    ) {
        if bpp > 16.0 {
            return;
        }
        let Some(bounds) = preview.bounds else {
            return;
        };
        let world = |x: f64, z: f64| {
            Pos2::new(
                rect.center().x + ((x - center.x) / bpp) as f32,
                rect.center().y + ((z - center.z) / bpp) as f32,
            )
        };
        let painter = ui.painter().with_clip_rect(rect);
        let footprint = Rect::from_min_max(
            world(bounds.min_x as f64, bounds.min_z as f64),
            world(bounds.max_x as f64 + 1., bounds.max_z as f64 + 1.),
        );
        if let Some(texture) = &self.texture {
            painter.image(
                texture.id(),
                footprint,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
        // Individual recorded pieces show the actual non-rectangular extent at close zoom.
        for piece in &preview.pieces {
            let piece_rect = Rect::from_min_max(
                world(piece.bounds.min_x as f64, piece.bounds.min_z as f64),
                world(
                    piece.bounds.max_x as f64 + 1.,
                    piece.bounds.max_z as f64 + 1.,
                ),
            );
            painter.rect_stroke(
                piece_rect,
                0.,
                egui::Stroke::new(1_f32, Color32::from_rgba_unmultiplied(249, 223, 163, 190)),
                egui::StrokeKind::Inside,
            );
        }
    }

    pub fn paint_containers(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        center: crate::model::Point,
        bpp: f64,
        preview: &data::StructurePreview,
        selected_container: Option<usize>,
        pointer: Option<Pos2>,
    ) -> Option<(usize, f32)> {
        if bpp > 16.0 {
            return None;
        }
        let _bounds = preview.bounds?;
        let world = |x: f64, z: f64| {
            Pos2::new(
                rect.center().x + ((x - center.x) / bpp) as f32,
                rect.center().y + ((z - center.z) / bpp) as f32,
            )
        };
        let painter = ui.painter().with_clip_rect(rect);
        let mut hit = None;
        let mut best = f32::INFINITY;
        for (index, container) in preview.containers.iter().enumerate() {
            let pos = world(container.pos.x as f64 + 0.5, container.pos.z as f64 + 0.5);
            if !rect.expand(10.).contains(pos) {
                continue;
            }
            let side = (11.0_f64 / bpp.max(0.45)).clamp(4.0, 12.0) as f32;
            let active = selected_container == Some(index);
            if active {
                painter.circle_filled(
                    pos,
                    side * 0.8,
                    Color32::from_rgba_unmultiplied(19, 27, 30, 225),
                );
            }
            // Minimal chest glyph with a contrasting lid and latch; positioned from block coords.
            let chest = Rect::from_center_size(pos, Vec2::new(side, side * 0.68));
            painter.rect_filled(chest, 1., Color32::from_rgb(145, 94, 49));
            painter.rect_filled(
                Rect::from_min_max(chest.left_top(), Pos2::new(chest.right(), chest.center().y)),
                1.,
                Color32::from_rgb(194, 144, 78),
            );
            painter.line_segment(
                [
                    Pos2::new(chest.center().x, chest.center().y - 1.),
                    Pos2::new(chest.center().x, chest.bottom()),
                ],
                egui::Stroke::new(1_f32, Color32::from_rgb(61, 43, 31)),
            );
            if let Some(distance) = container_hit(pointer, rect, pos, (side * 0.8).max(8.))
                && distance < best
            {
                best = distance;
                hit = Some((index, distance));
            }
        }
        hit
    }
}

fn container_hit(pointer: Option<Pos2>, rect: Rect, center: Pos2, radius: f32) -> Option<f32> {
    let pointer = pointer.filter(|pointer| rect.contains(*pointer))?;
    let distance = pointer.distance(center);
    (distance <= radius).then_some(distance)
}

fn rasterize(preview: &data::StructurePreview, cut_y: Option<i32>) -> Option<egui::ColorImage> {
    if preview.blocks.is_empty() {
        return None;
    }
    let bounds = preview.bounds?;
    let width = (bounds.max_x as i64 - bounds.min_x as i64 + 1).clamp(1, 65_536) as f64;
    let height = (bounds.max_z as i64 - bounds.min_z as i64 + 1).clamp(1, 65_536) as f64;
    let scale = (MAP_TEXTURE_SIZE as f64 / width.max(height)).min(1.0);
    let out_w = (width * scale).ceil().clamp(1., MAP_TEXTURE_SIZE as f64) as u32;
    let out_h = (height * scale).ceil().clamp(1., MAP_TEXTURE_SIZE as f64) as u32;
    let mut image = image::RgbaImage::new(out_w, out_h);
    let mut highest = vec![i32::MIN; out_w as usize * out_h as usize];
    for block in &preview.blocks {
        if cut_y.is_some_and(|cut| block.pos.y > cut) {
            continue;
        }
        if block.pos.x < bounds.min_x
            || block.pos.x > bounds.max_x
            || block.pos.z < bounds.min_z
            || block.pos.z > bounds.max_z
        {
            continue;
        }
        let x = (((block.pos.x - bounds.min_x) as f64 + 0.5) * scale).floor() as u32;
        let y = (((block.pos.z - bounds.min_z) as f64 + 0.5) * scale).floor() as u32;
        let index = y.min(out_h - 1) as usize * out_w as usize + x.min(out_w - 1) as usize;
        if block.pos.y >= highest[index] {
            highest[index] = block.pos.y;
            image.put_pixel(
                x.min(out_w - 1),
                y.min(out_h - 1),
                image::Rgba([block.color[0], block.color[1], block.color[2], 208]),
            );
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [out_w as usize, out_h as usize],
        image.as_raw(),
    ))
}

pub(super) fn show_details(
    ui: &mut egui::Ui,
    preview: Option<&data::StructurePreview>,
    error: Option<&str>,
    pending: bool,
    selected: &mut Option<usize>,
    show_layout: &mut bool,
    cut_y: &mut Option<i32>,
) -> bool {
    let mut reload = false;
    if let Some(preview) = preview {
        ui.horizontal(|ui| {
            ui.checkbox(show_layout, "Show structure layout");
            ui.label(match preview.source {
                data::PreviewSource::Saved => "Saved blocks",
                data::PreviewSource::SeedLayout => "Seed layout",
                data::PreviewSource::TemplateLayout if !preview.complete => {
                    "Partial template layout"
                }
                data::PreviewSource::TemplateLayout => "Template layout",
            });
            if preview.source == data::PreviewSource::Saved {
                reload = ui
                    .add_enabled(!pending, egui::Button::new("Refresh").small())
                    .on_hover_text(
                        "Read the saved structure and newly opened player inventories again",
                    )
                    .clicked();
            }
        });
        if let Some(bounds) = preview.bounds {
            ui.horizontal(|ui| {
                let mut cut_enabled = cut_y.is_some();
                if ui.checkbox(&mut cut_enabled, "Cutaway").changed() {
                    *cut_y = cut_enabled.then_some(bounds.max_y);
                }
                if let Some(value) = cut_y {
                    ui.add(egui::Slider::new(value, bounds.min_y..=bounds.max_y).text("Y"));
                    if ui.small_button("Full height").clicked() {
                        *cut_y = None;
                    }
                }
            });
        }
        if !preview.notes.is_empty() {
            ui.collapsing("Generation details", |ui| {
                for note in &preview.notes {
                    ui.label(egui::RichText::new(note).size(10.).color(super::MUTED));
                }
            });
        }
        if preview.containers.is_empty() {
            ui.label(
                egui::RichText::new("No containers in this layout")
                    .size(10.)
                    .color(super::MUTED),
            );
        } else {
            ui.label(format!("{} containers", preview.containers.len()));
            egui::ScrollArea::vertical()
                .id_salt("structure-containers")
                .max_height(135.)
                .show(ui, |ui| {
                    for (index, container) in preview.containers.iter().enumerate() {
                        let pos = container.pos;
                        let label = format!(
                            "{}  ·  {}, {}, {}",
                            container
                                .block
                                .rsplit(':')
                                .next()
                                .unwrap_or(&container.block)
                                .replace('_', " "),
                            pos.x,
                            pos.y,
                            pos.z
                        );
                        if ui
                            .selectable_label(*selected == Some(index), label)
                            .clicked()
                        {
                            *selected = Some(index);
                            *show_layout = true;
                            *cut_y = preview.bounds.map(|bounds| {
                                container
                                    .pos
                                    .y
                                    .saturating_add(1)
                                    .clamp(bounds.min_y, bounds.max_y)
                            });
                        }
                    }
                });
            if let Some(index) = *selected
                && let Some(container) = preview.containers.get(index)
            {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!(
                        "{}, {}, {}",
                        container.pos.x, container.pos.y, container.pos.z
                    ))
                    .monospace()
                    .size(10.),
                );
                if container.lootr {
                    ui.label(
                        "Lootr · contents vary by player; opened inventories are listed below",
                    );
                }
                let template_inventory =
                    preview.source != data::PreviewSource::Saved && !container.items.is_empty();
                if (container.contents_known || template_inventory)
                    && (!container.lootr || container.player_inventories.is_empty())
                {
                    ui.label(if preview.source == data::PreviewSource::Saved {
                        "Saved contents"
                    } else {
                        "Template contents"
                    });
                    if template_inventory && !container.contents_known {
                        ui.label(
                            egui::RichText::new("Processors may change these items")
                                .size(10.)
                                .color(super::MUTED),
                        );
                    }
                    if container.items.is_empty() {
                        ui.label("Empty");
                    }
                    show_inventory(ui, &container.items);
                }
                for inventory in &container.player_inventories {
                    ui.collapsing(format!("Saved player {}", inventory.uuid), |ui| {
                        if inventory.items.is_empty() {
                            ui.label("Empty");
                        }
                        show_inventory(ui, &inventory.items);
                    });
                }
                if !container.contents_known && container.loot_table.is_some() {
                    ui.label(format!(
                        "Table: {}",
                        container.loot_table.as_deref().unwrap_or_default()
                    ));
                }
                if !container.possible_drops.is_empty() {
                    ui.label("Possible drops");
                }
                for drop in &container.possible_drops {
                    let count = if drop.min == drop.max {
                        format!("×{}", drop.min)
                    } else {
                        format!("×{}–{}", drop.min, drop.max)
                    };
                    ui.label(format!(
                        "{} {count}",
                        drop.id
                            .rsplit(':')
                            .next()
                            .unwrap_or(&drop.id)
                            .replace('_', " ")
                    ));
                }
                for note in &container.notes {
                    ui.label(egui::RichText::new(note).size(10.).color(super::MUTED));
                }
            }
        }
    } else if let Some(error) = error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
        reload = ui.small_button("Retry inspection").clicked();
    } else if pending {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Reading structure layout and containers");
        });
    }
    reload
}

fn inventory_totals(items: &[data::ItemStack]) -> std::collections::BTreeMap<(&str, &str), u64> {
    let mut totals = std::collections::BTreeMap::new();
    for item in items {
        *totals
            .entry((item.id.as_str(), item.description.as_str()))
            .or_insert(0) += u64::from(item.count);
    }
    totals
}

fn show_inventory(ui: &mut egui::Ui, items: &[data::ItemStack]) {
    for ((id, description), count) in inventory_totals(items) {
        let response = ui.label(format!(
            "{} ×{count}",
            id.rsplit(':').next().unwrap_or(id).replace('_', " ")
        ));
        if !description.trim().is_empty() {
            response.on_hover_text(description.trim());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::StructureBounds,
        structure_preview::{BlockPos, Piece, PreviewSource, ShapeBlock, StructurePreview},
    };

    #[test]
    fn inventory_display_combines_split_stacks_but_keeps_distinct_item_metadata() {
        let item = |id: &str, count, description: &str| data::ItemStack {
            id: id.into(),
            count,
            slot: Some(1),
            description: description.into(),
        };
        let stacks = vec![
            item("tfc:straw", 2, ""),
            item("tfc:straw", 4, ""),
            item("ad_astra:gas_tank", 1, "Fluid: 1000 mB heliox"),
            item("ad_astra:gas_tank", 1, ""),
        ];
        let totals = inventory_totals(&stacks);
        assert_eq!(totals.len(), 3);
        assert_eq!(totals[&("tfc:straw", "")], 6);
        assert_eq!(totals[&("ad_astra:gas_tank", "Fluid: 1000 mB heliox")], 1);
        assert_eq!(
            stacks.len(),
            4,
            "the display must retain the underlying saved slot data"
        );
    }

    fn sample() -> StructurePreview {
        StructurePreview {
            structure_id: "test:building".into(),
            bounds: Some(StructureBounds {
                min_x: -2,
                min_y: 60,
                min_z: -4,
                max_x: 0,
                max_y: 90,
                max_z: -3,
            }),
            source: PreviewSource::Saved,
            pieces: vec![Piece {
                template: Some("test:room".into()),
                bounds: StructureBounds {
                    min_x: -2,
                    min_y: 60,
                    min_z: -4,
                    max_x: 0,
                    max_y: 90,
                    max_z: -3,
                },
            }],
            blocks: vec![
                ShapeBlock {
                    pos: BlockPos {
                        x: -2,
                        y: 70,
                        z: -4,
                    },
                    block: "test:wall".into(),
                    color: [1, 2, 3],
                },
                ShapeBlock {
                    pos: BlockPos {
                        x: -2,
                        y: 80,
                        z: -4,
                    },
                    block: "test:roof".into(),
                    color: [9, 8, 7],
                },
                ShapeBlock {
                    pos: BlockPos { x: 0, y: 67, z: -3 },
                    block: "test:floor".into(),
                    color: [4, 5, 6],
                },
            ],
            containers: Vec::new(),
            notes: Vec::new(),
            complete: true,
        }
    }

    #[test]
    fn shape_raster_keeps_negative_coordinates_and_empty_columns_sparse() {
        let image = rasterize(&sample(), None).unwrap();
        assert_eq!(image.size, [3, 2]);
        assert_eq!(
            image.pixels[0],
            Color32::from_rgba_unmultiplied(9, 8, 7, 208)
        );
        assert_eq!(
            image.pixels[5],
            Color32::from_rgba_unmultiplied(4, 5, 6, 208)
        );
        assert_eq!(image.pixels[1].a(), 0);
    }

    #[test]
    fn cutaway_raster_hides_roof_and_keeps_lower_wall() {
        let image = rasterize(&sample(), Some(75)).unwrap();
        assert_eq!(
            image.pixels[0],
            Color32::from_rgba_unmultiplied(1, 2, 3, 208)
        );
        assert_eq!(
            image.pixels[5],
            Color32::from_rgba_unmultiplied(4, 5, 6, 208)
        );
    }

    #[test]
    fn container_hit_testing_requires_pointer_inside_map() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.));
        let chest = Pos2::new(50., 50.);
        assert_eq!(container_hit(None, rect, chest, 8.), None);
        assert_eq!(
            container_hit(Some(Pos2::new(150., 50.)), rect, chest, 8.),
            None
        );
        assert_eq!(container_hit(Some(chest), rect, chest, 8.), Some(0.));
    }
}
