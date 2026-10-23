//! Saved and seed-predicted surface tiles, rendered as small cached GPU textures.
use crate::{
    model::{Bounds, Point, StructureBounds},
    prism::{SurfaceChunk, SurfaceColumn},
};
use eframe::egui::{self, Color32, Rect, TextureHandle, TextureOptions, Ui};
use lru::LruCache;
use std::{
    collections::{HashSet, VecDeque},
    num::NonZeroUsize,
    path::PathBuf,
};

const CACHE_CHUNKS: usize = 4096;
const BATCH_CHUNKS: usize = 64;
const DETAIL_BPP: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SurfaceSource {
    Saved,
    Predicted,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SurfaceData {
    pub chunk: SurfaceChunk,
    pub source: SurfaceSource,
}

struct CachedChunk {
    data: SurfaceData,
    texture: Option<TextureHandle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ChunkKey(pub i32, pub i32);

pub(super) struct SurfaceView {
    dimension: crate::dimension::Dimension,
    slice_y: i32,
    enabled: bool,
    path: Option<PathBuf>,
    structure_inspection: Option<StructureBounds>,
    cache: LruCache<ChunkKey, Option<CachedChunk>>,
    requested: HashSet<ChunkKey>,
    uploads: VecDeque<ChunkKey>,
    upload_queued: HashSet<ChunkKey>,
    upload_center: Option<Point>,
    serial: u64,
    pending: Option<u64>,
    status: String,
}

impl Default for SurfaceView {
    fn default() -> Self {
        Self {
            dimension: crate::dimension::Dimension::Overworld,
            slice_y: 96,
            enabled: false,
            path: None,
            structure_inspection: None,
            cache: LruCache::new(NonZeroUsize::new(CACHE_CHUNKS).unwrap()),
            requested: HashSet::new(),
            uploads: VecDeque::new(),
            upload_queued: HashSet::new(),
            upload_center: None,
            serial: 0,
            pending: None,
            status: "Exact surface detail is off".into(),
        }
    }
}

impl SurfaceView {
    pub fn set_dimension(&mut self, dimension: crate::dimension::Dimension, slice_y: i32) {
        if self.dimension != dimension || self.slice_y != slice_y {
            self.dimension = dimension;
            self.slice_y = slice_y;
            self.invalidate(true);
        }
    }
    pub fn invalidate(&mut self, clear_cache: bool) {
        self.serial = self.serial.wrapping_add(1);
        self.pending = None;
        self.requested.clear();
        if clear_cache {
            self.cache.clear();
            self.uploads.clear();
            self.upload_queued.clear();
            self.upload_center = None;
        }
        self.update_status(0);
    }

    pub fn set_options(&mut self, enabled: bool, path: Option<PathBuf>) {
        if self.path == path && self.enabled == enabled {
            return;
        }
        if self.path != path {
            self.cache.clear();
            self.requested.clear();
            self.uploads.clear();
            self.upload_queued.clear();
            self.upload_center = None;
            self.pending = None;
        }
        self.path = path;
        self.enabled = enabled;
        self.status = if !enabled {
            "Exact surface detail is off".into()
        } else if self.path.is_none() {
            "Seed surface detail loads as you zoom in".into()
        } else {
            "Saved surface detail loads as you zoom in".into()
        };
    }

    /// Inspect only saved blocks within a selected structure's recorded bounds.
    /// Changing this overlay clears its chunk cache without resetting the world or
    /// invalidating unrelated biome/terrain tiles.
    pub fn set_structure_inspection(&mut self, bounds: Option<StructureBounds>) {
        if self.structure_inspection == bounds {
            return;
        }
        self.structure_inspection = bounds;
        self.invalidate(true);
        self.status = if bounds.is_some() {
            "Structure cutaway · saved blocks only".into()
        } else if self.path.is_some() {
            "Saved surface detail loads as you zoom in".into()
        } else {
            "Seed surface detail loads as you zoom in".into()
        };
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn visible_at(&self, layer_is_biomes: bool, bpp: f64) -> bool {
        self.enabled && layer_is_biomes && bpp <= DETAIL_BPP
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some() || !self.uploads.is_empty()
    }

    pub fn take_request(
        &mut self,
        bounds: Bounds,
        center: Point,
        bpp: f64,
    ) -> Option<(Option<PathBuf>, Vec<ChunkKey>, u64, Option<StructureBounds>)> {
        if !self.enabled || bpp > DETAIL_BPP || self.pending.is_some() {
            return None;
        }
        let path = self.path.clone();
        let mut min_x = (bounds.min_x.floor() as i32).div_euclid(16);
        let mut max_x = (bounds.max_x.ceil() as i32).div_euclid(16);
        let mut min_z = (bounds.min_z.floor() as i32).div_euclid(16);
        let mut max_z = (bounds.max_z.ceil() as i32).div_euclid(16);
        if let Some(clip) = self.structure_inspection {
            if clip.min_x > clip.max_x || clip.min_z > clip.max_z {
                return None;
            }
            min_x = min_x.max(clip.min_x.div_euclid(16));
            max_x = max_x.min(clip.max_x.div_euclid(16));
            min_z = min_z.max(clip.min_z.div_euclid(16));
            max_z = max_z.min(clip.max_z.div_euclid(16));
            if min_x > max_x || min_z > max_z {
                return None;
            }
        }
        // A large monitor can show more than 4096 chunks at the activation zoom.
        // Keep the full visible working set so it cannot evict itself while filling.
        let visible_count = (max_x as i64 - min_x as i64 + 1).max(0) as usize
            * (max_z as i64 - min_z as i64 + 1).max(0) as usize;
        let capacity = CACHE_CHUNKS.max(visible_count.saturating_add(256));
        if self.cache.cap().get() != capacity {
            self.cache.resize(NonZeroUsize::new(capacity).unwrap());
        }
        let mut missing = Vec::new();
        for cz in min_z..=max_z {
            for cx in min_x..=max_x {
                let key = ChunkKey(cx, cz);
                if self.cache.contains(&key) || self.requested.contains(&key) {
                    continue;
                }
                let distance =
                    ((cx as f64 + 0.5) * 16. - center.x).hypot((cz as f64 + 0.5) * 16. - center.z);
                missing.push((distance, key));
            }
        }
        missing.sort_by(|a, b| a.0.total_cmp(&b.0));
        let keys: Vec<_> = missing
            .into_iter()
            .take(BATCH_CHUNKS)
            .map(|(_, k)| k)
            .collect();
        if keys.is_empty() {
            return None;
        }
        self.serial = self.serial.wrapping_add(1);
        self.pending = Some(self.serial);
        self.requested.extend(keys.iter().copied());
        Some((path, keys, self.serial, self.structure_inspection))
    }

    pub fn receive(
        &mut self,
        serial: u64,
        results: Vec<(i32, i32, Result<Option<SurfaceData>, String>)>,
    ) {
        if self.pending != Some(serial) {
            return;
        }
        self.pending = None;
        let mut errors = 0;
        let mut inserted = Vec::new();
        for (cx, cz, result) in results {
            let key = ChunkKey(cx, cz);
            self.requested.remove(&key);
            match result {
                Ok(Some(data)) => {
                    self.cache.put(
                        key,
                        Some(CachedChunk {
                            data,
                            texture: None,
                        }),
                    );
                    if self.upload_queued.insert(key) {
                        self.uploads.push_back(key);
                    }
                    inserted.push(key);
                }
                Ok(None) => {
                    self.cache.put(key, None);
                }
                Err(_) => {
                    errors += 1;
                    self.cache.put(key, None);
                }
            }
        }
        for key in inserted {
            for neighbor in [
                ChunkKey(key.0 - 1, key.1),
                ChunkKey(key.0 + 1, key.1),
                ChunkKey(key.0, key.1 - 1),
                ChunkKey(key.0, key.1 + 1),
            ] {
                let needs_refresh = self
                    .cache
                    .get_mut(&neighbor)
                    .and_then(Option::as_mut)
                    .is_some_and(|entry| entry.texture.take().is_some());
                if needs_refresh && self.upload_queued.insert(neighbor) {
                    self.uploads.push_back(neighbor);
                }
            }
        }
        self.update_status(errors);
    }

    pub fn mark_missing(&mut self, serial: u64, keys: &[ChunkKey]) {
        if self.pending != Some(serial) {
            return;
        }
        self.pending = None;
        for key in keys {
            self.requested.remove(key);
        }
        self.update_status(0);
    }

    pub fn status(&self) -> &str {
        if !self.enabled {
            "Surface detail is off"
        } else if self.pending.is_some() {
            if self.dimension == crate::dimension::Dimension::Beneath {
                "Loading saved cave floor…"
            } else if self.path.is_some() {
                "Loading saved surface…"
            } else {
                "Generating seed surface…"
            }
        } else {
            &self.status
        }
    }

    fn update_status(&mut self, errors: usize) {
        let mut saved = 0;
        let mut predicted = 0;
        for entry in self.cache.iter().filter_map(|(_, entry)| entry.as_ref()) {
            match entry.data.source {
                SurfaceSource::Saved => saved += 1,
                SurfaceSource::Predicted => predicted += 1,
            }
        }
        if self.dimension == crate::dimension::Dimension::Beneath {
            let (min_y, max_y) = self.dimension.band(self.slice_y);
            self.status = format!(
                "Saved blocks Y {min_y}–{max_y} · {saved} chunks · unexplored areas show seed biomes{}",
                if errors > 0 {
                    format!(" · {errors} read errors")
                } else {
                    String::new()
                }
            );
            return;
        }
        if self.structure_inspection.is_some() {
            self.status = format!(
                "Structure cutaway · {saved} saved chunks · saved blocks only{}",
                if errors > 0 {
                    format!(" · {errors} read errors")
                } else {
                    String::new()
                }
            );
            return;
        }
        let source = match (saved > 0, predicted > 0) {
            (true, true) => "Saved",
            (true, false) => "Saved surface",
            (false, true) => "",
            (false, false) if self.path.is_some() => "Saved surface",
            (false, false) => "",
        };
        self.status = format!(
            "{}{} cached chunks{}",
            if source.is_empty() {
                String::new()
            } else {
                format!("{source} · ")
            },
            saved + predicted,
            if errors > 0 {
                format!(" · {errors} read errors")
            } else {
                String::new()
            }
        );
    }

    pub fn probe(&mut self, point: Point) -> Option<&SurfaceColumn> {
        if !self.enabled {
            return None;
        }
        let bx = point.x.floor() as i32;
        let bz = point.z.floor() as i32;
        let key = ChunkKey(bx.div_euclid(16), bz.div_euclid(16));
        let chunk = &self.cache.get(&key)?.as_ref()?.data.chunk;
        let x = bx.rem_euclid(16) as usize;
        let z = bz.rem_euclid(16) as usize;
        chunk
            .columns
            .get(z * 16 + x)
            .filter(|column| column.y != i32::MIN && !column.block.is_empty())
    }

    pub fn source(&mut self, point: Point) -> Option<SurfaceSource> {
        if !self.enabled {
            return None;
        }
        let bx = point.x.floor() as i32;
        let bz = point.z.floor() as i32;
        let key = ChunkKey(bx.div_euclid(16), bz.div_euclid(16));
        let x = bx.rem_euclid(16) as usize;
        let z = bz.rem_euclid(16) as usize;
        let entry = self.cache.get(&key)?.as_ref()?;
        let column = entry.data.chunk.columns.get(z * 16 + x)?;
        (column.y != i32::MIN && !column.block.is_empty()).then_some(entry.data.source)
    }

    pub fn paint(&mut self, ui: &Ui, rect: Rect, bounds: Bounds, bpp: f64) {
        if !self.enabled || bpp > DETAIL_BPP {
            return;
        }
        self.upload_one(ui, bounds);
        let painter = ui.painter().with_clip_rect(rect);
        let min_x = (bounds.min_x.floor() as i32).div_euclid(16);
        let max_x = (bounds.max_x.ceil() as i32).div_euclid(16);
        let min_z = (bounds.min_z.floor() as i32).div_euclid(16);
        let max_z = (bounds.max_z.ceil() as i32).div_euclid(16);
        for cz in min_z..=max_z {
            for cx in min_x..=max_x {
                let key = ChunkKey(cx, cz);
                let Some(Some(chunk)) = self.cache.get(&key) else {
                    continue;
                };
                let Some(texture) = chunk.texture.as_ref() else {
                    continue;
                };
                let min = egui::pos2(
                    rect.left() + ((cx as f64 * 16. - bounds.min_x) / bpp) as f32,
                    rect.top() + ((cz as f64 * 16. - bounds.min_z) / bpp) as f32,
                );
                let max = egui::pos2(min.x + 16. / bpp as f32, min.y + 16. / bpp as f32);
                let dest = Rect::from_min_max(min, max);
                if dest.intersects(rect) {
                    painter.image(
                        texture.id(),
                        dest,
                        Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                        Color32::WHITE,
                    );
                }
            }
        }
    }

    fn upload_one(&mut self, ui: &Ui, bounds: Bounds) {
        if self.uploads.is_empty() {
            return;
        }
        let center_x = (bounds.min_x + bounds.max_x) * 0.5;
        let center_z = (bounds.min_z + bounds.max_z) * 0.5;
        if self.upload_center.is_none_or(|p| {
            p.distance(Point {
                x: center_x,
                z: center_z,
            }) > 128.
        }) {
            self.uploads.make_contiguous().sort_by(|a, b| {
                let d = |k: &ChunkKey| {
                    ((k.0 as f64 + 0.5) * 16. - center_x).hypot((k.1 as f64 + 0.5) * 16. - center_z)
                };
                d(a).total_cmp(&d(b))
            });
            self.upload_center = Some(Point {
                x: center_x,
                z: center_z,
            });
        }
        while let Some(key) = self.uploads.pop_front() {
            self.upload_queued.remove(&key);
            let Some(Some(entry)) = self.cache.peek(&key) else {
                continue;
            };
            if entry.texture.is_some() {
                continue;
            }
            let pixels = self.rasterize(key);
            let image = egui::ColorImage::new([16, 16], pixels);
            let texture = ui.ctx().load_texture(
                format!("surface-{}-{}", key.0, key.1),
                image,
                TextureOptions::NEAREST,
            );
            if let Some(Some(entry)) = self.cache.get_mut(&key) {
                entry.texture = Some(texture);
            }
            break;
        }
    }

    fn rasterize(&self, key: ChunkKey) -> Vec<Color32> {
        let Some(Some(entry)) = self.cache.peek(&key) else {
            return vec![Color32::TRANSPARENT; 256];
        };
        (0..256)
            .map(|i| {
                let column = &entry.data.chunk.columns[i];
                if column.y == i32::MIN || column.block.is_empty() {
                    return Color32::TRANSPARENT;
                }
                let x = (i % 16) as i32;
                let z = (i / 16) as i32;
                let h = column.y as f32;
                let west = self.height_at(key, x - 1, z).unwrap_or(h);
                let east = self.height_at(key, x + 1, z).unwrap_or(h);
                let north = self.height_at(key, x, z - 1).unwrap_or(h);
                let south = self.height_at(key, x, z + 1).unwrap_or(h);
                let dx = (east - west) * 0.5;
                let dz = (south - north) * 0.5;
                let norm = (dx * dx + dz * dz + 1.).sqrt().recip();
                let light =
                    (0.86 + (-dx * -0.45 + 0.82 - dz * -0.38) * norm * 0.22).clamp(0.72, 1.15);
                let base = if self.dimension == crate::dimension::Dimension::Beneath {
                    column.color
                } else if let Some(tree) = column
                    .tree
                    .as_ref()
                    .filter(|_| entry.data.source == SurfaceSource::Predicted)
                {
                    if tree.contains("dead") {
                        [128, 110, 86]
                    } else if tree.contains("pine")
                        || tree.contains("fir")
                        || tree.contains("spruce")
                    {
                        [57, 100, 82]
                    } else if tree.contains("birch") || tree.contains("aspen") {
                        [85, 124, 76]
                    } else {
                        [67, 111, 73]
                    }
                } else if entry.data.source == SurfaceSource::Predicted && column.crop.is_some() {
                    [172, 151, 79]
                } else if column.tree.is_some() {
                    [56, 105, 62]
                } else if column.crop.is_some() {
                    [166, 145, 69]
                } else {
                    column.color
                };
                Color32::from_rgb(
                    (base[0] as f32 * light).clamp(0., 255.) as u8,
                    (base[1] as f32 * light).clamp(0., 255.) as u8,
                    (base[2] as f32 * light).clamp(0., 255.) as u8,
                )
            })
            .collect()
    }

    fn height_at(&self, key: ChunkKey, x: i32, z: i32) -> Option<f32> {
        let adjacent = ChunkKey(key.0 + x.div_euclid(16), key.1 + z.div_euclid(16));
        let entry = self.cache.peek(&adjacent)?.as_ref()?;
        let index = z.rem_euclid(16) as usize * 16 + x.rem_euclid(16) as usize;
        let y = entry.data.chunk.columns.get(index)?.y;
        (y != i32::MIN).then_some(y as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(min_x: f64, min_z: f64, max_x: f64, max_z: f64) -> Bounds {
        Bounds {
            min_x,
            min_z,
            max_x,
            max_z,
        }
    }

    fn dummy_chunk(cx: i32, cz: i32) -> SurfaceData {
        SurfaceData {
            chunk: SurfaceChunk {
                chunk_x: cx,
                chunk_z: cz,
                columns: (0..256)
                    .map(|_| SurfaceColumn {
                        y: 64,
                        block: "minecraft:stone".into(),
                        tree: None,
                        crop: None,
                        color: [120, 120, 120],
                    })
                    .collect(),
            },
            source: SurfaceSource::Predicted,
        }
    }

    #[test]
    fn far_viewport_requests_its_full_chunk_coverage_without_center_clipping() {
        let mut view = SurfaceView::default();
        view.set_options(true, None);
        let bounds = bounds(1_000_000., -2_000_000., 1_000_064., -1_999_936.);
        let center = Point { x: 0., z: 0. };
        let mut covered = HashSet::new();
        while let Some((_, keys, serial, _)) = view.take_request(bounds, center, 0.5) {
            assert!(keys.iter().all(|ChunkKey(x, z)| {
                (62_500..=62_504).contains(x) && (-125_000..=-124_996).contains(z)
            }));
            let results = keys
                .iter()
                .map(|key| {
                    covered.insert(*key);
                    (key.0, key.1, Ok(Some(dummy_chunk(key.0, key.1))))
                })
                .collect();
            view.receive(serial, results);
        }
        assert_eq!(covered.len(), 25);
        for z in -125_000..=-124_996 {
            for x in 62_500..=62_504 {
                assert!(covered.contains(&ChunkKey(x, z)));
            }
        }
    }

    #[test]
    fn cache_is_bounded_to_4096_and_backpressure_retries_the_same_chunks() {
        let mut view = SurfaceView::default();
        view.set_options(true, None);
        assert_eq!(view.cache.cap().get(), CACHE_CHUNKS);
        for n in 0..=CACHE_CHUNKS {
            view.cache.put(ChunkKey(n as i32, -7), None);
        }
        assert_eq!(view.cache.len(), CACHE_CHUNKS);

        let small = bounds(0., 0., 16., 16.);
        let (_, first, serial, _) = view
            .take_request(small, Point { x: 8., z: 8. }, 0.5)
            .unwrap();
        view.mark_missing(serial, &first);
        let (_, retry, _, _) = view
            .take_request(small, Point { x: 8., z: 8. }, 0.5)
            .unwrap();
        assert_eq!(retry, first);
    }

    #[test]
    fn per_frame_option_updates_preserve_surface_status() {
        let mut view = SurfaceView::default();
        view.set_options(true, None);
        view.status = "Seed prediction · 12 cached chunks".into();
        view.set_options(true, None);
        assert_eq!(view.status(), "Seed prediction · 12 cached chunks");
    }

    #[test]
    fn ultrawide_detail_view_finishes_without_evicting_visible_chunks() {
        let mut view = SurfaceView::default();
        view.set_options(true, None);
        // 3440 by 1440 screen points at 0.5 blocks/point exceeds the base LRU.
        let bounds = bounds(0., 0., 1720., 720.);
        let center = Point { x: 860., z: 360. };
        let mut requests = 0;
        loop {
            let Some((_, keys, serial, _)) = view.take_request(bounds, center, 0.5) else {
                break;
            };
            requests += 1;
            assert!(requests <= 100, "surface working set is evicting itself");
            view.receive(
                serial,
                keys.into_iter().map(|k| (k.0, k.1, Ok(None))).collect(),
            );
        }
        assert_eq!(view.cache.len(), 108 * 46);
        assert!(view.cache.cap().get() >= view.cache.len());
    }

    #[test]
    fn epoch_changes_release_pending_work_and_seed_changes_clear_surface_data() {
        let mut view = SurfaceView::default();
        view.set_options(true, None);
        let b = bounds(0., 0., 16., 16.);
        let (_, keys, serial, _) = view.take_request(b, Point::default(), 0.5).unwrap();
        view.invalidate(false);
        assert!(view.take_request(b, Point::default(), 0.5).is_some());
        view.receive(
            serial,
            vec![(
                keys[0].0,
                keys[0].1,
                Ok(Some(dummy_chunk(keys[0].0, keys[0].1))),
            )],
        );
        assert!(view.cache.is_empty(), "late epoch data must be ignored");
        view.cache.put(
            ChunkKey(0, 0),
            Some(CachedChunk {
                data: dummy_chunk(0, 0),
                texture: None,
            }),
        );
        view.invalidate(true);
        assert!(view.probe(Point::default()).is_none());
        assert!(view.pending.is_none());
    }

    #[test]
    fn changing_structure_cutaway_discards_only_the_detail_cache_and_attaches_clip() {
        let mut view = SurfaceView::default();
        view.set_options(true, Some(PathBuf::from("save/DIM-1")));
        let b = bounds(0., 0., 16., 16.);
        let (_, keys, serial, _) = view.take_request(b, Point::default(), 0.5).unwrap();
        view.receive(
            serial,
            vec![(
                keys[0].0,
                keys[0].1,
                Ok(Some(dummy_chunk(keys[0].0, keys[0].1))),
            )],
        );
        assert!(!view.cache.is_empty());
        let clip = StructureBounds {
            min_x: 2,
            min_y: 40,
            min_z: 3,
            max_x: 12,
            max_y: 90,
            max_z: 14,
        };
        view.set_structure_inspection(Some(clip));
        assert!(view.cache.is_empty());
        assert!(view.pending.is_none());
        assert_eq!(view.status(), "Structure cutaway · saved blocks only");
        let (_, _, _, attached) = view.take_request(b, Point::default(), 0.5).unwrap();
        assert_eq!(attached, Some(clip));
        view.set_structure_inspection(None);
        assert!(view.cache.is_empty());
        assert!(view.status().contains("Saved surface detail"));
    }

    #[test]
    fn disabled_surface_detail_neither_requests_nor_exposes_structure_data() {
        let mut view = SurfaceView::default();
        view.set_options(false, Some(PathBuf::from("save/DIM-1")));
        view.set_structure_inspection(Some(StructureBounds {
            min_x: 0,
            min_y: 0,
            min_z: 0,
            max_x: 15,
            max_y: 15,
            max_z: 15,
        }));
        assert!(
            view.take_request(bounds(0., 0., 16., 16.), Point::default(), 0.5)
                .is_none()
        );
        assert!(view.probe(Point::default()).is_none());
        assert!(view.source(Point::default()).is_none());
    }

    #[test]
    fn structure_clip_does_not_schedule_chunks_outside_its_horizontal_footprint() {
        let mut view = SurfaceView::default();
        view.set_options(true, Some(PathBuf::from("save")));
        view.set_structure_inspection(Some(StructureBounds {
            min_x: 32,
            min_y: 0,
            min_z: -16,
            max_x: 47,
            max_y: 80,
            max_z: -1,
        }));
        assert!(
            view.take_request(bounds(-256., -256., -128., -128.), Point::default(), 0.5)
                .is_none()
        );
        let (_, keys, _, _) = view
            .take_request(bounds(0., -32., 64., 16.), Point::default(), 0.5)
            .unwrap();
        assert_eq!(keys, vec![ChunkKey(2, -1)]);
    }
}
