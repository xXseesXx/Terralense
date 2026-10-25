//! Responsive native atlas. Generation and save scans run in a separate Rust process.
//! The UI only schedules work, consumes bounded results and displays cached textures.
mod composition;
mod engine;
mod sampling;
mod streaming;
mod surface;
use streaming::{TileScheduler, TileTask};
pub use surface::SurfaceSource;

use crate::dimension::Dimension;
use crate::{
    analysis,
    display::{DisplayPrefs, LoadingFilter, MapDetail, TerrainDetailOptions},
    exports,
};
use crate::{
    biome, features,
    model::{Bounds, Marker, MarkerKind, Point, Probe, Settings, StructureBounds, TreeEstimate},
    prism,
    world::World,
};
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions, Ui};
use lru::LruCache;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const TILE_POINTS: f64 = 256.;
const COARSE: usize = 64;
const DETAIL: usize = 256;
const TILE_CACHE: usize = 320;
const MARKER_CELL: f64 = 512.0;
const FEATURE_TILE_CACHE: usize = 256;
const MIN_FEATURE_TILE_BLOCKS: f64 = 256.0;
const MIN_FEATURE_COVERAGE_BLOCKS: f64 = 64.0;
// Keep hover connectivity work small enough to stay comfortable inside a UI frame.
const MAX_REGION_CELLS: usize = 16_384;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default,
)]
pub enum Layer {
    #[default]
    Biomes,
    Temperature,
    Rainfall,
    Height,
    Water,
    Geology,
    Forest,
    Hotspots,
    Ground,
}
impl Layer {
    pub const ALL: [Self; 9] = [
        Self::Biomes,
        Self::Temperature,
        Self::Rainfall,
        Self::Height,
        Self::Water,
        Self::Geology,
        Self::Forest,
        Self::Hotspots,
        Self::Ground,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Biomes => "Biomes",
            Self::Temperature => "Temperature",
            Self::Rainfall => "Rainfall",
            Self::Height => "Elevation",
            Self::Water => "Water & rivers",
            Self::Geology => "Geology",
            Self::Forest => "Forests",
            Self::Hotspots => "Hotspots",
            Self::Ground => "Ground blocks",
        }
    }
}

/// Biome frequencies sampled from already-rendered tiles in the current viewport.
/// `sampled` is the bounded grid size; `loaded` is the subset backed by cached tile data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegendSnapshot {
    pub entries: Vec<LegendEntry>,
    pub sampled: usize,
    pub loaded: usize,
    pub layer: Layer,
    pub dimension: Dimension,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    pub id: u16,
    pub name: String,
    pub color: [u8; 3],
    pub count: u32,
}

impl LegendEntry {
    /// Share of loaded samples, never presented as a share of the whole viewport.
    pub fn percent(&self, loaded: usize) -> f32 {
        if loaded == 0 {
            0.0
        } else {
            self.count as f32 * 100.0 / loaded as f32
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerScale {
    pub min: f32,
    pub max: f32,
    pub unit: &'static str,
    pub stops: Vec<[u8; 3]>,
    pub low: &'static str,
    pub high: &'static str,
}

fn planetary_height_stops(dimension: Dimension) -> &'static [[u8; 3]] {
    if dimension == Dimension::Moon {
        &[
            [70, 72, 83],
            [125, 127, 137],
            [183, 183, 188],
            [235, 234, 229],
        ]
    } else {
        &[[85, 58, 59], [139, 84, 60], [197, 136, 83], [231, 191, 137]]
    }
}

pub fn dimension_layer_scale(
    dimension: Dimension,
    layer: Layer,
    palette: crate::display::Palette,
) -> Option<LayerScale> {
    if dimension.is_planetary() {
        return (layer == Layer::Height).then(|| LayerScale {
            min: 0.,
            max: 192.,
            unit: "m",
            stops: planetary_height_stops(dimension).to_vec(),
            low: "0 m",
            high: "192 m",
        });
    }
    layer_scale(layer, palette)
}

/// Numeric color ramps used by the map, exposed to keep legend swatches in sync.
pub fn layer_scale(layer: Layer, palette: crate::display::Palette) -> Option<LayerScale> {
    let (mut min, mut max, unit, natural, accessible, low, high): (
        f32,
        f32,
        &'static str,
        &[[u8; 3]],
        &[[u8; 3]],
        &'static str,
        &'static str,
    ) = match layer {
        Layer::Temperature => (
            -25.0,
            35.0,
            "°C",
            &[
                [124, 161, 191],
                [180, 210, 208],
                [223, 223, 175],
                [212, 163, 105],
                [173, 85, 71],
            ],
            &[
                [51, 78, 139],
                [164, 193, 207],
                [248, 219, 114],
                [209, 119, 42],
            ],
            "−25 °C",
            "+35 °C",
        ),
        Layer::Rainfall => (
            0.0,
            500.0,
            "mm",
            &[
                [210, 190, 145],
                [177, 191, 136],
                [94, 153, 127],
                [45, 116, 129],
                [48, 87, 134],
            ],
            &[
                [247, 225, 156],
                [125, 193, 192],
                [43, 114, 142],
                [20, 55, 95],
            ],
            "0 mm",
            "500 mm",
        ),
        Layer::Height => (
            60.0,
            260.0,
            "m",
            &[
                [117, 160, 120],
                [159, 173, 133],
                [174, 157, 126],
                [157, 144, 136],
                [220, 226, 222],
            ],
            &[
                [117, 160, 120],
                [159, 173, 133],
                [174, 157, 126],
                [157, 144, 136],
                [220, 226, 222],
            ],
            "60 m",
            "260 m",
        ),
        Layer::Hotspots => (
            0.0,
            1.0,
            "",
            &[
                [172, 181, 159],
                [193, 163, 121],
                [194, 115, 78],
                [169, 62, 59],
            ],
            &[
                [172, 181, 159],
                [193, 163, 121],
                [194, 115, 78],
                [169, 62, 59],
            ],
            "Low",
            "High",
        ),
        _ => return None,
    };
    if palette == crate::display::Palette::Colorblind && layer == Layer::Temperature {
        min = -30.0;
        max = 30.0;
    }
    Some(LayerScale {
        min,
        max,
        unit,
        stops: if palette == crate::display::Palette::Colorblind {
            accessible
        } else {
            natural
        }
        .to_vec(),
        low,
        high,
    })
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Spec {
    seed: i64,
    settings: Settings,
    epoch: u64,
    #[serde(default)]
    display: DisplayPrefs,
    #[serde(default)]
    dimension: Dimension,
    #[serde(default = "default_slice_y")]
    slice_y: i32,
    #[serde(default)]
    world_path: Option<PathBuf>,
    #[serde(default)]
    source_revision: u64,
}
impl Spec {
    fn same_world(&self, other: &Self) -> bool {
        self.seed == other.seed
            && self.settings == other.settings
            && self.dimension == other.dimension
            && self.slice_y == other.slice_y
            && self.world_path == other.world_path
            && self.source_revision == other.source_revision
    }
}
fn default_slice_y() -> i32 {
    96
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
struct TileKey {
    zoom: i32,
    x: i32,
    z: i32,
    layer: Layer,
    detail: bool,
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct HeightGridKey {
    seed: i64,
    settings: [u64; 6],
    dimension: Dimension,
    slice_y: i32,
    x: i32,
    z: i32,
    zoom: i32,
    detail: bool,
    source: u64,
    sampling: u16,
}
impl HeightGridKey {
    fn new(world: &World, tile: TileKey) -> Self {
        let settings = &world.region.settings;
        Self {
            seed: world.seed,
            settings: [
                settings.temperature_scale.to_bits(),
                settings.temperature_constant.to_bits(),
                settings.rainfall_scale.to_bits(),
                settings.rainfall_constant.to_bits(),
                settings.continentalness.to_bits(),
                settings.grass_density.to_bits(),
            ],
            dimension: world.dimension,
            slice_y: world.slice_y,
            x: tile.x,
            z: tile.z,
            zoom: tile.zoom,
            detail: tile.detail,
            source: world.surface_source_id(),
            sampling: 0,
        }
    }

    fn with_terrain(mut self, detail: &TerrainDetailOptions) -> Self {
        self.sampling = [
            detail.physical_surface,
            detail.caves,
            detail.rivers_lakes,
            detail.volcanoes,
            detail.springs,
            detail.exposed_dikes,
            detail.large_rocks_special_soils,
            detail.precise_biome_boundaries,
        ]
        .into_iter()
        .enumerate()
        .fold(0, |bits, (i, on)| bits | ((on as u16) << i));
        self
    }

    fn with_biomes(mut self, precise: bool) -> Self {
        self.sampling = precise as u16;
        self
    }
}
type HeightGrid = Arc<Vec<Option<f64>>>;
/// Completed immutable grids are shared across engine workers. Only cache lookup
/// and publication hold this lock; terrain generation and cancellation never do.
struct RasterCache<V> {
    capacity: usize,
    entries: OnceLock<Mutex<LruCache<HeightGridKey, V>>>,
}
impl<V> RasterCache<V> {
    const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: OnceLock::new(),
        }
    }
    fn access<R>(&self, action: impl FnOnce(&mut LruCache<HeightGridKey, V>) -> R) -> R {
        let entries = self.entries.get_or_init(|| {
            Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(self.capacity).unwrap(),
            ))
        });
        action(
            &mut entries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}
// Global limits replace four separate copies of every worker's caches. 32 full
// height grids retain the previous four-worker budget while layers can reuse any
// worker's result. Source identity and revision remain in every key.
static HEIGHT_GRID_CACHE: RasterCache<HeightGrid> = RasterCache::new(32);
// Share finished physical relief across worker threads and layer switches.
// Material relief includes saved substrate/boulders; ordinary relief does not.
static TERRAIN_RELIEF_CACHE: RasterCache<HeightGrid> = RasterCache::new(16);
static MATERIAL_RELIEF_CACHE: RasterCache<HeightGrid> = RasterCache::new(16);

static WATER_MASK_CACHE: RasterCache<Arc<Vec<bool>>> = RasterCache::new(96);
static VOLCANIC_SURFACE_CACHE: RasterCache<Arc<Vec<Option<crate::volcanic_lava::Surface>>>> =
    RasterCache::new(96);
static BIOME_GRID_CACHE: RasterCache<Arc<Vec<u16>>> = RasterCache::new(96);
impl TileKey {
    fn span(self) -> f64 {
        TILE_POINTS * 2f64.powi(self.zoom)
    }
    fn origin(self) -> Point {
        let s = self.span();
        Point {
            x: self.x as f64 * s,
            z: self.z as f64 * s,
        }
    }
}
/// Keep close detail rasters at one stored sample per world block. Below one block per
/// screen point, the atlas cannot reveal finer-than-block world data, so reusing the zoom-zero
/// raster avoids repeated samples of the same block at fractional world coordinates.
fn tile_zoom_for_bpp(bpp: f64) -> i32 {
    if bpp <= 2.0 {
        0
    } else {
        bpp.log2().round() as i32
    }
}

fn tile_zoom_for_detail(bpp: f64, detail: MapDetail) -> i32 {
    tile_zoom_for_bpp(bpp).max(detail.min_tile_zoom())
}
#[derive(serde::Serialize, serde::Deserialize)]
struct CpuTile {
    key: TileKey,
    resolution: usize,
    #[serde(skip)]
    pixels: Vec<Color32>,
    #[serde(skip)]
    biomes: Vec<u16>,
    /// Surface water classification for the interior semantic raster. Kept separate
    /// from biome IDs so flooded land remains identifiable in legends.
    #[serde(skip)]
    surface_water: Vec<bool>,
}
struct GpuTile {
    texture: TextureHandle,
    /// egui updates sampler options with an image delta. Retain one gutter pixel
    /// so changing the filter needs only an identical 1x1 upload, not a new map.
    corner: Color32,
    resolution: usize,
    biomes: Vec<u16>,
    surface_water: Vec<bool>,
}
#[derive(serde::Serialize, serde::Deserialize)]
enum Job {
    Tile(Spec, TileKey, u64),
    Probe(Spec, Point, bool, u64),
    Trees(Spec, Vec<Point>, f64, u64),
    Features(Spec, FeatureTileKey, u64),
    InspectStructure(Spec, Option<PathBuf>, Marker, u64),
    Surface(
        Spec,
        Option<PathBuf>,
        Vec<(i32, i32)>,
        u64,
        #[serde(default)] Option<StructureBounds>,
    ),
    Search(Spec, Search),
    Analyze(Spec, analysis::Query, u64),
    Locate(Spec, analysis::LocationQuery, u64),
    Export(Spec, exports::Request, u64),
}
#[derive(serde::Serialize, serde::Deserialize)]
enum Reply {
    Tile(u64, u64, CpuTile),
    Skipped(u64, u64, TileKey),
    Probe(u64, u64, Probe),
    Trees(u64, u64, TreeEstimate),
    Features(u64, u64, FeatureTileKey, Vec<Marker>, bool),
    StructurePreview(
        u64,
        u64,
        Result<crate::structure_preview::StructurePreview, String>,
    ),
    Surface(
        u64,
        u64,
        Vec<(i32, i32, Result<Option<surface::SurfaceData>, String>)>,
    ),
    Search(u64, u64, Vec<Marker>, bool, String),
    Structures(u64, Result<prism::StructureScan, String>),
    EngineError(String),
    Worlds(Result<Vec<crate::model::WorldInfo>, String>),
    OpenedSource(u64, Result<crate::world_files::OpenedSource, String>),
    Analysis(u64, u64, Result<analysis::Report, String>),
    Locate(u64, u64, Result<analysis::LocationResults, String>),
    Export(u64, u64, Result<exports::Outcome, String>),
}

/// A structure instance retains its inspection across map pans, zoom and layer changes.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PreviewKey {
    id: String,
    x: u64,
    z: u64,
    saved: bool,
}
impl PreviewKey {
    fn new(marker: &Marker) -> Self {
        Self {
            id: marker.id.clone(),
            x: marker.x.to_bits(),
            z: marker.z.to_bits(),
            saved: marker.confirmed,
        }
    }
}

struct RegionCache {
    revision: u64,
    view: (i32, i64, i64, i32, i32, Layer),
    cols: usize,
    rows: usize,
    cell: f64,
    values: Vec<Option<u16>>,
    biome_labels: Vec<u32>,
    continent_labels: Vec<u32>,
    biome_sizes: HashMap<u32, usize>,
    continent_sizes: HashMap<u32, usize>,
    biome_edges: HashSet<u32>,
    continent_edges: HashSet<u32>,
    overlays: HashMap<(bool, u32), TextureHandle>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Search {
    origin: Point,
    radius: f64,
    query: String,
    predictions: bool,
    scope: crate::search::Scope,
    serial: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
struct FeatureTileKey {
    x: i64,
    z: i64,
    tier: u8,
    scale: u8,
    ores: bool,
    structures: bool,
}

/// Candidate data survives zoom changes. The scan tier records how complete this
/// spatial result is; a smaller tier means the scan included more feature families.
struct FeatureTileData {
    markers: Vec<Marker>,
    scan_tier: u8,
}

impl FeatureTileKey {
    fn at(point: Point, tier: u8, ores: bool, structures: bool) -> Self {
        let span = Self::span_for(tier);
        Self {
            x: (point.x / span).floor() as i64,
            z: (point.z / span).floor() as i64,
            tier,
            scale: 0,
            ores,
            structures,
        }
    }

    fn bounds(self) -> Bounds {
        let span = Self::span_for(self.tier) / 2f64.powi(self.scale as i32);
        Bounds {
            min_x: self.x as f64 * span,
            min_z: self.z as f64 * span,
            max_x: (self.x + 1) as f64 * span,
            max_z: (self.z + 1) as f64 * span,
        }
    }

    fn center(self) -> Point {
        let span = Self::span_for(self.tier) / 2f64.powi(self.scale as i32);
        Point {
            x: (self.x as f64 + 0.5) * span,
            z: (self.z as f64 + 0.5) * span,
        }
    }

    fn span_for(tier: u8) -> f64 {
        features::lod_max_blocks_per_point(tier) * 128.
    }

    fn children(self) -> [Self; 4] {
        let scale = self.scale.saturating_add(1);
        [
            Self {
                x: self.x * 2,
                z: self.z * 2,
                scale,
                ..self
            },
            Self {
                x: self.x * 2 + 1,
                z: self.z * 2,
                scale,
                ..self
            },
            Self {
                x: self.x * 2,
                z: self.z * 2 + 1,
                scale,
                ..self
            },
            Self {
                x: self.x * 2 + 1,
                z: self.z * 2 + 1,
                scale,
                ..self
            },
        ]
    }

    #[cfg(test)]
    fn root(self) -> Self {
        let divisor = 1_i64.checked_shl(self.scale as u32).unwrap_or(i64::MAX);
        Self {
            x: self.x.div_euclid(divisor),
            z: self.z.div_euclid(divisor),
            scale: 0,
            ..self
        }
    }
}

fn visible_feature_tiles(
    bounds: Bounds,
    center: Point,
    tier: u8,
    ores: bool,
    structures: bool,
) -> Vec<FeatureTileKey> {
    let min = FeatureTileKey::at(
        Point {
            x: bounds.min_x,
            z: bounds.min_z,
        },
        tier,
        ores,
        structures,
    );
    let max = FeatureTileKey::at(
        Point {
            x: bounds.max_x,
            z: bounds.max_z,
        },
        tier,
        ores,
        structures,
    );
    let columns = (max.x as i128 - min.x as i128 + 1).max(0) as u128;
    let rows = (max.z as i128 - min.z as i128 + 1).max(0) as u128;
    let mut wanted = Vec::with_capacity(columns.saturating_mul(rows).min(65_536) as usize);
    for z in min.z..=max.z {
        for x in min.x..=max.x {
            wanted.push(FeatureTileKey {
                x,
                z,
                tier,
                scale: 0,
                ores,
                structures,
            });
        }
    }
    wanted.sort_unstable_by(|a, b| {
        a.center()
            .distance(center)
            .total_cmp(&b.center().distance(center))
            .then_with(|| a.z.cmp(&b.z))
            .then_with(|| a.x.cmp(&b.x))
    });
    wanted
}

fn feature_scans_ready(visible_tiles: usize, ready_tiles: usize) -> bool {
    visible_tiles > 0 && ready_tiles > 0 && ready_tiles >= visible_tiles.saturating_add(3) / 4
}

#[cfg(test)]
fn next_missing_feature_tile(
    key: FeatureTileKey,
    splits: &HashSet<FeatureTileKey>,
    cached: &HashMap<FeatureTileKey, Vec<Marker>>,
    center: Point,
) -> Option<FeatureTileKey> {
    if splits.contains(&key) {
        let mut children = key.children();
        children.sort_unstable_by(|a, b| {
            a.center()
                .distance(center)
                .total_cmp(&b.center().distance(center))
        });
        children
            .into_iter()
            .find_map(|child| next_missing_feature_tile(child, splits, cached, center))
    } else if cached.contains_key(&key) {
        None
    } else {
        Some(key)
    }
}

fn region_covered_at_tier(
    requested: FeatureTileKey,
    cached: &HashMap<FeatureTileKey, FeatureTileData>,
) -> bool {
    let bounds = requested.bounds();
    cached.iter().any(|(key, data)| {
        data.scan_tier <= requested.tier
            && (!requested.ores || key.ores)
            && (!requested.structures || key.structures)
            && key.bounds().min_x <= bounds.min_x
            && key.bounds().max_x >= bounds.max_x
            && key.bounds().min_z <= bounds.min_z
            && key.bounds().max_z >= bounds.max_z
    })
}

fn next_uncovered_feature_tile(
    key: FeatureTileKey,
    splits: &HashSet<FeatureTileKey>,
    cached: &HashMap<FeatureTileKey, FeatureTileData>,
    center: Point,
) -> Option<FeatureTileKey> {
    if region_covered_at_tier(key, cached) {
        return None;
    }
    let bounds = key.bounds();
    let span = (bounds.max_x - bounds.min_x).max(bounds.max_z - bounds.min_z);
    let has_finer_coverage = cached.iter().any(|(candidate, data)| {
        if data.scan_tier > key.tier
            || (key.ores && !candidate.ores)
            || (key.structures && !candidate.structures)
        {
            return false;
        }
        let other = candidate.bounds();
        other.max_x > bounds.min_x
            && other.min_x < bounds.max_x
            && other.max_z > bounds.min_z
            && other.min_z < bounds.max_z
    });
    // A hard physical leaf limit prevents malformed/edge coverage from recursing
    // indefinitely. Between this and the adaptive scan limit, only descend when
    // cached tiles genuinely overlap and may jointly cover this region.
    if span <= MIN_FEATURE_COVERAGE_BLOCKS {
        return Some(key);
    }
    if span <= MIN_FEATURE_TILE_BLOCKS && !has_finer_coverage {
        return Some(key);
    }
    // Keep the previous adaptive scan size for cold regions. Subdivide only after
    // a scan was truncated or when cached coverage genuinely overlaps this region.
    if !splits.contains(&key) && !has_finer_coverage {
        return Some(key);
    }
    let mut children = key.children();
    children.sort_unstable_by(|a, b| {
        a.center()
            .distance(center)
            .total_cmp(&b.center().distance(center))
    });
    // Recursing even without an explicit truncation split lets fine scans already
    // cached from a previous zoom cover a larger coarse tile.
    children
        .into_iter()
        .find_map(|child| next_uncovered_feature_tile(child, splits, cached, center))
}

/// Spatial buckets keep repeated save/prediction deduplication and map queries local.
/// Indices refer to the immutable marker slice used to build this snapshot.
#[derive(Default)]
struct MarkerIndex {
    cells: HashMap<(i64, i64), Vec<usize>>,
    structure_start_cells: HashMap<(i64, i64), Vec<usize>>,
    structure_bounds_cells: HashMap<(i64, i64), Vec<usize>>,
    oversized_structure_bounds: Vec<usize>,
}

impl MarkerIndex {
    fn build(markers: &[Marker]) -> Self {
        let mut index = Self::default();
        for (i, marker) in markers.iter().enumerate() {
            if marker.x.is_finite() && marker.z.is_finite() {
                index
                    .cells
                    .entry(Self::cell(marker.map_point()))
                    .or_default()
                    .push(i);
                if marker.kind == MarkerKind::Structure {
                    index
                        .structure_start_cells
                        .entry(Self::cell(marker.point()))
                        .or_default()
                        .push(i);
                }
            }
            // The old footprint query did not depend on the saved structure-start
            // coordinates when a complete bounding box was present.
            if marker.kind == MarkerKind::Structure
                && let Some(bounds) = marker.bounds
            {
                let (min_x, min_z) = Self::cell(Point {
                    x: bounds.min_x as f64,
                    z: bounds.min_z as f64,
                });
                let (max_x, max_z) = Self::cell(Point {
                    x: bounds.max_x as f64 + 1.,
                    z: bounds.max_z as f64 + 1.,
                });
                let columns = (max_x as i128 - min_x as i128 + 1).max(0) as u128;
                let rows = (max_z as i128 - min_z as i128 + 1).max(0) as u128;
                if columns.saturating_mul(rows) > 64 {
                    index.oversized_structure_bounds.push(i);
                } else {
                    for x in min_x..=max_x {
                        for z in min_z..=max_z {
                            index
                                .structure_bounds_cells
                                .entry((x, z))
                                .or_default()
                                .push(i);
                        }
                    }
                }
            }
        }
        index
    }

    fn cell(point: Point) -> (i64, i64) {
        (
            (point.x / MARKER_CELL).floor() as i64,
            (point.z / MARKER_CELL).floor() as i64,
        )
    }

    fn has_structure_near(&self, markers: &[Marker], marker: &Marker) -> bool {
        let (cx, cz) = Self::cell(marker.point());
        for x in cx - 1..=cx + 1 {
            for z in cz - 1..=cz + 1 {
                if let Some(indices) = self.structure_start_cells.get(&(x, z))
                    && indices.iter().any(|&i| {
                        let saved = &markers[i];
                        saved.namespace == marker.namespace
                            && saved.point().distance(marker.point()) < 32.
                    })
                {
                    return true;
                }
            }
        }
        false
    }

    fn structures_intersecting(&self, bounds: Bounds) -> Vec<usize> {
        if bounds.min_x > bounds.max_x
            || bounds.min_z > bounds.max_z
            || !bounds.min_x.is_finite()
            || !bounds.max_x.is_finite()
            || !bounds.min_z.is_finite()
            || !bounds.max_z.is_finite()
        {
            return Vec::new();
        }
        let (min_x, min_z) = Self::cell(Point {
            x: bounds.min_x,
            z: bounds.min_z,
        });
        let (max_x, max_z) = Self::cell(Point {
            x: bounds.max_x,
            z: bounds.max_z,
        });
        let columns = (max_x as i128 - min_x as i128 + 1).max(0) as u128;
        let rows = (max_z as i128 - min_z as i128 + 1).max(0) as u128;
        let mut indices = self.oversized_structure_bounds.clone();
        if columns.saturating_mul(rows) <= 32_768 {
            for x in min_x..=max_x {
                for z in min_z..=max_z {
                    if let Some(found) = self.structure_bounds_cells.get(&(x, z)) {
                        indices.extend_from_slice(found);
                    }
                }
            }
        } else {
            for (&(x, z), found) in &self.structure_bounds_cells {
                if x >= min_x && x <= max_x && z >= min_z && z <= max_z {
                    indices.extend_from_slice(found);
                }
            }
        }
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    fn visible<'a>(
        &self,
        markers: &'a [Marker],
        bounds: Bounds,
        ores: bool,
        structures: bool,
    ) -> Vec<&'a Marker> {
        if bounds.min_x > bounds.max_x
            || bounds.min_z > bounds.max_z
            || !bounds.min_x.is_finite()
            || !bounds.max_x.is_finite()
            || !bounds.min_z.is_finite()
            || !bounds.max_z.is_finite()
        {
            return Vec::new();
        }
        let (min_x, min_z) = Self::cell(Point {
            x: bounds.min_x,
            z: bounds.min_z,
        });
        let (max_x, max_z) = Self::cell(Point {
            x: bounds.max_x,
            z: bounds.max_z,
        });
        let columns = (max_x as i128 - min_x as i128 + 1).clamp(0, u64::MAX as i128) as u64;
        let rows = (max_z as i128 - min_z as i128 + 1).clamp(0, u64::MAX as i128) as u64;
        let mut visible = Vec::new();
        let mut push_cell = |cell: (i64, i64)| {
            if let Some(indices) = self.cells.get(&cell) {
                for &i in indices {
                    let marker = &markers[i];
                    let shown = match &marker.kind {
                        MarkerKind::Ore => ores,
                        MarkerKind::Structure | MarkerKind::Volcano => structures,
                        MarkerKind::Spawn | MarkerKind::Player => true,
                        _ => false,
                    };
                    let point = marker.map_point();
                    if shown && bounds.contains(point.x, point.z) {
                        visible.push((i, marker));
                    }
                }
            }
        };
        // Normal map views enumerate only intersecting cells. For very wide views, walk
        // occupied cells instead of looping over millions of empty world cells.
        if columns.saturating_mul(rows) <= 32_768 {
            for x in min_x..=max_x {
                for z in min_z..=max_z {
                    push_cell((x, z));
                }
            }
        } else {
            for &(x, z) in self.cells.keys() {
                if x >= min_x && x <= max_x && z >= min_z && z <= max_z {
                    push_cell((x, z));
                }
            }
        }
        visible.sort_unstable_by_key(|(i, _)| *i);
        visible.into_iter().map(|(_, marker)| marker).collect()
    }
}

pub struct Atlas {
    ctx: egui::Context,
    spec: Spec,
    epoch: Arc<AtomicU64>,
    view: Arc<AtomicU64>,
    engine: engine::Client,
    engine_error: Option<String>,
    save_path: Option<PathBuf>,
    surface: surface::SurfaceView,
    structure_previews:
        LruCache<PreviewKey, Result<crate::structure_preview::StructurePreview, String>>,
    preview_pending: Option<(PreviewKey, u64)>,
    preview_serial: u64,
    preview_revision: u64,
    #[cfg(test)]
    out: Sender<Reply>,
    replies: Receiver<Reply>,
    discovered_worlds: Option<Result<Vec<crate::model::WorldInfo>, String>>,
    open_source_pending: Option<u64>,
    opened_source: Option<(u64, Result<crate::world_files::OpenedSource, String>)>,
    tiles: LruCache<TileKey, GpuTile>,
    pending: HashMap<TileKey, (u64, u64)>,
    // Avoid rebuilding and serializing the identical plan on every egui frame.
    last_tile_plan: Option<(u64, HashSet<TileKey>, Vec<(TileKey, u64)>)>,
    uploads: VecDeque<CpuTile>,
    markers: Vec<Marker>,
    predicted: Vec<Marker>,
    saved: Vec<Marker>,
    world_locations: Vec<crate::model::WorldLocation>,
    saved_index: MarkerIndex,
    completed_structure_chunks: HashMap<(i32, i32), prism::ChunkCoverage>,
    marker_index: MarkerIndex,
    ores: bool,
    structures: bool,
    ore_filter: String,
    feature_tiles: HashMap<FeatureTileKey, FeatureTileData>,
    feature_splits: HashSet<FeatureTileKey>,
    feature_truncated: HashSet<FeatureTileKey>,
    feature_lru: VecDeque<FeatureTileKey>,
    feature_visible_keys: Vec<FeatureTileKey>,
    feature_plan_key: Option<(u8, i64, i64, i64, i64, bool, bool)>,
    feature_serial: u64,
    feature_pending: bool,
    feature_markers_dirty: bool,
    hover_point: Option<Point>,
    hover_requested: Option<Point>,
    hover_since: Instant,
    probe: Option<Probe>,
    destination_probe: Option<Probe>,
    destination_request: Option<(Point, u64)>,
    probe_serial: u64,
    trees: Option<TreeEstimate>,
    probe_height: bool,
    tree_sampling: bool,
    tree_serial: u64,
    regions: bool,
    region_overlay: Option<(TextureHandle, Rect)>,
    region_cache: Option<RegionCache>,
    selection_summary: Option<(f64, bool)>,
    semantic_revision: u64,
    semantic_changed_at: Instant,
    view_changed_at: Instant,
    selection_key: Option<(u64, u32, bool)>,
    tree_requested: Option<(u64, u32, bool)>,
    view_key: Option<(i32, i64, i64, i32, i32, Layer)>,
    bounds: Bounds,
    rect: Rect,
    center: Point,
    bpp: f64,
    legend_snapshot: LegendSnapshot,
    legend_updated_at: Option<Instant>,
    legend_signature: Option<(u64, Layer, Dimension, [u64; 5], u64)>,
    tile_revision: u64,
    status: String,
    scan_status: String,
    scan_epoch: u64,
    loading_scan: bool,
    search: Option<Search>,
    search_results: Vec<Marker>,
    search_candidates: Vec<Marker>,
    search_status: String,
    search_pending: bool,
    search_limited: bool,
    search_note: String,
    search_revision: Arc<AtomicU64>,
    has_save: bool,
    feature_limited: bool,
    task_serial: u64,
    task_busy: bool,
    task_status: String,
    completed_export: Option<Result<exports::Outcome, String>>,
    analysis_report: Option<analysis::Report>,
    locations: Option<analysis::LocationResults>,
}

/// Snapshot of raster coverage for the current viewport, useful to distinguish completed
/// refinement from a merely idle worker queue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileStreamProgress {
    pub visible: usize,
    pub detailed: usize,
    pub fallback_ready: usize,
    pub holes: usize,
    pub pending: usize,
    pub uploads: usize,
}
impl Atlas {
    pub fn new(ctx: &egui::Context, seed: i64, settings: Settings) -> Self {
        let engine = engine::Client::new(ctx);
        let replies = engine.replies.clone();
        let epoch = Arc::new(AtomicU64::new(1));
        let view = Arc::new(AtomicU64::new(1));
        let search_revision = Arc::new(AtomicU64::new(0));
        Self {
            ctx: ctx.clone(),
            spec: Spec {
                seed,
                settings,
                epoch: 1,
                display: DisplayPrefs::default(),
                dimension: Dimension::Overworld,
                slice_y: default_slice_y(),
                world_path: None,
                source_revision: 0,
            },
            epoch,
            view,
            #[cfg(test)]
            out: engine.test_out.clone(),
            engine,
            engine_error: None,
            save_path: None,
            surface: surface::SurfaceView::default(),
            structure_previews: LruCache::new(std::num::NonZeroUsize::new(16).unwrap()),
            preview_pending: None,
            preview_serial: 0,
            preview_revision: 0,
            replies,
            discovered_worlds: None,
            open_source_pending: None,
            opened_source: None,
            tiles: LruCache::new(std::num::NonZeroUsize::new(TILE_CACHE).unwrap()),
            pending: HashMap::new(),
            last_tile_plan: None,
            uploads: VecDeque::new(),
            markers: vec![],
            predicted: vec![],
            saved: vec![],
            world_locations: vec![],
            saved_index: MarkerIndex::default(),
            completed_structure_chunks: HashMap::new(),
            marker_index: MarkerIndex::default(),
            ores: true,
            structures: true,
            ore_filter: String::new(),
            feature_tiles: HashMap::new(),
            feature_splits: HashSet::new(),
            feature_truncated: HashSet::new(),
            feature_lru: VecDeque::new(),
            feature_visible_keys: Vec::new(),
            feature_plan_key: None,
            feature_serial: 0,
            feature_pending: false,
            feature_markers_dirty: false,
            hover_point: None,
            hover_requested: None,
            hover_since: Instant::now(),
            probe: None,
            destination_probe: None,
            destination_request: None,
            probe_serial: 0,
            trees: None,
            probe_height: false,
            tree_sampling: true,
            tree_serial: 0,
            regions: true,
            region_overlay: None,
            region_cache: None,
            selection_summary: None,
            semantic_revision: 0,
            semantic_changed_at: Instant::now(),
            view_changed_at: Instant::now(),
            selection_key: None,
            tree_requested: None,
            view_key: None,
            bounds: Bounds::default(),
            rect: Rect::NOTHING,
            center: Point::default(),
            bpp: 16.,
            legend_snapshot: LegendSnapshot::default(),
            legend_updated_at: None,
            legend_signature: None,
            tile_revision: 0,
            status: "Preparing atlas…".into(),
            scan_status: String::new(),
            scan_epoch: 0,
            loading_scan: false,
            search: None,
            search_results: vec![],
            search_candidates: vec![],
            search_status: "Choose a point and search the world.".into(),
            search_pending: false,
            search_limited: false,
            search_note: String::new(),
            search_revision,
            has_save: false,
            feature_limited: false,
            task_serial: 0,
            task_busy: false,
            task_status: String::new(),
            completed_export: None,
            analysis_report: None,
            locations: None,
        }
    }
    pub fn discover_worlds(&self, instance: PathBuf) {
        self.engine.discover(instance);
    }
    pub fn take_worlds(&mut self) -> Option<Result<Vec<crate::model::WorldInfo>, String>> {
        self.poll();
        self.discovered_worlds.take()
    }
    pub fn set_world(&mut self, seed: i64, settings: Settings, save: Option<PathBuf>) {
        self.world_locations.clear();
        self.reset_world(seed, settings, save, false);
    }

    /// Set source metadata locations independently from the saved structure scan.
    /// They survive dimension and vertical-slice changes.
    pub fn set_world_locations(&mut self, locations: Vec<crate::model::WorldLocation>) {
        self.world_locations = locations
            .into_iter()
            .filter(|location| {
                matches!(location.marker.kind, MarkerKind::Spawn | MarkerKind::Player)
            })
            .collect();
        self.rebuild_markers();
        self.update_search(false);
    }
    fn reset_world(
        &mut self,
        seed: i64,
        settings: Settings,
        save: Option<PathBuf>,
        preserve_saved: bool,
    ) {
        self.preview_pending = None;
        if !preserve_saved {
            self.structure_previews.clear();
            self.preview_revision = self.preview_revision.wrapping_add(1);
        }
        self.surface.invalidate(true);
        self.surface
            .set_dimension(self.spec.dimension, self.spec.slice_y);
        self.save_path = save.clone();
        self.surface.set_options(
            self.surface.enabled(),
            save.as_deref()
                .map(|path| self.spec.dimension.storage(path)),
        );
        self.spec = Spec {
            seed,
            settings,
            epoch: self.epoch.fetch_add(1, Ordering::Relaxed) + 1,
            display: self.spec.display.clone(),
            dimension: self.spec.dimension,
            slice_y: self.spec.slice_y,
            world_path: save.clone(),
            source_revision: if preserve_saved {
                self.spec.source_revision
            } else {
                self.spec.source_revision.wrapping_add(1)
            },
        };
        self.surface.set_season(
            self.spec.display.simulated_season(),
            self.spec.settings.temperature_scale,
        );
        self.view.fetch_add(1, Ordering::Relaxed);

        self.tiles.clear();
        self.pending.clear();
        self.last_tile_plan = None;
        self.uploads.clear();
        self.markers.clear();
        // A Beneath slice changes the terrain raster, not seed-derived placement positions.
        // Keep completed feature data so its pins remain present while the new band loads.
        if !preserve_saved {
            self.predicted.clear();
            self.saved.clear();
            self.saved_index = MarkerIndex::default();
            self.completed_structure_chunks.clear();
            self.scan_epoch = self.spec.epoch;
        }
        self.marker_index = MarkerIndex::default();
        if !preserve_saved {
            self.feature_tiles.clear();
            self.feature_splits.clear();
            self.feature_truncated.clear();
            self.feature_lru.clear();
            self.feature_visible_keys.clear();
            self.feature_limited = false;
        }
        self.feature_plan_key = None;
        self.feature_pending = false;
        self.probe = None;
        self.destination_probe = None;
        self.destination_request = None;
        self.hover_requested = None;
        self.trees = None;
        self.region_overlay = None;
        self.region_cache = None;
        self.selection_summary = None;
        self.selection_key = None;
        self.tree_requested = None;
        self.view_key = None;
        self.search_results.clear();
        self.search_candidates.clear();
        self.search = None;
        self.search_status.clear();
        self.search_note.clear();
        self.search_pending = false;
        self.has_save = save.is_some();
        if !preserve_saved {
            self.loading_scan = save.is_some();
        }
        self.search_limited = false;
        self.cancel_tasks();
        self.analysis_report = None;
        self.locations = None;
        let serial = self.search_revision.fetch_add(1, Ordering::Relaxed) + 1;
        self.engine.reset(self.spec.epoch, serial);
        if !preserve_saved {
            self.scan_status = if save.is_some() {
                "Reading saved structures…".into()
            } else {
                "Seed preview".into()
            };
            if let Some(save) = save {
                self.engine
                    .scan(self.scan_epoch, self.spec.dimension.storage(&save));
            }
        }
        self.rebuild_markers();
    }
    pub fn dimension(&self) -> Dimension {
        self.spec.dimension
    }
    pub fn slice_y(&self) -> i32 {
        self.spec.slice_y
    }
    pub fn set_dimension(&mut self, dimension: Dimension, slice_y: i32) {
        let slice_y = if dimension == Dimension::Beneath {
            dimension.band(slice_y).1
        } else {
            dimension.default_y()
        };
        if self.spec.dimension == dimension && self.spec.slice_y == slice_y {
            return;
        }
        let same_dimension = self.spec.dimension == dimension;
        self.spec.dimension = dimension;
        self.spec.slice_y = slice_y;
        self.reset_world(
            self.spec.seed,
            self.spec.settings.clone(),
            self.save_path.clone(),
            same_dimension,
        );
    }
    pub fn engine_error(&self) -> Option<&str> {
        self.engine_error.as_deref()
    }
    /// Resolve a saved-world source in the engine process, keeping filesystem work off the UI.
    pub fn open_source(&mut self, path: PathBuf, serial: u64) {
        if let Some(error) = &self.engine_error {
            self.open_source_pending = None;
            self.opened_source = Some((serial, Err(error.clone())));
            return;
        }
        self.open_source_pending = Some(serial);
        self.opened_source = None;
        self.engine.open_source(path, serial);
    }

    /// Retrieve the latest completed world import, keyed by its caller-provided serial.
    pub fn take_opened_source(
        &mut self,
    ) -> Option<(u64, Result<crate::world_files::OpenedSource, String>)> {
        self.poll();
        self.opened_source.take()
    }
    pub fn set_surface_options(&mut self, enabled: bool, path: Option<PathBuf>) {
        self.surface.set_options(
            enabled,
            path.as_deref().map(|p| self.spec.dimension.storage(p)),
        );
    }
    /// Limit saved surface detail to the selected structure's recorded bounds.
    /// This affects only the detail overlay and does not reset world generation.
    pub fn set_structure_inspection(&mut self, bounds: Option<StructureBounds>) {
        self.surface.set_structure_inspection(bounds);
    }
    /// Queue one on-demand inspection in the engine. No filesystem reads or template
    /// assembly happen on the event thread. The same instance is reused at every zoom.
    pub fn inspect_structure(&mut self, marker: &Marker) {
        self.poll();
        if marker.kind != MarkerKind::Structure {
            return;
        }
        let key = PreviewKey::new(marker);
        if self.structure_previews.get(&key).is_some()
            || self
                .preview_pending
                .as_ref()
                .is_some_and(|(pending, _)| *pending == key)
        {
            return;
        }
        if let Some(error) = &self.engine_error {
            self.structure_previews.put(key, Err(error.clone()));
            return;
        }
        self.preview_serial = self.preview_serial.wrapping_add(1);
        let path = self
            .save_path
            .as_deref()
            .map(|p| self.spec.dimension.storage(p));
        if self.engine.high(Job::InspectStructure(
            self.spec.clone(),
            path,
            marker.clone(),
            self.preview_serial,
        )) {
            self.preview_pending = Some((key, self.preview_serial));
        }
    }
    pub fn structure_preview(
        &self,
        marker: &Marker,
    ) -> Option<&crate::structure_preview::StructurePreview> {
        self.structure_previews
            .peek(&PreviewKey::new(marker))
            .and_then(|r| r.as_ref().ok())
    }
    pub fn structure_preview_error(&self, marker: &Marker) -> Option<&str> {
        self.structure_previews
            .peek(&PreviewKey::new(marker))
            .and_then(|r| r.as_ref().err())
            .map(String::as_str)
    }
    pub fn structure_preview_pending(&self, marker: &Marker) -> bool {
        self.preview_pending
            .as_ref()
            .is_some_and(|(key, _)| *key == PreviewKey::new(marker))
    }
    pub fn structure_preview_revision(&self) -> u64 {
        self.preview_revision
    }
    pub fn reload_structure_preview(&mut self, marker: &Marker) {
        let key = PreviewKey::new(marker);
        self.structure_previews.pop(&key);
        if self
            .preview_pending
            .as_ref()
            .is_some_and(|(pending, _)| *pending == key)
        {
            self.preview_pending = None;
        }
        self.inspect_structure(marker);
    }
    pub fn surface_probe(&mut self, point: Point) -> Option<&prism::SurfaceColumn> {
        if !self.surface_visible(self.view_key.is_some_and(|key| key.5 == Layer::Biomes)) {
            return None;
        }
        self.surface.probe(point)
    }
    pub fn surface_source(&mut self, point: Point) -> Option<surface::SurfaceSource> {
        if !self.surface_visible(self.view_key.is_some_and(|key| key.5 == Layer::Biomes)) {
            return None;
        }
        self.surface.source(point)
    }
    pub fn surface_status(&self) -> &str {
        self.surface.status()
    }
    fn surface_visible(&self, biomes: bool) -> bool {
        let display = &self.spec.display;
        let opaque_biomes = display.layer_stack.as_ref().is_none_or(|_| {
            let layers = display.active_layers(Layer::Biomes, self.spec.dimension);
            let mut visible = layers
                .iter()
                .filter(|blend| blend.opacity.is_finite() && blend.opacity > 0.);
            matches!((visible.next(), visible.next()),
                (Some(blend), None) if blend.layer == Layer::Biomes && blend.opacity >= 1.)
        });
        let earth = self.spec.dimension == Dimension::Overworld;
        let terrain_style = self.spec.dimension != Dimension::Beneath
            && ((display.height_shading && display.hillshade_strength > 0.)
                || display.contours.is_active());
        let highlights = !display.highlighted_biomes.is_empty()
            || (earth
                && (!display.highlighted_rocks.is_empty()
                    || display.temperature_enabled
                    || display.rainfall_enabled));
        let grass = earth && display.grass_colors && display.grass_opacity > 0.;
        let trees = earth && display.trees && display.tree_opacity > 0.;
        self.spec.display.terrain.physical_surface
            && self.spec.display.terrain.caves
            && self.spec.display.terrain.rivers_lakes
            && self.spec.display.terrain.volcanoes
            && self.spec.display.terrain.springs
            && self.spec.display.terrain.exposed_dikes
            && self.spec.display.terrain.large_rocks_special_soils
            && self.spec.display.terrain.precise_biome_boundaries
            && self.spec.display.max_detail == MapDetail::Block
            && opaque_biomes
            && !grass
            && !trees
            && display.palette == crate::display::Palette::Natural
            && !terrain_style
            && !highlights
            && self.surface.visible_at(biomes, self.bpp)
    }
    pub fn restart_engine(&mut self) {
        self.engine = engine::Client::new(&self.ctx);
        self.replies = self.engine.replies.clone();
        #[cfg(test)]
        {
            self.out = self.engine.test_out.clone();
        }
        self.engine_error = None;
        self.set_world(
            self.spec.seed,
            self.spec.settings.clone(),
            self.save_path.clone(),
        );
    }
    /// Separate generation process, exposed for diagnostics.
    pub fn engine_pid(&self) -> Option<u32> {
        self.engine.pid()
    }
    pub fn set_marker_options(&mut self, ores: bool, structures: bool, ore_filter: &str) {
        let changed = self.ores != ores || self.structures != structures;
        if changed {
            self.feature_serial = self.feature_serial.wrapping_add(1);
            self.feature_pending = false;
        }
        self.ores = ores;
        self.structures = structures;
        if changed || self.ore_filter != ore_filter {
            self.ore_filter = ore_filter.to_owned();
            self.rebuild_markers();
        }
    }
    pub fn set_probe_options(&mut self, height: bool, trees: bool) {
        let height = height && self.spec.dimension != Dimension::Beneath;
        let trees = trees && self.spec.dimension == Dimension::Overworld;
        if self.probe_height != height || self.tree_sampling != trees {
            self.probe_height = height;
            self.tree_sampling = trees;
            self.probe = None;
            self.trees = None;
            self.tree_requested = None;
            self.selection_key = None;
            self.tree_serial = self.tree_serial.wrapping_add(1);
            self.hover_requested = None;
            self.probe_serial = self.probe_serial.wrapping_add(1);
            self.hover_since = Instant::now() - Duration::from_millis(200);
        }
    }
    pub fn selection_summary(&self) -> Option<(f64, bool)> {
        self.selection_summary
    }
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }
    pub fn visible_markers(&self, bounds: Bounds, ores: bool, structures: bool) -> Vec<&Marker> {
        let mut visible = self
            .marker_index
            .visible(&self.markers, bounds, ores, structures);
        if structures {
            for index in self.marker_index.structures_intersecting(bounds) {
                let marker = &self.markers[index];
                let point = marker.map_point();
                if marker.kind != MarkerKind::Structure || bounds.contains(point.x, point.z) {
                    continue;
                }
                if marker.bounds.is_some_and(|b| {
                    b.min_x as f64 <= bounds.max_x
                        && b.max_x as f64 + 1. >= bounds.min_x
                        && b.min_z as f64 <= bounds.max_z
                        && b.max_z as f64 + 1. >= bounds.min_z
                }) {
                    visible.push(marker);
                }
            }
        }
        visible
    }
    pub fn saved_structures(&self) -> &[Marker] {
        &self.saved
    }
    /// A completed chunk with intact start metadata is stronger evidence than a
    /// placement candidate. Unknown and incomplete chunks never supply a veto.
    pub fn structure_candidate_absent(&self, marker: &Marker) -> bool {
        if marker.kind != MarkerKind::Structure
            || marker.confirmed
            || !marker.x.is_finite()
            || !marker.z.is_finite()
        {
            return false;
        }
        let cx = (marker.x.floor() as i32).div_euclid(16);
        let cz = (marker.z.floor() as i32).div_euclid(16);
        self.completed_structure_chunks
            .get(&(cx.div_euclid(32), cz.div_euclid(32)))
            .is_some_and(|coverage| coverage.contains(cx, cz))
    }
    pub fn probe(&self) -> Option<&Probe> {
        self.probe.as_ref()
    }
    /// Actions need the destination block, not the nearby probe tolerated by
    /// hover display at distant zoom levels. This never samples on the UI thread.
    pub fn probe_at(&self, point: Point) -> Option<&Probe> {
        self.destination_probe
            .as_ref()
            .filter(|probe| {
                point.x.is_finite()
                    && point.z.is_finite()
                    && probe.point.x.floor() == point.x.floor()
                    && probe.point.z.floor() == point.z.floor()
            })
            .or_else(|| {
                self.probe.as_ref().filter(|probe| {
                    point.x.is_finite()
                        && point.z.is_finite()
                        && probe.point.x.floor() == point.x.floor()
                        && probe.point.z.floor() == point.z.floor()
                })
            })
    }
    /// Resolve a context-menu destination in the engine, even while the pointer
    /// moves off the map. Planetary copies also need height with its tooltip off.
    pub fn request_destination_probe(&mut self, point: Point) {
        if !point.x.is_finite() || !point.z.is_finite() || self.engine_error.is_some() {
            return;
        }
        if self.probe_at(point).is_some_and(|probe| {
            probe.height.is_some()
                || probe.ground_y.is_some()
                || probe.surface_water_y.is_some()
                || probe.surface_lava.is_some()
        }) || self.destination_request.is_some_and(|(pending, _)| {
            pending.x.floor() == point.x.floor() && pending.z.floor() == point.z.floor()
        }) {
            return;
        }
        self.probe_serial = self.probe_serial.wrapping_add(1);
        if self.engine.high(Job::Probe(
            self.spec.clone(),
            point,
            true,
            self.probe_serial,
        )) {
            self.destination_request = Some((point, self.probe_serial));
        }
    }
    pub fn tree_estimate(&self) -> Option<&TreeEstimate> {
        self.trees.as_ref()
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    /// Return a cached viewport biome histogram. It samples at most 4096 points and
    /// reads only raster tiles already resident in the UI cache. Refreshes are throttled.
    pub fn legend_snapshot(&mut self, layer: Layer) -> &LegendSnapshot {
        const SIDE: usize = 64;
        const REFRESH: Duration = Duration::from_millis(200);
        let signature = (
            self.spec.epoch,
            layer,
            self.spec.dimension,
            [
                self.bounds.min_x.to_bits(),
                self.bounds.min_z.to_bits(),
                self.bounds.max_x.to_bits(),
                self.bounds.max_z.to_bits(),
                self.bpp.to_bits(),
            ],
            self.tile_revision,
        );
        let invalidated = self.legend_signature != Some(signature);
        let context_changed = self.legend_signature.as_ref().is_none_or(|previous| {
            previous.0 != signature.0 || previous.1 != signature.1 || previous.2 != signature.2
        });
        let due = self
            .legend_updated_at
            .is_none_or(|updated| context_changed || updated.elapsed() >= REFRESH && invalidated);
        if invalidated && !due {
            self.ctx.request_repaint_after(
                REFRESH.saturating_sub(self.legend_updated_at.unwrap().elapsed()),
            );
        }
        if due {
            let bounds = self.bounds;
            let dimension = self.spec.dimension;
            let zoom = tile_zoom_for_detail(self.bpp, self.spec.display.max_detail);
            let mut counts = HashMap::<u16, u32>::new();
            let sampled = SIDE * SIDE;
            let mut loaded = 0usize;
            if (layer == Layer::Biomes || dimension == Dimension::Beneath)
                && bounds.max_x > bounds.min_x
                && bounds.max_z > bounds.min_z
            {
                for iz in 0..SIDE {
                    let z = bounds.min_z
                        + (iz as f64 + 0.5) / SIDE as f64 * (bounds.max_z - bounds.min_z);
                    for ix in 0..SIDE {
                        let x = bounds.min_x
                            + (ix as f64 + 0.5) / SIDE as f64 * (bounds.max_x - bounds.min_x);
                        if let Some(id) = self.biome_at_cached(Point { x, z }, zoom, layer) {
                            loaded += 1;
                            *counts.entry(id).or_default() += 1;
                        }
                    }
                }
            }
            let ids: Box<dyn Iterator<Item = u16>> = if dimension.is_planetary() {
                Box::new(crate::planetary::biome_ids(dimension).into_iter())
            } else if dimension == Dimension::Beneath {
                Box::new(1000..1011)
            } else {
                Box::new(0..biome::ids::BIOME_IDS.len() as u16)
            };
            let entries = if layer == Layer::Biomes || dimension == Dimension::Beneath {
                let mut entries: Vec<_> = ids
                    .map(|id| LegendEntry {
                        id,
                        name: biome::biome_name(id).to_owned(),
                        color: biome_legend_color(id, dimension, &self.spec.display),
                        count: counts.get(&id).copied().unwrap_or(0),
                    })
                    .collect();
                entries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
                entries
            } else {
                Vec::new()
            };
            self.legend_snapshot = LegendSnapshot {
                entries,
                sampled,
                loaded,
                layer,
                dimension,
            };
            self.legend_updated_at = Some(Instant::now());
            self.legend_signature = Some(signature);
        }
        &self.legend_snapshot
    }
    pub fn analyze(&mut self, query: analysis::Query) -> bool {
        self.begin_task("Sampling area…");
        self.analysis_report = None;
        let serial = self.task_serial;
        let sent = self
            .engine
            .low(Job::Analyze(self.spec.clone(), query, serial));
        if !sent {
            self.finish_task("Generation queue is busy; retry in a moment.");
        }
        sent
    }
    pub fn locate(&mut self, query: analysis::LocationQuery) -> bool {
        self.begin_task("Searching nearby terrain…");
        self.locations = None;
        let serial = self.task_serial;
        let sent = self
            .engine
            .low(Job::Locate(self.spec.clone(), query, serial));
        if !sent {
            self.finish_task("Generation queue is busy; retry in a moment.");
        }
        sent
    }
    pub fn export(&mut self, request: exports::Request) -> bool {
        self.begin_task("Preparing export…");
        let serial = self.task_serial;
        let sent = self
            .engine
            .low(Job::Export(self.spec.clone(), request, serial));
        if !sent {
            self.finish_task("Generation queue is busy; retry in a moment.");
        }
        sent
    }
    fn begin_task(&mut self, status: &str) {
        self.task_serial = self.task_serial.wrapping_add(1);
        self.engine.task_revision(self.task_serial);
        self.task_busy = true;
        self.task_status = status.to_owned();
    }
    fn finish_task(&mut self, status: &str) {
        self.task_busy = false;
        self.task_status = status.to_owned();
    }
    pub fn cancel_tasks(&mut self) {
        self.task_serial = self.task_serial.wrapping_add(1);
        self.engine.task_revision(self.task_serial);
        if self.task_busy {
            self.finish_task("Cancelled");
        }
    }
    pub fn analysis_report(&self) -> Option<&analysis::Report> {
        self.analysis_report.as_ref()
    }
    pub fn locations(&self) -> Option<&analysis::LocationResults> {
        self.locations.as_ref()
    }
    pub fn take_export(&mut self) -> Option<Result<exports::Outcome, String>> {
        self.completed_export.take()
    }
    pub fn task_status(&self) -> &str {
        &self.task_status
    }
    pub fn tasks_busy(&self) -> bool {
        self.task_busy
    }
    pub fn set_display_options(&mut self, prefs: &DisplayPrefs) {
        self.surface.set_season(
            prefs.simulated_season(),
            self.spec.settings.temperature_scale,
        );
        if self.spec.display.same_raster(prefs) {
            if self.spec.display.loading_filter != prefs.loading_filter {
                for (key, tile) in self.tiles.iter_mut() {
                    tile.texture.set_partial(
                        [0, 0],
                        ColorImage::new([1, 1], vec![tile.corner]),
                        tile_texture_options(*key, prefs.loading_filter),
                    );
                }
            }
            if self.spec.display.max_detail != prefs.max_detail {
                self.view_key = None;
                self.legend_signature = None;
                self.region_cache = None;
                self.region_overlay = None;
            }
            self.spec.display = prefs.clone();
            self.ctx.request_repaint();
            return;
        }
        let mut previous_without_calendar = self.spec.display.clone();
        previous_without_calendar.season = prefs.season;
        let calendar_only = previous_without_calendar == *prefs;
        let pending_search = self.search_pending.then(|| self.search.clone()).flatten();
        self.cancel_tasks();
        self.spec.display = prefs.clone();
        self.spec.epoch = self.epoch.fetch_add(1, Ordering::Relaxed) + 1;
        self.surface.invalidate(false);
        self.view.fetch_add(1, Ordering::Relaxed);
        self.tiles.clear();
        self.pending.clear();
        self.last_tile_plan = None;
        self.uploads.clear();
        self.view_key = None;
        // Complete feature scans depend on the world, not raster/style detail.
        // Pending jobs are canceled by epoch; completed structure/ore tiles stay reusable.
        self.feature_plan_key = None;
        self.feature_pending = false;
        if !calendar_only {
            self.probe = None;
            self.destination_probe = None;
        }
        self.destination_request = None;
        self.trees = None;
        self.tree_serial = self.tree_serial.wrapping_add(1);
        self.probe_serial = self.probe_serial.wrapping_add(1);
        self.hover_requested = None;
        self.selection_key = None;
        self.tree_requested = None;
        self.region_overlay = None;
        self.region_cache = None;
        self.selection_key = None;
        self.tree_requested = None;
        let serial = self.search_revision.load(Ordering::Relaxed);
        self.engine.reset(self.spec.epoch, serial);
        if let Some(search) = pending_search {
            self.search_pending = self.engine.low(Job::Search(self.spec.clone(), search));
        }
    }
    pub fn search_results(&self) -> &[Marker] {
        &self.search_results
    }
    pub fn search_status(&self) -> &str {
        &self.search_status
    }
    pub fn is_searching(&self) -> bool {
        self.search_pending || self.loading_scan
    }
    pub fn is_loading(&self) -> bool {
        // Border prefetch should not keep a fully refined visible map in a loading state.
        self.pending.keys().any(|key| self.tile_visible(*key))
            || self.uploads.iter().any(|tile| self.tile_visible(tile.key))
            || self.feature_pending
            || self.surface_visible(self.view_key.is_some_and(|key| key.5 == Layer::Biomes))
                && self.surface.is_pending()
            || self.loading_scan
            || self.search_pending
    }

    /// Reports exact-detail and parent/child fallback coverage for the current raster view.
    pub fn tile_stream_progress(&self) -> TileStreamProgress {
        let Some((_, _, _, _, _, layer)) = self.view_key else {
            return TileStreamProgress::default();
        };
        let zoom = tile_zoom_for_detail(self.bpp, self.spec.display.max_detail);
        let span = TILE_POINTS * 2f64.powi(zoom);
        let min_x = (self.bounds.min_x / span).floor() as i32;
        let max_x = (self.bounds.max_x / span).floor() as i32;
        let min_z = (self.bounds.min_z / span).floor() as i32;
        let max_z = (self.bounds.max_z / span).floor() as i32;
        let mut progress = TileStreamProgress::default();
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let key = TileKey {
                    zoom,
                    x,
                    z,
                    layer,
                    detail: true,
                };
                progress.visible += 1;
                progress.detailed += usize::from(self.tiles.contains(&key));
                if self.tile_has_coverage(key, 0) {
                    progress.fallback_ready += 1;
                } else {
                    progress.holes += 1;
                }
                progress.pending += usize::from(
                    self.pending.contains_key(&key)
                        || self.pending.contains_key(&TileKey {
                            detail: false,
                            ..key
                        }),
                );
                progress.uploads += self.uploads.iter().any(|tile| {
                    tile.key == key
                        || tile.key
                            == TileKey {
                                detail: false,
                                ..key
                            }
                }) as usize;
            }
        }
        progress
    }

    fn tile_has_coverage(&self, key: TileKey, depth: u8) -> bool {
        if self.tiles.contains(&key)
            || self.tiles.contains(&TileKey {
                detail: false,
                ..key
            })
        {
            return true;
        }
        for level in 1..=8 {
            let divisor = 1_i32 << level;
            let parent = TileKey {
                zoom: key.zoom + level,
                x: key.x.div_euclid(divisor),
                z: key.z.div_euclid(divisor),
                ..key
            };
            if self.tiles.contains(&parent)
                || self.tiles.contains(&TileKey {
                    detail: false,
                    ..parent
                })
            {
                return true;
            }
        }
        if depth >= 2 || key.zoom <= self.spec.display.max_detail.min_tile_zoom() {
            return false;
        }
        let child_zoom = key.zoom - 1;
        [
            (key.x * 2, key.z * 2),
            (key.x * 2 + 1, key.z * 2),
            (key.x * 2, key.z * 2 + 1),
            (key.x * 2 + 1, key.z * 2 + 1),
        ]
        .into_iter()
        .all(|(x, z)| {
            self.tile_has_coverage(
                TileKey {
                    zoom: child_zoom,
                    x,
                    z,
                    ..key
                },
                depth + 1,
            )
        })
    }
    fn tile_visible(&self, key: TileKey) -> bool {
        let Some(view) = self.view_key else {
            return false;
        };
        tile_intersects_view(
            key,
            self.bounds,
            tile_zoom_for_detail(self.bpp, self.spec.display.max_detail),
            view.5,
        )
    }
    pub fn hover(&mut self, point: Option<Point>, regions: bool) {
        self.regions = regions;
        let moved = match (self.hover_point, point) {
            (Some(a), Some(b)) => a.distance(b) > self.bpp * 1.5,
            (None, None) => false,
            _ => true,
        };
        if moved {
            self.hover_point = point;
            self.hover_since = Instant::now();
            self.probe = None;
            self.hover_requested = None;
        }
        if point.is_none() || !regions {
            self.region_overlay = None;
            self.selection_key = None;
            self.region_cache = None;
            self.selection_summary = None;
            self.tree_requested = None;
            self.tree_serial = self.tree_serial.wrapping_add(1);
            self.trees = None;
        }
    }
    pub fn search(&mut self, origin: Point, radius: f64, query: String, predictions: bool) {
        self.find(
            origin,
            radius,
            query,
            predictions,
            crate::search::Scope::Structures,
        );
    }
    pub fn find(
        &mut self,
        origin: Point,
        radius: f64,
        query: String,
        predictions: bool,
        scope: crate::search::Scope,
    ) {
        let serial = self.search_revision.fetch_add(1, Ordering::Relaxed) + 1;
        self.engine.search_revision(serial);
        if !origin.x.is_finite()
            || !origin.z.is_finite()
            || !radius.is_finite()
            || radius <= 0.
            || origin.x.abs() > 30_000_000.
            || origin.z.abs() > 30_000_000.
        {
            self.search = None;
            self.search_results.clear();
            self.search_candidates.clear();
            self.search_pending = false;
            self.search_status =
                "Enter coordinates within the world border and a positive radius.".into();
            return;
        }
        let search = Search {
            origin,
            radius: radius.clamp(1., 100_000.),
            query,
            predictions,
            scope,
            serial,
        };
        let needs_worker = scope != crate::search::Scope::Structures
            && (scope != crate::search::Scope::All || !search.query.trim().is_empty())
            || predictions;
        self.search = Some(search.clone());
        self.search_candidates.clear();
        self.search_limited = false;
        self.search_note.clear();
        self.search_pending = needs_worker;
        self.update_search(false);
        if needs_worker {
            if self.engine.low(Job::Search(self.spec.clone(), search)) {
                self.search_status.push_str(" · searching generation…");
            } else {
                self.search_pending = false;
                self.search_status.push_str(" · map is busy, retry shortly");
            }
        }
    }
    fn update_search(&mut self, truncated: bool) {
        self.search_limited |= truncated;
        let truncated = self.search_limited;
        let Some(search) = &self.search else {
            return;
        };
        let q = search.query.to_lowercase();
        let saved: Vec<_> = self
            .saved
            .iter()
            .filter(|m| search.scope.accepts(m) && crate::search::matches_query(m, &q))
            .cloned()
            .collect();
        let candidates: Vec<_> = self
            .search_candidates
            .iter()
            .filter(|m| search.scope.accepts(m) && crate::search::matches_query(m, &q))
            .filter(|m| {
                m.kind != MarkerKind::Structure
                    || !self.saved_index.has_structure_near(&self.saved, m)
            })
            .filter(|m| !self.structure_candidate_absent(m))
            .cloned()
            .collect();
        let locations = self
            .world_locations
            .iter()
            .filter(|location| location.dimension == self.spec.dimension.id())
            .map(|location| &location.marker)
            .filter(|marker| {
                search.scope.accepts(marker) && crate::search::matches_query(marker, &q)
            })
            .cloned()
            .collect::<Vec<_>>();
        self.search_results = crate::search::rank(
            search.origin,
            search.radius,
            saved.into_iter().chain(candidates).chain(locations),
            4096,
        );
        self.search_status = format!(
            "{} results · within {:0.1} km{}{}",
            self.search_results.len(),
            search.radius / 1000.,
            if self.loading_scan {
                " · save index loading"
            } else if self.has_save {
                " · saved world indexed"
            } else {
                " · seed preview"
            },
            if truncated {
                " · candidate search limited"
            } else {
                ""
            }
        );
        if !self.search_note.is_empty() {
            self.search_status.push_str(" · ");
            self.search_status.push_str(&self.search_note);
        }
    }
    fn rebuild_markers(&mut self) {
        self.markers = self
            .saved
            .iter()
            .filter(|_| self.structures)
            .cloned()
            .collect();
        self.markers.extend(
            self.world_locations
                .iter()
                .filter(|location| location.dimension == self.spec.dimension.id())
                .map(|location| location.marker.clone()),
        );
        for m in &self.predicted {
            let visible = match m.kind {
                MarkerKind::Ore => self.ores && query_matches(m, &self.ore_filter.to_lowercase()),
                MarkerKind::Structure | MarkerKind::Volcano => self.structures,
                _ => false,
            };
            if visible
                && !self.structure_candidate_absent(m)
                && !(m.kind == MarkerKind::Structure
                    && self.saved_index.has_structure_near(&self.saved, m))
            {
                self.markers.push(m.clone());
            }
        }
        for marker in &mut self.markers {
            if marker.kind == MarkerKind::Structure
                && marker.bounds.is_none()
                && let Some(Ok(preview)) = self.structure_previews.peek(&PreviewKey::new(marker))
                && preview.complete
                && preview.source != crate::structure_preview::PreviewSource::TemplateLayout
            {
                marker.bounds = preview.bounds;
            }
        }
        self.marker_index = MarkerIndex::build(&self.markers);
    }

    fn refresh_visible_features(&mut self) {
        let mut seen = HashSet::new();
        let mut markers = Vec::new();
        let mut incomplete = false;
        for (&key, data) in &self.feature_tiles {
            incomplete |= self.feature_truncated.contains(&key) && self.feature_key_is_visible(key);
            markers.extend(
                data.markers
                    .iter()
                    .filter(|marker| seen.insert(marker.id.clone()))
                    .cloned(),
            );
        }
        self.predicted = markers;
        self.feature_limited = incomplete;
        self.rebuild_markers();
        self.feature_markers_dirty = false;
    }

    fn feature_key_is_visible(&self, key: FeatureTileKey) -> bool {
        let tile = key.bounds();
        self.feature_visible_keys.iter().any(|root| {
            let bounds = root.bounds();
            tile.max_x >= bounds.min_x
                && tile.min_x <= bounds.max_x
                && tile.max_z >= bounds.min_z
                && tile.min_z <= bounds.max_z
        })
    }

    fn cache_feature_result(&mut self, key: FeatureTileKey, markers: Vec<Marker>, truncated: bool) {
        if truncated && key.bounds().max_x - key.bounds().min_x > MIN_FEATURE_TILE_BLOCKS {
            self.feature_splits.insert(key);
            self.feature_tiles.remove(&key);
            self.feature_lru.retain(|old| *old != key);
            self.feature_truncated.remove(&key);
        } else {
            let entry = self
                .feature_tiles
                .entry(key)
                .or_insert_with(|| FeatureTileData {
                    markers: Vec::new(),
                    scan_tier: key.tier,
                });
            let mut known = entry
                .markers
                .iter()
                .map(|marker| marker.id.clone())
                .collect::<HashSet<_>>();
            entry.markers.extend(
                markers
                    .into_iter()
                    .filter(|marker| known.insert(marker.id.clone())),
            );
            entry.scan_tier = entry.scan_tier.min(key.tier);
            self.feature_lru.retain(|old| *old != key);
            self.feature_lru.push_back(key);
            if truncated {
                self.feature_truncated.insert(key);
            } else {
                self.feature_splits.remove(&key);
                self.feature_truncated.remove(&key);
            }
        }
        let mut offscreen = self
            .feature_tiles
            .keys()
            .filter(|candidate| !self.feature_key_is_visible(**candidate))
            .count();
        let attempts = self.feature_lru.len();
        for _ in 0..attempts {
            if offscreen <= FEATURE_TILE_CACHE {
                break;
            }
            let Some(old) = self.feature_lru.pop_front() else {
                break;
            };
            if self.feature_key_is_visible(old) {
                self.feature_lru.push_back(old);
            } else if self.feature_tiles.remove(&old).is_some() {
                self.feature_splits.remove(&old);
                self.feature_truncated.remove(&old);
                offscreen -= 1;
            }
        }
        // A poll can drain several feature replies at once. Defer the expensive
        // all-cache merge and marker-index rebuild until that batch is accepted.
        self.feature_markers_dirty = true;
    }

    fn update_feature_stream(&mut self, settled: bool, coarse_ready: bool) {
        let tier = features::lod_tier(self.bpp);
        if self.engine_error.is_some()
            || (!self.ores && !self.structures)
            || features::lod_max_blocks_per_point(tier) <= 0.0
        {
            if !self.feature_visible_keys.is_empty() || !self.predicted.is_empty() {
                self.feature_visible_keys.clear();
                self.feature_plan_key = None;
                self.predicted.clear();
                self.rebuild_markers();
            }
            if self.feature_pending {
                self.feature_serial = self.feature_serial.wrapping_add(1);
                self.feature_pending = false;
            }
            return;
        }

        let min = FeatureTileKey::at(
            Point {
                x: self.bounds.min_x,
                z: self.bounds.min_z,
            },
            tier,
            self.ores,
            self.structures,
        );
        let max = FeatureTileKey::at(
            Point {
                x: self.bounds.max_x,
                z: self.bounds.max_z,
            },
            tier,
            self.ores,
            self.structures,
        );
        let plan_key = (tier, min.x, min.z, max.x, max.z, self.ores, self.structures);
        if self.feature_plan_key != Some(plan_key) {
            self.feature_plan_key = Some(plan_key);
            let wanted =
                visible_feature_tiles(self.bounds, self.center, tier, self.ores, self.structures);
            self.feature_visible_keys = wanted;
            self.refresh_visible_features();
        }
        if !settled || !coarse_ready || self.feature_pending {
            return;
        }
        let Some(key) = self.feature_visible_keys.iter().find_map(|root| {
            next_uncovered_feature_tile(
                *root,
                &self.feature_splits,
                &self.feature_tiles,
                self.center,
            )
        }) else {
            return;
        };
        self.feature_serial = self.feature_serial.wrapping_add(1);
        let serial = self.feature_serial;
        self.feature_pending = self
            .engine
            .low(Job::Features(self.spec.clone(), key, serial));
    }

    pub fn paint(&mut self, ui: &mut Ui, rect: Rect, center: Point, bpp: f64, layer: Layer) {
        self.rect = rect;
        self.center = center;
        self.bpp = bpp.clamp(0.125, 4096.);
        let half_w = rect.width() as f64 * self.bpp / 2.;
        let half_h = rect.height() as f64 * self.bpp / 2.;
        self.bounds = Bounds {
            min_x: center.x - half_w,
            min_z: center.z - half_h,
            max_x: center.x + half_w,
            max_z: center.z + half_h,
        };
        // Close detail tiles keep one texel per world block through two screen points per
        // block. Farther out, nearest LOD keeps tile size near 256 screen points.
        let zoom = tile_zoom_for_detail(self.bpp, self.spec.display.max_detail);
        let span = TILE_POINTS * 2f64.powi(zoom);
        let key = (
            (self.bpp.log2() * 1_000_000.).round() as i32,
            (center.x / self.bpp / 4.) as i64,
            (center.z / self.bpp / 4.) as i64,
            rect.width() as i32,
            rect.height() as i32,
            layer,
        );
        if self.view_key != Some(key) {
            self.view_key = Some(key);
            self.view.fetch_add(1, Ordering::Relaxed);
            self.view_changed_at = Instant::now();
            self.region_overlay = None;
            self.selection_key = None;
            self.region_cache = None;
            self.selection_summary = None;
            self.tree_requested = None;
            self.tree_serial = self.tree_serial.wrapping_add(1);
            self.trees = None;
        }
        self.poll();
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0., Color32::from_rgb(20, 42, 58));
        let min_x = (self.bounds.min_x / span).floor() as i32;
        let max_x = (self.bounds.max_x / span).floor() as i32;
        let min_z = (self.bounds.min_z / span).floor() as i32;
        let max_z = (self.bounds.max_z / span).floor() as i32;
        let mut visible = Vec::new();
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let key = TileKey {
                    zoom,
                    x,
                    z,
                    layer,
                    detail: true,
                };
                let o = key.origin();
                let dest = self.world_rect(o, span);
                if let Some(tile) = self.tiles.get(&key) {
                    painter.image(
                        tile.texture.id(),
                        dest,
                        tile_uv(tile.resolution),
                        Color32::WHITE,
                    );
                } else {
                    self.paint_parent(ui, rect, dest, key);
                    if let Some(tile) = self.tiles.get(&TileKey {
                        detail: false,
                        ..key
                    }) {
                        painter.image(
                            tile.texture.id(),
                            dest,
                            tile_uv(tile.resolution),
                            Color32::WHITE,
                        );
                    }
                    self.paint_children(ui, rect, key);
                }
                let distance = (o.x + span / 2. - center.x).hypot(o.z + span / 2. - center.z);
                visible.push((
                    distance,
                    TileKey {
                        detail: false,
                        ..key
                    },
                    key,
                ));
            }
        }
        if self.surface_visible(layer == Layer::Biomes) {
            self.surface.paint(ui, rect, self.bounds, self.bpp);
            if self.engine_error.is_none()
                && let Some((path, chunks, serial, structure_bounds)) =
                    self.surface.take_request(self.bounds, center, self.bpp)
            {
                let coords = chunks.iter().map(|c| (c.0, c.1)).collect();
                if !self.engine.low(Job::Surface(
                    self.spec.clone(),
                    path,
                    coords,
                    serial,
                    structure_bounds,
                )) {
                    self.surface.mark_missing(serial, &chunks);
                }
            }
        }
        prioritize_visible(&mut visible);
        // A coarse tile at one edge cannot hold up refinement in the rest of the map.
        let mut requested = refinement_requests(&visible, |key| self.tiles.contains(key));
        let mut wanted = HashSet::new();
        for (_, coarse, fine) in &visible {
            wanted.extend([*coarse, *fine]);
        }
        // Warm a one-tile border only after visible work, ready for the next short pan.
        let mut border = Vec::new();
        for z in min_z - 1..=max_z + 1 {
            for x in min_x - 1..=max_x + 1 {
                if (min_x..=max_x).contains(&x) && (min_z..=max_z).contains(&z) {
                    continue;
                }
                let coarse = TileKey {
                    zoom,
                    x,
                    z,
                    layer,
                    detail: false,
                };
                let fine = TileKey {
                    detail: true,
                    ..coarse
                };
                wanted.insert(coarse);
                if !self.tiles.contains(&coarse) && !self.tiles.contains(&fine) {
                    let o = coarse.origin();
                    border.push((
                        (o.x + span / 2. - center.x).hypot(o.z + span / 2. - center.z),
                        coarse,
                    ));
                }
            }
        }
        border.sort_by(|a, b| a.0.total_cmp(&b.0));
        requested.extend(border.into_iter().map(|(_, key)| key));
        let capacity = (visible.len() * 2 + 64).clamp(TILE_CACHE, 1024);
        if self.tiles.cap().get() != capacity {
            self.tiles
                .resize(std::num::NonZeroUsize::new(capacity).unwrap());
        }
        self.schedule_tiles(wanted, requested);
        let feature_ready_count = visible
            .iter()
            .filter(|(_, c, f)| self.tiles.contains(c) || self.tiles.contains(f))
            .count();
        let feature_ready = feature_scans_ready(visible.len(), feature_ready_count);
        let settled = self.view_changed_at.elapsed() >= Duration::from_millis(160);
        if self.regions
            && self.hover_point.is_some()
            && settled
            && (self.pending.is_empty()
                || self.semantic_changed_at.elapsed() >= Duration::from_millis(120))
        {
            self.paint_region(ui, layer, zoom);
        }
        self.request_probe();
        self.update_feature_stream(settled, feature_ready);
        let ready = visible
            .iter()
            .filter(|(_, c, f)| self.tiles.contains(f) || self.tiles.contains(c))
            .count();
        let refined = visible
            .iter()
            .filter(|(_, _, f)| self.tiles.contains(f))
            .count();
        self.status = if self.engine_error.is_some() {
            "Generation paused".into()
        } else if ready < visible.len() {
            format!("Drawing map · {ready}/{}", visible.len())
        } else if refined < visible.len() {
            format!("Refining map · {refined}/{}", visible.len())
        } else {
            String::new()
        };
        if !self.scan_status.is_empty() {
            if !self.status.is_empty() {
                self.status.push_str("  ·  ");
            }
            self.status.push_str(&self.scan_status);
        }
        if self.feature_pending {
            self.status.push_str("  ·  locating features");
        } else if self.ores {
            self.status.push_str(if self.feature_limited {
                "  ·  feature scan limited in visible area"
            } else {
                "  ·  locating visible features"
            });
        }
        if self.surface_visible(layer == Layer::Biomes) {
            if !self.status.is_empty() {
                self.status.push_str("  ·  ");
            }
            self.status.push_str(self.surface.status());
        }
        if self.is_loading() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        } else if !settled || self.hover_point.is_some() && self.hover_requested.is_none() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
    fn schedule_tiles(&mut self, wanted: HashSet<TileKey>, requested: Vec<TileKey>) -> bool {
        let epoch = self.spec.epoch;
        let revision = self.view.load(Ordering::Relaxed);
        self.pending
            .retain(|key, (e, _)| *e == epoch && wanted.contains(key));
        let mut task_keys = Vec::with_capacity(requested.len());
        for key in requested {
            if self.uploads.iter().any(|t| t.key == key) {
                continue;
            }
            let pair = *self.pending.entry(key).or_insert((epoch, revision));
            task_keys.push((key, pair.1));
        }
        if self.engine_error.is_none() {
            if self
                .last_tile_plan
                .as_ref()
                .is_some_and(|last| last.0 == epoch && last.1 == wanted && last.2 == task_keys)
            {
                return false;
            }
            let tasks = task_keys
                .iter()
                .map(|&(key, revision)| TileTask {
                    spec: self.spec.clone(),
                    key,
                    revision,
                })
                .collect();
            self.last_tile_plan = Some((epoch, wanted.clone(), task_keys));
            self.engine.plan(epoch, wanted, tasks);
            true
        } else {
            self.pending.clear();
            self.last_tile_plan = None;
            false
        }
    }
    fn request_probe(&mut self) {
        if self.engine_error.is_some() || self.destination_request.is_some() {
            return;
        }
        let Some(p) = self.hover_point else {
            return;
        };
        if self.hover_since.elapsed() < Duration::from_millis(110) || self.hover_requested.is_some()
        {
            return;
        }
        self.probe_serial += 1;
        if self.engine.high(Job::Probe(
            self.spec.clone(),
            p,
            self.probe_height,
            self.probe_serial,
        )) {
            self.hover_requested = Some(p);
        }
    }
    fn poll(&mut self) {
        let delivery_start = Instant::now();
        for _ in 0..12 {
            let Ok(reply) = self.replies.try_recv() else {
                break;
            };
            match reply {
                Reply::Tile(epoch, _revision, tile) => {
                    self.engine.complete(epoch, tile.key);
                    if epoch == self.spec.epoch {
                        // Tile content is independent of the viewport that requested it.
                        self.pending.remove(&tile.key);
                        self.uploads.push_back(tile);
                        // The upload queue is capped below; an accepted tile can be
                        // discarded there when a burst exceeds capacity. Reopening the
                        // plan lets any tile still missing from the GPU cache be retried.
                        self.last_tile_plan = None;
                    }
                }
                Reply::Skipped(epoch, revision, key) => {
                    self.engine.complete(epoch, key);
                    remove_pending_if_current(&mut self.pending, key, epoch, revision);
                    // The queue may have discarded this active job's replacement
                    // while its old revision was still running. Force the unchanged
                    // viewport to publish that tile again on its next schedule pass.
                    if epoch == self.spec.epoch {
                        self.last_tile_plan = None;
                    }
                }
                Reply::Probe(epoch, serial, probe) => {
                    if epoch == self.spec.epoch
                        && self
                            .destination_request
                            .is_some_and(|(_, pending)| pending == serial)
                    {
                        self.destination_probe = Some(probe.clone());
                        self.destination_request = None;
                    }
                    if epoch == self.spec.epoch
                        && serial == self.probe_serial
                        && self
                            .hover_point
                            .is_some_and(|p| p.distance(probe.point) < self.bpp * 2.)
                    {
                        self.probe = Some(probe);
                    }
                }
                Reply::Trees(epoch, serial, estimate) => {
                    if epoch == self.spec.epoch && serial == self.tree_serial {
                        self.trees = Some(estimate);
                    }
                }
                Reply::Features(epoch, serial, key, markers, truncated) => {
                    if epoch == self.spec.epoch && serial == self.feature_serial {
                        self.feature_pending = false;
                        self.cache_feature_result(key, markers, truncated);
                    }
                }
                Reply::Surface(epoch, serial, chunks) => {
                    if epoch == self.spec.epoch {
                        self.surface.receive(serial, chunks);
                    }
                }
                Reply::StructurePreview(epoch, serial, result) => {
                    if epoch == self.spec.epoch
                        && self
                            .preview_pending
                            .as_ref()
                            .is_some_and(|(_, current)| *current == serial)
                        && let Some((key, _)) = self.preview_pending.take()
                    {
                        self.structure_previews.put(key, result);
                        self.preview_revision = self.preview_revision.wrapping_add(1);
                        self.rebuild_markers();
                    }
                }
                Reply::Search(epoch, serial, markers, truncated, note) => {
                    if epoch == self.spec.epoch
                        && self.search.as_ref().is_some_and(|s| s.serial == serial)
                    {
                        self.search_pending = false;
                        self.search_candidates = markers;
                        self.search_note = note;
                        self.update_search(truncated);
                    }
                }
                Reply::Analysis(epoch, serial, result) => {
                    if epoch == self.spec.epoch && serial == self.task_serial {
                        match result {
                            Ok(report) => {
                                self.analysis_report = Some(report);
                                self.finish_task("Area analysis ready");
                            }
                            Err(error) => self.finish_task(&error),
                        }
                    }
                }
                Reply::Locate(epoch, serial, result) => {
                    if epoch == self.spec.epoch && serial == self.task_serial {
                        match result {
                            Ok(locations) => {
                                self.locations = Some(locations);
                                self.finish_task("Nearby locations ready");
                            }
                            Err(error) => self.finish_task(&error),
                        }
                    }
                }
                Reply::Export(epoch, serial, result) => {
                    if epoch == self.spec.epoch && serial == self.task_serial {
                        match &result {
                            Ok(outcome) => self.finish_task(&format!(
                                "Saved {} · {}",
                                outcome.path.display(),
                                outcome.note
                            )),
                            Err(error) => self.finish_task(error),
                        }
                        self.completed_export = Some(result);
                    }
                }
                Reply::Worlds(result) => self.discovered_worlds = Some(result),
                Reply::OpenedSource(serial, result) => {
                    if self.open_source_pending == Some(serial) {
                        self.open_source_pending = None;
                        self.opened_source = Some((serial, result));
                    }
                }
                Reply::EngineError(error) => {
                    self.surface.invalidate(false);
                    self.engine_error = Some(error.clone());
                    if let Some((key, _)) = self.preview_pending.take() {
                        self.structure_previews.put(key, Err(error.clone()));
                    }
                    self.discovered_worlds
                        .get_or_insert_with(|| Err(error.clone()));
                    self.pending.clear();
                    self.feature_pending = false;
                    self.search_pending = false;
                    self.task_busy = false;
                    self.loading_scan = false;
                    self.scan_status = format!("Generation process: {error}");
                    if let Some(serial) = self.open_source_pending.take() {
                        self.opened_source = Some((serial, Err(error.clone())));
                    }
                }
                Reply::Structures(epoch, result) => {
                    if epoch == self.scan_epoch {
                        self.loading_scan = false;
                        match result {
                            Ok(scan) => {
                                self.scan_status = format!(
                                    "{} saved structures{}",
                                    scan.markers.len(),
                                    if scan.errors.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" · {} read errors", scan.errors.len())
                                    }
                                );
                                self.saved = scan.markers;
                                self.completed_structure_chunks = scan
                                    .completed
                                    .into_iter()
                                    .map(|coverage| {
                                        ((coverage.region_x, coverage.region_z), coverage)
                                    })
                                    .collect();
                                self.saved_index = MarkerIndex::build(&self.saved);
                                self.rebuild_markers();
                                self.update_search(false);
                            }
                            Err(error) => self.scan_status = format!("Save index: {error}"),
                        }
                    }
                }
            }
            if delivery_start.elapsed() >= Duration::from_millis(2) {
                break;
            }
        }
        if self.feature_markers_dirty {
            self.refresh_visible_features();
        }
        // New visible coverage uploads before completed offscreen cache work.
        let center = self.center;
        let layer = self.view_key.map(|v| v.5);
        self.uploads.make_contiguous().sort_by(|a, b| {
            let priority = |t: &CpuTile| {
                let o = t.key.origin();
                let span = t.key.span();
                let visible = layer == Some(t.key.layer)
                    && o.x <= self.bounds.max_x
                    && o.x + span >= self.bounds.min_x
                    && o.z <= self.bounds.max_z
                    && o.z + span >= self.bounds.min_z;
                (
                    !visible,
                    t.key.detail,
                    (o.x + span / 2. - center.x).hypot(o.z + span / 2. - center.z),
                )
            };
            let a = priority(a);
            let b = priority(b);
            (a.0, a.1)
                .cmp(&(b.0, b.1))
                .then_with(|| a.2.total_cmp(&b.2))
        });
        self.uploads.truncate(TILE_CACHE);
        let start = Instant::now();
        let mut n = 0;
        while let Some(tile) = self.uploads.pop_front() {
            let fine = TileKey {
                detail: true,
                ..tile.key
            };
            if !tile.key.detail && self.tiles.contains(&fine) {
                continue;
            }
            let side = tile.resolution + 2;
            let corner = tile.pixels[0];
            let image = ColorImage::new([side, side], tile.pixels);
            let texture = self.ctx.load_texture(
                format!("tile-{:?}", tile.key),
                image,
                tile_texture_options(tile.key, self.spec.display.loading_filter),
            );
            if tile.key.detail {
                self.tiles.pop(&TileKey {
                    detail: false,
                    ..tile.key
                });
            }
            self.tiles.put(
                tile.key,
                GpuTile {
                    texture,
                    corner,
                    resolution: tile.resolution,
                    biomes: tile.biomes,
                    surface_water: tile.surface_water,
                },
            );
            self.tile_revision = self.tile_revision.wrapping_add(1);
            if self.tile_visible(tile.key) {
                self.semantic_revision += 1;
                self.semantic_changed_at = Instant::now();
            }
            n += 1;
            if n >= 1 || start.elapsed() > Duration::from_millis(2) {
                break;
            }
        }
    }
    fn world_rect(&self, origin: Point, span: f64) -> Rect {
        tile_screen_rect(self.rect, self.center, self.bpp, origin, span)
    }
    fn paint_children(&mut self, ui: &Ui, clip: Rect, key: TileKey) {
        let painter = ui.painter().with_clip_rect(clip);
        for level in 1..=2 {
            if key.zoom - level < self.spec.display.max_detail.min_tile_zoom() {
                break;
            }
            let count = 1 << level;
            for z in 0..count {
                for x in 0..count {
                    let child = TileKey {
                        zoom: key.zoom - level,
                        x: key.x * count + x,
                        z: key.z * count + z,
                        ..key
                    };
                    let dest = self.world_rect(child.origin(), child.span());
                    let coarse = TileKey {
                        detail: false,
                        ..child
                    };
                    if let Some(tile) = self.tiles.get(&child) {
                        painter.image(
                            tile.texture.id(),
                            dest,
                            tile_uv(tile.resolution),
                            Color32::WHITE,
                        );
                    } else if let Some(tile) = self.tiles.get(&coarse) {
                        painter.image(
                            tile.texture.id(),
                            dest,
                            tile_uv(tile.resolution),
                            Color32::WHITE,
                        );
                    }
                }
            }
        }
    }
    fn paint_parent(&mut self, ui: &Ui, clip: Rect, dest: Rect, key: TileKey) {
        for level in 1..=8 {
            let z = key.zoom + level;
            let x = key.x.div_euclid(1 << level);
            let y = key.z.div_euclid(1 << level);
            let parent = TileKey {
                zoom: z,
                x,
                z: y,
                ..key
            };
            for detail in [true, false] {
                let p = TileKey { detail, ..parent };
                if let Some(tile) = self.tiles.get(&p) {
                    let n = (1 << level) as f32;
                    let rx = key.x.rem_euclid(1 << level) as f32 / n;
                    let rz = key.z.rem_euclid(1 << level) as f32 / n;
                    let inner = tile_uv(tile.resolution);
                    let uv = |x, y| {
                        Pos2::new(
                            inner.min.x + x * inner.width(),
                            inner.min.y + y * inner.height(),
                        )
                    };
                    ui.painter().with_clip_rect(clip).image(
                        tile.texture.id(),
                        dest,
                        Rect::from_min_max(uv(rx, rz), uv(rx + 1. / n, rz + 1. / n)),
                        Color32::WHITE,
                    );
                    return;
                }
            }
        }
    }
    fn biome_at_cached(&self, p: Point, zoom: i32, layer: Layer) -> Option<u16> {
        self.semantic_sample_at_cached(p, zoom, layer)
            .map(|sample| sample.0)
    }
    /// Returns original biome ID and visible surface-water classification from the
    /// same tile/fallback that supplies the current semantic pixels.
    fn semantic_sample_at_cached(&self, p: Point, zoom: i32, layer: Layer) -> Option<(u16, bool)> {
        let key_at = |zoom| {
            let span = TILE_POINTS * 2f64.powi(zoom);
            TileKey {
                zoom,
                x: (p.x / span).floor() as i32,
                z: (p.z / span).floor() as i32,
                layer,
                detail: false,
            }
        };
        let value = |key: TileKey, tiles: &LruCache<TileKey, GpuTile>| {
            let tile = tiles.peek(&key)?;
            let origin = key.origin();
            let span = key.span();
            let ix = ((p.x - origin.x) / span * tile.resolution as f64)
                .floor()
                .clamp(0., (tile.resolution - 1) as f64) as usize;
            let iz = ((p.z - origin.z) / span * tile.resolution as f64)
                .floor()
                .clamp(0., (tile.resolution - 1) as f64) as usize;
            let index = iz * tile.resolution + ix;
            Some((*tile.biomes.get(index)?, *tile.surface_water.get(index)?))
        };
        let exact = key_at(zoom);
        if let Some(sample) = value(
            TileKey {
                detail: true,
                ..exact
            },
            &self.tiles,
        ) {
            return Some(sample);
        }
        // `paint_children` overlays up to two finer zooms over the coarse tile.
        for level in (1..=2).rev() {
            if zoom - level < self.spec.display.max_detail.min_tile_zoom() {
                continue;
            }
            let key = key_at(zoom - level);
            for detail in [true, false] {
                if let Some(sample) = value(TileKey { detail, ..key }, &self.tiles) {
                    return Some(sample);
                }
            }
        }
        if let Some(sample) = value(exact, &self.tiles) {
            return Some(sample);
        }
        // `paint_parent` uses the nearest available parent through eight zoom levels.
        for level in 1..=8 {
            let key = key_at(zoom + level);
            for detail in [true, false] {
                if let Some(sample) = value(TileKey { detail, ..key }, &self.tiles) {
                    return Some(sample);
                }
            }
        }
        None
    }
    fn paint_region(&mut self, ui: &Ui, layer: Layer, zoom: i32) {
        let Some(p) = self.hover_point else {
            return;
        };
        // At normal window sizes this keeps the connected-component grid near 16k
        // cells. Larger windows automatically use a coarser hover grid; the area
        // estimate below uses this same cell size, so its units remain correct.
        let (cell, cols, rows) = region_grid(self.rect.width(), self.rect.height());
        if cols == 0 || rows == 0 {
            return;
        }
        let px = ((p.x - self.bounds.min_x) / self.bpp / cell).floor() as i32;
        let pz = ((p.z - self.bounds.min_z) / self.bpp / cell).floor() as i32;
        let continent = self.spec.dimension == Dimension::Overworld && self.bpp >= 32.;
        if px < 0 || pz < 0 || px >= cols as i32 || pz >= rows as i32 {
            self.selection_summary = None;
            return;
        }
        let view = self.view_key.unwrap_or_default();
        if self
            .region_cache
            .as_ref()
            .is_none_or(|c| !semantic_cache_valid(c, self.semantic_revision, view, cols, rows))
        {
            let mut values = vec![None; cols * rows];
            for z in 0..rows {
                for x in 0..cols {
                    values[z * cols + x] = self
                        .semantic_sample_at_cached(
                            Point {
                                x: self.bounds.min_x + (x as f64 + 0.5) * cell * self.bpp,
                                z: self.bounds.min_z + (z as f64 + 0.5) * cell * self.bpp,
                            },
                            zoom,
                            layer,
                        )
                        .and_then(region_biome_value);
                }
            }
            let (biome_labels, biome_sizes, biome_edges) =
                component_labels(&values, cols, rows, false);
            let (continent_labels, continent_sizes, continent_edges) =
                component_labels(&values, cols, rows, true);
            self.region_cache = Some(RegionCache {
                revision: self.semantic_revision,
                view,
                cols,
                rows,
                cell,
                values,
                biome_labels,
                continent_labels,
                biome_sizes,
                continent_sizes,
                biome_edges,
                continent_edges,
                overlays: HashMap::new(),
            });
        }
        let cache = self.region_cache.as_mut().unwrap();
        let start = pz as usize * cols + px as usize;
        if cache.values[start].is_none_or(biome::is_ocean) {
            self.selection_summary = None;
            self.region_overlay = None;
            self.trees = None;
            self.tree_requested = None;
            self.tree_serial = self.tree_serial.wrapping_add(1);
            return;
        }
        let label = if continent {
            cache.continent_labels[start]
        } else {
            cache.biome_labels[start]
        };
        if label == 0 {
            self.selection_summary = None;
            self.trees = None;
            self.tree_requested = None;
            return;
        }
        let (size, clipped) = if continent {
            (
                cache.continent_sizes[&label],
                cache.continent_edges.contains(&label),
            )
        } else {
            (
                cache.biome_sizes[&label],
                cache.biome_edges.contains(&label),
            )
        };
        self.selection_summary = Some((
            size as f64 * (cache.cell * self.bpp).powi(2) / 1_000_000.,
            clipped,
        ));
        let key = (self.semantic_revision, label, continent);
        let selection_changed = self.selection_key != Some(key);
        if selection_changed {
            self.tree_serial = self.tree_serial.wrapping_add(1);
            self.tree_requested = None;
            self.trees = None;
        }
        if selection_changed
            || (self.tree_sampling
                && self.tree_requested != Some((self.semantic_revision, label, continent)))
        {
            self.selection_key = Some(key);
            if !cache.overlays.contains_key(&(continent, label)) {
                let labels = if continent {
                    &cache.continent_labels
                } else {
                    &cache.biome_labels
                };
                let mut pixels = vec![Color32::TRANSPARENT; cols * rows];
                for z in 0..rows {
                    for x in 0..cols {
                        let i = z * cols + x;
                        if labels[i] != label {
                            continue;
                        }
                        let edge = x == 0
                            || z == 0
                            || x + 1 == cols
                            || z + 1 == rows
                            || labels[i - 1] != label
                            || labels[i + 1] != label
                            || labels[i - cols] != label
                            || labels[i + cols] != label;
                        pixels[i] = if edge {
                            Color32::from_rgba_unmultiplied(198, 239, 229, 150)
                        } else {
                            Color32::from_rgba_unmultiplied(205, 243, 230, 22)
                        };
                    }
                }
                let texture = self.ctx.load_texture(
                    "connected-region",
                    ColorImage::new([cols, rows], pixels),
                    TextureOptions::LINEAR,
                );
                // Each texture is cols × rows pixels. Retain at most two recent
                // selections so repeated hovering cannot accumulate large GPU
                // textures for every label encountered.
                if cache.overlays.len() >= 2 {
                    cache.overlays.clear();
                }
                cache.overlays.insert((continent, label), texture);
            }
            if self.tree_sampling
                && !continent
                && self.tree_requested != Some((self.semantic_revision, label, continent))
            {
                let selected_count = (0..cols * rows)
                    .filter(|&i| {
                        (if continent {
                            cache.continent_labels[i]
                        } else {
                            cache.biome_labels[i]
                        }) == label
                    })
                    .count();
                let count = selected_count.min(32);
                let mut points = Vec::with_capacity(count);
                if selected_count != 0 {
                    let mut ordinal = 0usize;
                    for idx in 0..cols * rows {
                        let selected_label = if continent {
                            cache.continent_labels[idx]
                        } else {
                            cache.biome_labels[idx]
                        };
                        if selected_label == label {
                            if ordinal * count / selected_count
                                != ordinal.saturating_sub(1) * count / selected_count
                                || ordinal == 0
                            {
                                let x = idx % cols;
                                let z = idx / cols;
                                points.push(Point {
                                    x: self.bounds.min_x + (x as f64 + 0.5) * cell * self.bpp,
                                    z: self.bounds.min_z + (z as f64 + 0.5) * cell * self.bpp,
                                });
                            }
                            ordinal += 1;
                        }
                    }
                }
                if !points.is_empty() {
                    self.tree_serial = self.tree_serial.wrapping_add(1);
                    let area_each = size as f64 * (cell * self.bpp).powi(2) / points.len() as f64;
                    if self.engine.low(Job::Trees(
                        self.spec.clone(),
                        points,
                        area_each,
                        self.tree_serial,
                    )) {
                        self.tree_requested = Some((self.semantic_revision, label, continent));
                        self.trees = None;
                    }
                }
            }
        }
        let cache = self.region_cache.as_ref().unwrap();
        if let Some(texture) = cache.overlays.get(&(continent, label)) {
            ui.painter().image(
                texture.id(),
                self.rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
    }
}

fn semantic_cache_valid(
    cache: &RegionCache,
    revision: u64,
    view: (i32, i64, i64, i32, i32, Layer),
    cols: usize,
    rows: usize,
) -> bool {
    cache.revision == revision && cache.view == view && cache.cols == cols && cache.rows == rows
}

/// Preserve the source biome for legends, but omit every surface-water sample from
/// connected-region topology. Flooded land otherwise creates false continent bridges.
fn region_biome_value(sample: (u16, bool)) -> Option<u16> {
    (!sample.1).then_some(sample.0)
}

fn region_grid(width: f32, height: f32) -> (f64, usize, usize) {
    let width = width.max(1.) as f64;
    let height = height.max(1.) as f64;
    let mut cell = (width * height / MAX_REGION_CELLS as f64).sqrt().max(6.);
    let (mut cols, mut rows) = (
        (width / cell).ceil() as usize,
        (height / cell).ceil() as usize,
    );
    // `ceil` can push the product past the target for unusual aspect ratios.
    for _ in 0..3 {
        let count = cols.saturating_mul(rows);
        if count <= MAX_REGION_CELLS {
            break;
        }
        cell *= (count as f64 / MAX_REGION_CELLS as f64).sqrt() * 1.002;
        cols = (width / cell).ceil() as usize;
        rows = (height / cell).ceil() as usize;
    }
    (cell, cols, rows)
}

fn remove_pending_if_current(
    pending: &mut HashMap<TileKey, (u64, u64)>,
    key: TileKey,
    epoch: u64,
    revision: u64,
) {
    if pending.get(&key) == Some(&(epoch, revision)) {
        pending.remove(&key);
    }
}

fn query_matches(marker: &Marker, q: &str) -> bool {
    q.is_empty()
        || marker.label.to_lowercase().contains(q)
        || marker.id.to_lowercase().contains(q)
        || marker.namespace.to_lowercase().contains(q)
        || marker.detail.to_lowercase().contains(q)
}

fn prioritize_visible(visible: &mut [(f64, TileKey, TileKey)]) {
    visible.sort_by(|a, b| a.0.total_cmp(&b.0));
}

fn component_labels(
    values: &[Option<u16>],
    cols: usize,
    rows: usize,
    continent: bool,
) -> (Vec<u32>, HashMap<u32, usize>, HashSet<u32>) {
    let mut labels = vec![0u32; values.len()];
    let mut sizes = HashMap::new();
    let mut edges = HashSet::new();
    let mut next = 1u32;
    for start in 0..values.len() {
        let Some(target) = values[start].filter(|id| !biome::is_ocean(*id)) else {
            continue;
        };
        if labels[start] != 0 {
            continue;
        }
        let label = next;
        next = next.wrapping_add(1).max(1);
        labels[start] = label;
        let mut stack = vec![start];
        let mut size = 0;
        let mut touches_edge = false;
        while let Some(i) = stack.pop() {
            size += 1;
            let x = i % cols;
            let z = i / cols;
            touches_edge |= x == 0
                || z == 0
                || x + 1 == cols
                || z + 1 == rows
                || x.checked_sub(1)
                    .is_some_and(|nx| values[z * cols + nx].is_none())
                || (x + 1 < cols && values[z * cols + x + 1].is_none())
                || z.checked_sub(1)
                    .is_some_and(|nz| values[nz * cols + x].is_none())
                || (z + 1 < rows && values[(z + 1) * cols + x].is_none());
            for j in [
                x.checked_sub(1).map(|nx| z * cols + nx),
                (x + 1 < cols).then_some(z * cols + x + 1),
                z.checked_sub(1).map(|nz| nz * cols + x),
                (z + 1 < rows).then_some((z + 1) * cols + x),
            ]
            .into_iter()
            .flatten()
            {
                if labels[j] != 0 {
                    continue;
                }
                if values[j].is_some_and(|id| !biome::is_ocean(id) && (continent || id == target)) {
                    labels[j] = label;
                    stack.push(j);
                }
            }
        }
        sizes.insert(label, size);
        if touches_edge {
            edges.insert(label);
        }
    }
    (labels, sizes, edges)
}

#[cfg(test)]
fn sample_layer(w: &World, x: i32, z: i32, id: u16, layer: Layer) -> Probe {
    sample_layer_with_water(
        w,
        x,
        z,
        id,
        layer,
        None,
        None,
        None,
        true,
        &TerrainDetailOptions::full(),
    )
}

fn sample_layer_with_water(
    w: &World,
    x: i32,
    z: i32,
    id: u16,
    layer: Layer,
    water: Option<bool>,
    volcanic_surface: Option<Option<crate::volcanic_lava::Surface>>,
    terrain: Option<Option<f64>>,
    use_saved_ground: bool,
    options: &TerrainDetailOptions,
) -> Probe {
    if w.dimension != Dimension::Overworld {
        let mut sample = w.sample(x, z, layer == Layer::Height);
        sample.biome = map_biome_name(w.dimension, id).to_owned();
        return sample;
    }
    // Terrain generation uses the stored quart source. Fuzzy display boundaries
    // must not move water onto a dry block or change generation conditions.
    let source_id = w.biome_id(x, z);
    let ocean = biome::is_ocean(source_id);
    let mut p = Probe {
        point: Point {
            x: x as f64,
            z: z as f64,
        },
        biome: biome::biome_name(id).to_owned(),
        ocean,
        ..Probe::default()
    };
    if layer == Layer::Ground {
        p.climate = w.region.climate(x, z);
        crate::ground_view::populate_at_scale_with_options(
            w,
            x,
            z,
            &mut p,
            use_saved_ground,
            use_saved_ground,
            options,
        );
        if p.ground_saved {
            return p;
        }
    }
    let surface = volcanic_surface.unwrap_or_else(|| {
        if options.volcanoes || options.springs {
            w.volcanic_surface_with_options(x, z, options)
        } else {
            None
        }
    });
    match surface {
        Some(crate::volcanic_lava::Surface::Lava(y)) => p.surface_lava = Some(y),
        Some(crate::volcanic_lava::Surface::Magma(y)) => p.surface_magma = Some(y),
        Some(crate::volcanic_lava::Surface::Water(y)) => {
            p.surface_water = true;
            p.surface_water_y = Some(y);
        }
        None => {}
    }
    let terrain_height = if layer == Layer::Height
        || options.rivers_lakes && water.is_none() && !ocean && surface.is_none()
    {
        terrain.unwrap_or_else(|| w.height(x, z))
    } else {
        None
    };
    p.surface_water = p.surface_lava.is_none()
        && p.surface_magma.is_none()
        && (p.surface_water_y.is_some()
            || ocean
            || water.unwrap_or_else(|| {
                if options.rivers_lakes {
                    crate::surface_water::is_surface_water(source_id, terrain_height)
                } else {
                    crate::surface_water::is_inland_water_biome(source_id)
                }
            }));
    if layer == Layer::Height {
        p.height = terrain_height;
    }
    if use_saved_ground && options.physical_surface && options.caves && surface.is_none() {
        if water.is_none() || layer == Layer::Height {
            crate::caves::populate_with_options(w, x, z, &mut p, options);
        } else if !p.surface_water {
            // A shared local mask already contains the physical fluid test.
            p.ocean = false;
        }
    }

    // River/lake biome borders may cross dry terrain. At close zoom saved ground
    // also preserves actual elevated water, rather than inferring it from a label.
    if options.rivers_lakes
        && (crate::surface_water::is_inland_water_biome(source_id)
            || crate::surface_water::is_inland_water_biome(id))
        && (matches!(layer, Layer::Biomes | Layer::Ground)
            || use_saved_ground && w.surface_source_id() != 0)
    {
        if layer == Layer::Biomes && p.biome.ends_with("lake") {
            if use_saved_ground {
                crate::ground_view::populate_saved(w, x, z, &mut p);
            }
        } else {
            crate::ground_view::populate_at_scale_with_options(
                w,
                x,
                z,
                &mut p,
                use_saved_ground,
                use_saved_ground,
                options,
            );
        }
        if !p.is_bare_surface()
            && p.ground_block.is_some()
            && matches!(layer, Layer::Biomes | Layer::Ground)
        {
            p.climate = w.region.climate(x, z);
        }
    }
    if p.is_bare_surface() {
        return p;
    }
    match layer {
        Layer::Biomes | Layer::Water | Layer::Ground => {}
        Layer::Temperature | Layer::Rainfall => p.climate = w.region.climate(x, z),
        Layer::Height => {}
        Layer::Geology => p.rocks = crate::geology::rock_stack(w.region.rock_point_at(x, z)),
        Layer::Forest => p.forest = w.region.forest_type(x >> 4, z >> 4),
        Layer::Hotspots => {
            let (value, age) = w.region.hotspot_at(x, z);
            p.hotspot = value;
            p.hotspot_age = age as f64;
        }
    }
    p
}

fn map_biome_name(dimension: Dimension, id: u16) -> &'static str {
    match dimension {
        Dimension::Beneath => crate::beneath::biome_name(id),
        Dimension::Moon | Dimension::Mars => crate::planetary::biome_name(id),
        Dimension::Overworld => biome::biome_name(id),
    }
}

/// Sample the displayed biome at the visible surface, or the selected Beneath
/// band. Keep this separate from World::biome_id, which drives world generation.
fn map_biome_id(w: &World, x: i32, z: i32) -> u16 {
    map_biome_id_with_height(w, x, z, None)
}

fn map_biome_id_with_height(w: &World, x: i32, z: i32, terrain: Option<Option<f64>>) -> u16 {
    if w.dimension == Dimension::Overworld {
        // A Y-independent source has only four distinct surrounding biomes. In
        // uniform interiors every fuzzy corner agrees, so avoid height/noise
        // sampling there (especially on large, unshaded ocean tiles).
        let (qx, qz) = (x.wrapping_sub(2) >> 2, z.wrapping_sub(2) >> 2);
        let id = w.earth_biome_at_quart(qx, qz);
        if [(qx + 1, qz), (qx, qz + 1), (qx + 1, qz + 1)]
            .into_iter()
            .all(|(sx, sz)| w.earth_biome_at_quart(sx, sz) == id)
        {
            return id;
        }
    }
    let y = if w.dimension == Dimension::Beneath {
        w.dimension.band(w.slice_y).0 + 8
    } else {
        let height = terrain.unwrap_or_else(|| w.height(x, z)).unwrap_or(63.0);
        if w.dimension == Dimension::Overworld {
            (height.trunc() as i32 + 1).max(crate::surface_water::SEA_SURFACE_BLOCK_Y)
        } else {
            height.floor() as i32
        }
    };
    w.block_biome_id(x, y, z)
}

fn surface_climate(world: &World, chunk_x: i32, chunk_z: i32) -> Vec<[f32; 2]> {
    if world.dimension != Dimension::Overworld {
        return Vec::new();
    }
    (0..256)
        .map(|index| {
            let x = chunk_x.wrapping_mul(16).wrapping_add(index % 16);
            let z = chunk_z.wrapping_mul(16).wrapping_add(index / 16);
            let climate = world.region.climate(x, z);
            [climate.temperature as f32, climate.rainfall as f32]
        })
        .collect()
}

fn sample_probe(w: &World, x: i32, z: i32, height: bool) -> Probe {
    let mut sample = w.sample(x, z, height);
    sample.biome = map_biome_name(w.dimension, map_biome_id(w, x, z)).to_owned();
    if w.dimension == Dimension::Overworld {
        crate::caves::populate(w, x, z, &mut sample);
        crate::ground_view::populate(w, x, z, &mut sample, true);
    }
    sample
}

fn sample_probe_with_display(
    w: &World,
    x: i32,
    z: i32,
    height: bool,
    display: &DisplayPrefs,
) -> Probe {
    // Explicit elevation/ground tooltips and teleport copies request actual
    // terrain. A basic hover must not restart cave and decoration generation.
    if height
        || display.terrain != TerrainDetailOptions::fast()
        || w.dimension != Dimension::Overworld
    {
        return sample_probe(w, x, z, height);
    }
    let id = w.biome_id(x, z);
    let (hotspot, age) = w.region.hotspot_at(x, z);
    Probe {
        point: Point {
            x: x as f64,
            z: z as f64,
        },
        biome: biome::biome_name(id).to_owned(),
        ocean: biome::is_ocean(id),
        surface_water: biome::is_ocean(id) || crate::surface_water::is_inland_water_biome(id),
        climate: w.climate(x, z),
        rocks: crate::geology::rock_stack(w.region.rock_point_at(x, z)),
        forest: w.region.forest_type(x >> 4, z >> 4),
        hotspot,
        hotspot_age: age as f64,
        ..Probe::default()
    }
}

/// Shared optimized raster sampling path for the GUI and headless renderer.
pub fn raster_color(world: &World, layer: Layer, x: i32, z: i32) -> [u8; 4] {
    raster_color_with_display(
        world,
        layer,
        x,
        z,
        &DisplayPrefs {
            terrain: TerrainDetailOptions::full(),
            ..Default::default()
        },
    )
}

pub fn raster_color_with_display(
    world: &World,
    layer: Layer,
    x: i32,
    z: i32,
    display: &DisplayPrefs,
) -> [u8; 4] {
    raster_color_with_display_step(world, layer, x, z, display, 1.0)
}

/// Raster color with an explicit world-block interval between adjacent output pixels.
/// Exporters use the same terrain shading neighbours as atlas tiles.
pub fn raster_color_with_display_step(
    world: &World,
    layer: Layer,
    x: i32,
    z: i32,
    display: &DisplayPrefs,
    step: f64,
) -> [u8; 4] {
    raster_color_with_display_spacing(world, layer, x, z, display, [step, step])
}

/// Color and terrain overlays at the output raster's horizontal and vertical spacing.
pub fn raster_color_with_display_spacing(
    world: &World,
    layer: Layer,
    x: i32,
    z: i32,
    display: &DisplayPrefs,
    spacing: [f64; 2],
) -> [u8; 4] {
    RasterSampler::new(world, layer, display, spacing).color(x, z, &|| false)
}

/// Reuse one composition/sampling plan across an export's pixels.
pub struct RasterSampler<'a> {
    world: &'a World,
    layer: Layer,
    display: &'a DisplayPrefs,
    spacing: [f64; 2],
    composition: Option<composition::Composition>,
    material_relief: bool,
    fast_biomes: Option<sampling::FastBiomes>,
}

impl<'a> RasterSampler<'a> {
    pub fn new(
        world: &'a World,
        layer: Layer,
        display: &'a DisplayPrefs,
        spacing: [f64; 2],
    ) -> Self {
        let step = spacing[0].max(spacing[1]);
        let fast_biomes = sampling::FastBiomes::new(world, layer, display);
        let composition = composition::Composition::new(display, layer, world.dimension, step);
        let material_relief = world.dimension == Dimension::Overworld
            && step <= 4.
            && composition
                .as_ref()
                .is_some_and(|plan| plan.uses_ground_material());
        Self {
            world,
            layer,
            display,
            spacing,
            composition,
            material_relief,
            fast_biomes,
        }
    }

    fn relief_height(&self, x: i32, z: i32) -> Option<f64> {
        let terrain = relief_solid_top(self.world, self.world.height(x, z));
        if self.material_relief {
            crate::ground_view::relief_height_with_options(
                self.world,
                x,
                z,
                terrain,
                &self.display.terrain,
            )
        } else if self.world.dimension == Dimension::Overworld
            && self.spacing[0].max(self.spacing[1]) <= 4.
        {
            crate::ground_view::spring_relief_height_with_options(
                self.world,
                x,
                z,
                terrain,
                &self.display.terrain,
            )
        } else {
            terrain
        }
    }

    pub fn color(&self, x: i32, z: i32, cancelled: &impl Fn() -> bool) -> [u8; 4] {
        let (world, layer, display, spacing) = (self.world, self.layer, self.display, self.spacing);
        if let Some(palette) = &self.fast_biomes {
            return palette.color(palette.id(world, x, z)).to_array();
        }
        let id = if display.terrain.precise_biome_boundaries {
            map_biome_id(world, x, z)
        } else {
            world.biome_id(x, z)
        };
        let p = if let Some(composition) = &self.composition {
            composition.sample(world, x, z, id, None, None, None)
        } else {
            sample_layer_with_water(
                world,
                x,
                z,
                id,
                layer,
                None,
                None,
                None,
                spacing[0].max(spacing[1]) <= 4.,
                &display.terrain,
            )
        };
        let mut pixel = if let Some(composition) = &self.composition {
            composition.color(world, id, &p, display, x, z)
        } else {
            display_color(world, layer, id, &p, display, x, z)
        };
        pixel = sampling::overlay_litter(
            world,
            pixel,
            x,
            z,
            spacing[0].max(spacing[1]),
            display,
            p.is_bare_surface(),
            cancelled,
        );
        if display.height_shading
            && display.hillshade_strength > 0.0
            && world.dimension != Dimension::Beneath
            && p.surface_lava.is_none()
            && p.surface_magma.is_none()
        {
            let step = spacing[0].max(spacing[1]).round().max(1.0);
            let dx = spacing[0].round().max(1.0) as i32;
            let dz = spacing[1].round().max(1.0) as i32;
            let heights = [
                self.relief_height(x, z),
                self.relief_height(x, z.saturating_sub(dz)),
                self.relief_height(x.saturating_sub(dx), z.saturating_sub(dz)),
            ];
            pixel = apply_hillshade(pixel, heights, step, display.hillshade_strength);
        }
        if display.contours.is_active()
            && world.dimension != Dimension::Beneath
            && p.surface_lava.is_none()
            && p.surface_magma.is_none()
            && (display.contours.underwater || !p.is_water())
        {
            let dx = spacing[0].round().max(1.0) as i32;
            let dz = spacing[1].round().max(1.0) as i32;
            let heights = std::array::from_fn(|i| {
                self.relief_height(
                    x.saturating_add((i as i32 % 3 - 1).saturating_mul(dx)),
                    z.saturating_add((i as i32 / 3 - 1).saturating_mul(dz)),
                )
            });
            pixel = apply_contours(pixel, heights, &display.contours);
        }
        if let Some(composition) = &self.composition {
            let surface = p.volcanic_surface().or_else(|| {
                p.is_water().then_some(crate::volcanic_lava::Surface::Water(
                    crate::surface_water::SEA_SURFACE_BLOCK_Y,
                ))
            });
            pixel = composition.overlay_trees(
                world,
                pixel,
                x,
                z,
                surface,
                spacing[0].max(spacing[1]),
                cancelled,
            );
        }
        [pixel.r(), pixel.g(), pixel.b(), pixel.a()]
    }
}

fn combine_tree_estimates(
    estimates: impl Iterator<Item = TreeEstimate>,
    area: f64,
) -> TreeEstimate {
    let mut total = 0.0;
    let mut species = std::collections::BTreeMap::<String, f64>::new();
    for estimate in estimates {
        total += estimate.total;
        for entry in estimate.species {
            let value = species.entry(entry.name).or_default();
            *value += entry.count;
        }
    }
    let mut entries: Vec<_> = species
        .into_iter()
        .map(|(name, count)| crate::model::TreeSpecies {
            name,
            share: if total > 0.0 { count / total } else { 0.0 },
            count,
        })
        .collect();
    entries.sort_by(|a, b| {
        b.count
            .total_cmp(&a.count)
            .then_with(|| a.name.cmp(&b.name))
    });
    TreeEstimate {
        total,
        per_hectare: if area > 0.0 {
            total * 10_000.0 / area
        } else {
            0.0
        },
        species: entries,
    }
}

fn tile_intersects_view(key: TileKey, bounds: Bounds, zoom: i32, layer: Layer) -> bool {
    let origin = key.origin();
    let span = key.span();
    key.zoom == zoom
        && key.layer == layer
        && origin.x < bounds.max_x
        && origin.x + span > bounds.min_x
        && origin.z < bounds.max_z
        && origin.z + span > bounds.min_z
}

fn refinement_requests(
    visible: &[(f64, TileKey, TileKey)],
    cached: impl Fn(&TileKey) -> bool,
) -> Vec<TileKey> {
    let mut requested = Vec::new();
    for (distance, coarse, fine) in visible {
        if cached(fine) {
            continue;
        }
        if cached(coarse) {
            // Center detail progresses while distant holes get their initial coverage.
            requested.push((distance + fine.span() * 0.2, *fine));
        } else {
            requested.push((distance * 0.5, *coarse));
        }
    }
    requested.sort_by(|a, b| a.0.total_cmp(&b.0));
    requested.into_iter().map(|(_, key)| key).collect()
}

/// Both sides of an edge sample the same world positions, including a one-texel gutter.
fn tile_sample(key: TileKey, resolution: usize, x: i32, z: i32) -> (i32, i32) {
    let o = key.origin();
    let step = key.span() / resolution as f64;
    (
        (o.x + (x as f64 + 0.5) * step).floor() as i32,
        (o.z + (z as f64 + 0.5) * step).floor() as i32,
    )
}
fn apply_hillshade(pixel: Color32, heights: [Option<f64>; 3], step: f64, strength: f32) -> Color32 {
    let [Some(center), Some(north), Some(northwest)] = heights else {
        return pixel;
    };
    let rgb = crate::hillshade::shade_rgb_xaero(
        [pixel.r(), pixel.g(), pixel.b()],
        center,
        north,
        northwest,
        step,
        strength,
    );
    Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], pixel.a())
}

/// Keep the generator's raw height grid immutable: water/biome calculations
/// consume that noise height, whereas lighting and contours use solid cells.
fn relief_solid_top(world: &World, terrain: Option<f64>) -> Option<f64> {
    if world.dimension == Dimension::Overworld {
        terrain
            .and_then(crate::surface_water::ordinary_solid_top)
            .map(f64::from)
    } else {
        terrain
    }
}

fn apply_contours(
    pixel: Color32,
    heights: [Option<f64>; 9],
    prefs: &crate::contours::ContourPrefs,
) -> Color32 {
    let rgb = crate::contours::shade_rgb([pixel.r(), pixel.g(), pixel.b()], heights, prefs);
    Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], pixel.a())
}

fn contour_tile_pixel(
    pixel: Color32,
    heights: &[Option<f64>],
    grid_side: usize,
    x: usize,
    z: usize,
    prefs: &crate::contours::ContourPrefs,
) -> Color32 {
    let neighborhood = std::array::from_fn(|i| heights[(z + i / 3) * grid_side + x + i % 3]);
    apply_contours(pixel, neighborhood, prefs)
}

fn height_grid_for_tile(
    world: &World,
    key: TileKey,
    resolution: usize,
    cancelled: impl Fn() -> bool,
) -> Option<HeightGrid> {
    if world.dimension == Dimension::Beneath {
        return None;
    }
    let cache_key = HeightGridKey::new(world, key);
    if let Some(grid) = HEIGHT_GRID_CACHE.access(|cache| cache.get(&cache_key).cloned()) {
        return Some(grid);
    }

    // Interior plus two texels of terrain padding on every side. The first padding layer
    // shades the displayed gutter; the outer one supplies its surrounding terrain samples.
    let side = resolution + 4;
    let mut grid = vec![None; side * side];
    let mut samples = 0usize;
    let mut store = |gx: usize, gz: usize, sx: i32, sz: i32| {
        if samples.is_multiple_of(32) && cancelled() {
            return false;
        }
        let (bx, bz) = tile_sample(key, resolution, sx, sz);
        grid[gz * side + gx] = world.height(bx, bz);
        samples += 1;
        true
    };
    // The main raster plus its visible gutter is visited in the same local Morton order as
    // color samples, keeping region-cache reuse high when zoomed across many cells.
    for &(x, z) in tile_sample_order(key.detail) {
        if !store(x + 1, z + 1, x as i32 - 1, z as i32 - 1) {
            return None;
        }
    }
    // Both hillshade and contours reuse this grid; contours also need diagonal neighbours.
    for z in 1..side - 1 {
        if !store(0, z, -2, z as i32 - 2)
            || !store(side - 1, z, resolution as i32 + 1, z as i32 - 2)
        {
            return None;
        }
    }
    for x in 1..side - 1 {
        if !store(x, 0, x as i32 - 2, -2)
            || !store(x, side - 1, x as i32 - 2, resolution as i32 + 1)
        {
            return None;
        }
    }
    for (gx, gz) in [(0, 0), (side - 1, 0), (0, side - 1), (side - 1, side - 1)] {
        if !store(gx, gz, gx as i32 - 2, gz as i32 - 2) {
            return None;
        }
    }
    if cancelled() {
        return None;
    }
    let grid = Arc::new(grid);
    HEIGHT_GRID_CACHE.access(|cache| {
        cache.put(cache_key, grid.clone());
    });
    Some(grid)
}

fn shade_tile_pixel(
    pixel: Color32,
    heights: &[Option<f64>],
    grid_side: usize,
    x: usize,
    z: usize,
    step: f64,
    strength: f32,
) -> Color32 {
    let index = |gx: usize, gz: usize| heights[gz * grid_side + gx];
    apply_hillshade(
        pixel,
        [index(x + 1, z + 1), index(x + 1, z), index(x, z)],
        step,
        strength,
    )
}

fn tile_uv(resolution: usize) -> Rect {
    let border = 1. / (resolution + 2) as f32;
    Rect::from_min_max(
        Pos2::new(border, border),
        Pos2::new(1. - border, 1. - border),
    )
}
fn tile_texture_options(key: TileKey, filter: LoadingFilter) -> TextureOptions {
    // Final block samples stay crisp; the preference controls magnified previews
    // and coarser tiles while the requested resolution loads.
    if key.detail && key.zoom == 0 || filter == LoadingFilter::Nearest {
        TextureOptions::NEAREST
    } else {
        TextureOptions::LINEAR
    }
}
fn tile_screen_rect(rect: Rect, center: Point, bpp: f64, origin: Point, span: f64) -> Rect {
    let project = |x: f64, z: f64| {
        Pos2::new(
            rect.center().x + ((x - center.x) / bpp) as f32,
            rect.center().y + ((z - center.z) / bpp) as f32,
        )
    };
    // Project shared edges identically; avoid independently rounded min + width sums.
    Rect::from_min_max(
        project(origin.x, origin.z),
        project(origin.x + span, origin.z + span),
    )
}
/// Morton traversal is computed once; output pixels still use ordinary row-major storage.
fn tile_sample_order(detail: bool) -> &'static [(usize, usize)] {
    use std::sync::OnceLock;
    static COARSE_ORDER: OnceLock<Vec<(usize, usize)>> = OnceLock::new();
    static DETAIL_ORDER: OnceLock<Vec<(usize, usize)>> = OnceLock::new();
    let cache = if detail { &DETAIL_ORDER } else { &COARSE_ORDER };
    cache.get_or_init(|| {
        fn visit(out: &mut Vec<(usize, usize)>, x: usize, z: usize, size: usize, side: usize) {
            if x >= side || z >= side {
                return;
            }
            if size == 1 {
                out.push((x, z));
                return;
            }
            let half = size / 2;
            for (dx, dz) in [(0, 0), (half, 0), (0, half), (half, half)] {
                visit(out, x + dx, z + dz, half, side);
            }
        }
        let side = if detail { DETAIL + 2 } else { COARSE + 2 };
        let mut order = Vec::with_capacity(side * side);
        visit(&mut order, 0, 0, side.next_power_of_two(), side);
        order
    })
}

fn render_tile(
    w: &World,
    key: TileKey,
    display: &DisplayPrefs,
    cancelled: impl Fn() -> bool,
) -> Option<CpuTile> {
    if let Some(palette) = sampling::FastBiomes::new(w, key.layer, display) {
        return sampling::render_fast_biomes(w, key, &palette, &cancelled);
    }
    let resolution = if key.detail { DETAIL } else { COARSE };
    let side = resolution + 2;
    let composition = composition::Composition::new(
        display,
        key.layer,
        w.dimension,
        key.span() / resolution as f64,
    );
    // Saved ground corrects local fluid coverage for material/grass maps. Those
    // overrides belong to this composition, not to the shared seed-surface masks
    // reused by every layer. Never let layer-switch order change a cached map.
    let share_surface_masks = w.surface_source_id() == 0
        || !composition
            .as_ref()
            .is_some_and(|plan| plan.uses_saved_ground());
    let local_relief = w.dimension == Dimension::Overworld && key.span() / resolution as f64 <= 4.;
    let material_relief = local_relief
        && composition
            .as_ref()
            .is_some_and(|plan| plan.uses_ground_material());
    let shaded = display.height_shading && display.hillshade_strength > 0.0;
    let contoured = display.contours.is_active();
    // Build completed relief heights once and use them for fuzzy biomes and water
    // as well. Basin sampling cannot evict these immutable samples mid-tile.
    let heights =
        if (shaded || contoured || composition.as_ref().is_some_and(|plan| plan.needs_height()))
            && w.dimension != Dimension::Beneath
        {
            Some(height_grid_for_tile(w, key, resolution, &cancelled)?)
        } else {
            None
        };
    let mut pixels = vec![Color32::TRANSPARENT; side * side];
    let mut water_pixels = vec![false; side * side];
    let mut volcanic_surface_pixels = vec![None; side * side];
    let mut biomes = vec![0; resolution * resolution];
    let mask_key = HeightGridKey::new(w, key).with_terrain(&display.terrain);
    let biome_key =
        HeightGridKey::new(w, key).with_biomes(display.terrain.precise_biome_boundaries);
    let cached_water = (w.dimension == Dimension::Overworld)
        .then(|| WATER_MASK_CACHE.access(|cache| cache.get(&mask_key).cloned()))
        .flatten();
    let cached_surface = (w.dimension == Dimension::Overworld)
        .then(|| VOLCANIC_SURFACE_CACHE.access(|cache| cache.get(&mask_key).cloned()))
        .flatten();
    let cached_biomes = BIOME_GRID_CACHE.access(|cache| cache.get(&biome_key).cloned());
    let mut biome_grid = cached_biomes.is_none().then(|| vec![0; side * side]);
    // Visit spatial neighbours together at every zoom. A long full-width row can
    // evict an entire working set of expensive regions before the next row reuses it.
    for (index, &(x, z)) in tile_sample_order(key.detail).iter().enumerate() {
        if index % 32 == 0 && cancelled() {
            return None;
        }
        let (bx, bz) = tile_sample(key, resolution, x as i32 - 1, z as i32 - 1);
        let terrain = heights
            .as_ref()
            .map(|grid| grid[(z + 1) * (resolution + 4) + x + 1]);
        let id = cached_biomes.as_ref().map_or_else(
            || {
                if display.terrain.precise_biome_boundaries {
                    map_biome_id_with_height(w, bx, bz, terrain)
                } else {
                    w.biome_id(bx, bz)
                }
            },
            |grid| grid[z * side + x],
        );
        if let Some(grid) = &mut biome_grid {
            grid[z * side + x] = id;
        }
        let water = cached_water.as_ref().map(|mask| mask[z * side + x]);
        let surface = cached_surface
            .as_ref()
            .map(|mask| mask.get(z * side + x).copied().flatten());
        let sample = if let Some(composition) = &composition {
            composition.sample(w, bx, bz, id, water, surface, terrain)
        } else {
            sample_layer_with_water(
                w,
                bx,
                bz,
                id,
                key.layer,
                water,
                surface,
                terrain,
                key.span() / resolution as f64 <= 4.,
                &display.terrain,
            )
        };
        let mut pixel = if let Some(composition) = &composition {
            composition.color(w, id, &sample, display, bx, bz)
        } else {
            display_color(w, key.layer, id, &sample, display, bx, bz)
        };
        pixel = sampling::overlay_litter(
            w,
            pixel,
            bx,
            bz,
            key.span() / resolution as f64,
            display,
            sample.is_bare_surface(),
            &cancelled,
        );
        pixels[z * side + x] = pixel;
        water_pixels[z * side + x] = sample.is_water();
        volcanic_surface_pixels[z * side + x] = sample.volcanic_surface();
        if (1..=resolution).contains(&x) && (1..=resolution).contains(&z) {
            biomes[(z - 1) * resolution + x - 1] = id;
        }
    }
    if let Some(grid) = biome_grid {
        if cancelled() {
            return None;
        }
        BIOME_GRID_CACHE.access(|cache| cache.put(biome_key, Arc::new(grid)));
    }
    if share_surface_masks && w.dimension == Dimension::Overworld && cached_water.is_none() {
        if cancelled() {
            return None;
        }
        WATER_MASK_CACHE.access(|cache| cache.put(mask_key, Arc::new(water_pixels.clone())));
    }
    if share_surface_masks && w.dimension == Dimension::Overworld && cached_surface.is_none() {
        if cancelled() {
            return None;
        }
        let mask = if volcanic_surface_pixels.iter().any(Option::is_some) {
            volcanic_surface_pixels.clone()
        } else {
            Vec::new()
        };
        VOLCANIC_SURFACE_CACHE.access(|cache| cache.put(mask_key, Arc::new(mask)));
    }
    if let Some(grid) = heights {
        let grid_side = resolution + 4;
        let grid = if w.dimension == Dimension::Overworld {
            let cache = if material_relief {
                &MATERIAL_RELIEF_CACHE
            } else {
                &TERRAIN_RELIEF_CACHE
            };
            if let Some(physical) = cache.access(|cache| cache.get(&mask_key).cloned()) {
                physical
            } else {
                let mut material = (*grid).clone();
                for gz in 0..grid_side {
                    if cancelled() {
                        return None;
                    }
                    for gx in 0..grid_side {
                        let (x, z) = tile_sample(key, resolution, gx as i32 - 2, gz as i32 - 2);
                        let index = gz * grid_side + gx;
                        material[index] = relief_solid_top(w, material[index]);
                        material[index] = if material_relief {
                            crate::ground_view::relief_height_with_options(
                                w,
                                x,
                                z,
                                material[index],
                                &display.terrain,
                            )
                        } else if local_relief {
                            crate::ground_view::spring_relief_height_with_options(
                                w,
                                x,
                                z,
                                material[index],
                                &display.terrain,
                            )
                        } else {
                            material[index]
                        };
                    }
                }
                if cancelled() {
                    return None;
                }
                let physical = Arc::new(material);
                cache.access(|cache| cache.put(mask_key, physical.clone()));
                physical
            }
        } else {
            grid
        };
        for (sample_index, &(x, z)) in tile_sample_order(key.detail).iter().enumerate() {
            if sample_index % 32 == 0 && cancelled() {
                return None;
            }
            let index = z * side + x;
            // Hot volcanic surfaces stay legible above buried terrain and contours.
            if volcanic_surface_pixels[index].is_some_and(|surface| surface.is_hot()) {
                continue;
            }
            if shaded {
                pixels[index] = shade_tile_pixel(
                    pixels[index],
                    &grid,
                    grid_side,
                    x,
                    z,
                    key.span() / resolution as f64,
                    display.hillshade_strength,
                );
            }
            if contoured && (display.contours.underwater || !water_pixels[index]) {
                pixels[index] =
                    contour_tile_pixel(pixels[index], &grid, grid_side, x, z, &display.contours);
            }
        }
    }
    if let Some(composition) = composition.as_ref().filter(|plan| plan.uses_trees()) {
        let step = key.span() / resolution as f64;
        for (index, &(x, z)) in tile_sample_order(key.detail).iter().enumerate() {
            if index % 32 == 0 && cancelled() {
                return None;
            }
            let offset = z * side + x;
            let surface = volcanic_surface_pixels[offset].or_else(|| {
                water_pixels[offset].then_some(crate::volcanic_lava::Surface::Water(
                    crate::surface_water::SEA_SURFACE_BLOCK_Y,
                ))
            });
            let (bx, bz) = tile_sample(key, resolution, x as i32 - 1, z as i32 - 1);
            pixels[offset] =
                composition.overlay_trees(w, pixels[offset], bx, bz, surface, step, &cancelled);
        }
    }
    let mut surface_water = Vec::with_capacity(resolution * resolution);
    for z in 1..=resolution {
        for x in 1..=resolution {
            surface_water.push(water_pixels[z * side + x]);
        }
    }
    Some(CpuTile {
        key,
        resolution,
        pixels,
        biomes,
        surface_water,
    })
}

fn cancellable_task(
    job_epoch: u64,
    current_epoch: &AtomicU64,
    job_serial: u64,
    current_revision: &AtomicU64,
) -> bool {
    job_epoch == current_epoch.load(Ordering::Relaxed)
        && job_serial == current_revision.load(Ordering::Relaxed)
}

fn worker(
    high: Receiver<Job>,
    low: Receiver<Job>,
    out: Sender<Reply>,
    epoch: Arc<AtomicU64>,
    tile_work: Arc<TileScheduler>,
    tile_wake: Receiver<()>,
    search_revision: Arc<AtomicU64>,
    task_revision: Arc<AtomicU64>,
) {
    let mut world: Option<(Spec, World)> = None;
    let mut tiles_since_request = 0;
    loop {
        let job = match high.try_recv() {
            Ok(job) => job,
            Err(crossbeam_channel::TryRecvError::Disconnected) => break,
            Err(crossbeam_channel::TryRecvError::Empty) => {
                if tiles_since_request >= 2 && !low.is_empty() {
                    tiles_since_request = 0;
                    if let Ok(job) = low.try_recv() {
                        job
                    } else {
                        continue;
                    }
                } else if let Some(task) = tile_work.take() {
                    Job::Tile(task.spec, task.key, task.revision)
                } else {
                    crossbeam_channel::select_biased! {
                        recv(high) -> j => match j { Ok(job) => job, Err(_) => break },
                        recv(tile_wake) -> _ => continue,
                        recv(low) -> j => match j { Ok(job) => job, Err(_) => break },
                    }
                }
            }
        };
        let spec = match &job {
            Job::Tile(s, ..)
            | Job::Probe(s, ..)
            | Job::Trees(s, ..)
            | Job::Features(s, ..)
            | Job::InspectStructure(s, ..)
            | Job::Surface(s, ..)
            | Job::Search(s, ..)
            | Job::Analyze(s, ..)
            | Job::Locate(s, ..)
            | Job::Export(s, ..) => s,
        };
        if spec.epoch != epoch.load(Ordering::Relaxed) {
            if let Job::Tile(ref s, key, revision) = job {
                let _ = out.send(Reply::Skipped(s.epoch, revision, key));
            }
            continue;
        }
        // Color/style revisions cancel stale jobs, while generator state stays
        // reusable across season changes. Save reloads have their own revision.
        if world
            .as_ref()
            .is_none_or(|(active, _)| !active.same_world(spec))
        {
            world = Some((
                spec.clone(),
                World::for_dimension(
                    spec.seed,
                    spec.settings.clone(),
                    spec.dimension,
                    spec.slice_y,
                )
                .with_surface_source(spec.world_path.clone(), spec.source_revision),
            ));
        }
        let w = &mut world.as_mut().unwrap().1;
        match job {
            Job::Tile(spec, key, revision) => {
                tiles_since_request += 1;
                let tile = render_tile(w, key, &spec.display, || {
                    spec.epoch != epoch.load(Ordering::Relaxed)
                        || !tile_work.wanted(spec.epoch, key)
                });
                let reply = match tile {
                    Some(tile) => Reply::Tile(spec.epoch, revision, tile),
                    None => Reply::Skipped(spec.epoch, revision, key),
                };
                let _ = out.send(reply);
            }
            Job::Probe(spec, p, height, serial) => {
                let _ = out.send(Reply::Probe(
                    spec.epoch,
                    serial,
                    sample_probe_with_display(
                        w,
                        p.x.floor() as i32,
                        p.z.floor() as i32,
                        height,
                        &spec.display,
                    ),
                ));
            }
            Job::Trees(spec, points, area_each, serial) => {
                let estimates = points.iter().map(|p| {
                    let x = p.x.floor() as i32;
                    let z = p.z.floor() as i32;
                    let id = w.biome_id(x, z);
                    let mut sample = Probe {
                        biome: biome::biome_name(id).to_owned(),
                        ocean: biome::is_ocean(id),
                        ..Probe::default()
                    };
                    sample.climate = w.region.climate(x, z);
                    sample.forest = w.region.forest_type(x >> 4, z >> 4);
                    w.tree_estimate(&sample, area_each)
                });
                let estimate = combine_tree_estimates(estimates, area_each * points.len() as f64);
                let _ = out.send(Reply::Trees(spec.epoch, serial, estimate));
            }
            Job::Features(spec, key, serial) => {
                let lod_bpp = features::lod_max_blocks_per_point(key.tier);
                let batch = if spec.dimension.is_planetary() {
                    crate::planetary::features::predict_visible(
                        w,
                        key.bounds(),
                        key.ores,
                        key.structures,
                        lod_bpp,
                        &|| spec.epoch != epoch.load(Ordering::Relaxed),
                    )
                } else if spec.dimension == Dimension::Beneath {
                    features::predict_beneath_visible(
                        w,
                        key.bounds(),
                        key.ores,
                        key.structures,
                        lod_bpp,
                        &|| spec.epoch != epoch.load(Ordering::Relaxed),
                    )
                } else {
                    features::predict_visible(
                        w,
                        key.bounds(),
                        key.ores,
                        key.structures,
                        lod_bpp,
                        &|| spec.epoch != epoch.load(Ordering::Relaxed),
                    )
                };
                let _ = out.send(Reply::Features(
                    spec.epoch,
                    serial,
                    key,
                    batch.markers,
                    batch.coverage.truncated,
                ));
            }
            Job::InspectStructure(spec, path, marker, serial) => {
                let result =
                    crate::structure_preview::inspect(w, &marker, path.as_deref(), &|| {
                        spec.epoch != epoch.load(Ordering::Relaxed)
                    });
                let _ = out.send(Reply::StructurePreview(spec.epoch, serial, result));
            }
            Job::Surface(spec, path, coordinates, serial, structure_bounds) => {
                let mut chunks = Vec::with_capacity(coordinates.len());
                for (x, z) in coordinates {
                    if spec.epoch != epoch.load(Ordering::Relaxed) {
                        break;
                    }
                    let predicted = || surface::SurfaceData {
                        chunk: crate::vegetation::generate_surface(w, x, z, &|| {
                            spec.epoch != epoch.load(Ordering::Relaxed)
                        }),
                        source: surface::SurfaceSource::Predicted,
                        climate: surface_climate(w, x, z),
                    };
                    let result = match path.as_deref() {
                        Some(path) => {
                            let saved = if let Some(bounds) = structure_bounds {
                                let (min_y, max_y) = if spec.dimension == Dimension::Beneath {
                                    let (band_min, band_max) = spec.dimension.band(spec.slice_y);
                                    (bounds.min_y.max(band_min), bounds.max_y.min(band_max))
                                } else {
                                    (bounds.min_y, bounds.max_y)
                                };
                                if min_y <= max_y {
                                    prism::read_surface_band(path, x, z, min_y, max_y).map(
                                        |chunk| {
                                            chunk.map(|chunk| {
                                                prism::clip_surface_chunk(chunk, bounds)
                                            })
                                        },
                                    )
                                } else {
                                    Ok(None)
                                }
                            } else if spec.dimension == Dimension::Beneath {
                                let (min_y, max_y) = spec.dimension.band(spec.slice_y);
                                prism::read_surface_band(path, x, z, min_y, max_y)
                            } else {
                                prism::read_surface_tile(path, x, z)
                            };
                            match saved {
                                Ok(Some(mut chunk)) => {
                                    if spec.dimension == Dimension::Overworld {
                                        for (index, column) in chunk.columns.iter_mut().enumerate()
                                        {
                                            if crate::ground_palette::is_grass_soil(&column.block) {
                                                let bx = x * 16 + (index % 16) as i32;
                                                let bz = z * 16 + (index / 16) as i32;
                                                let climate = w.region.climate(bx, bz);
                                                column.color = crate::ground_palette::multiply_tint(
                                                    crate::ground_palette::grass_tint_overlay()
                                                        .unwrap_or([255; 3]),
                                                    crate::map_colors::grass(
                                                        climate.temperature,
                                                        climate.rainfall,
                                                        column.y,
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                    Ok(Some(surface::SurfaceData {
                                        chunk,
                                        source: surface::SurfaceSource::Saved,
                                        climate: surface_climate(w, x, z),
                                    }))
                                }
                                Ok(None) if structure_bounds.is_some() => Ok(None),
                                Ok(None) => {
                                    if spec.dimension == Dimension::Overworld {
                                        Ok(Some(predicted()))
                                    } else {
                                        Ok(None)
                                    }
                                }
                                Err(error) => Err(error),
                            }
                        }
                        None if structure_bounds.is_some() => Ok(None),
                        None => {
                            if spec.dimension == Dimension::Overworld {
                                Ok(Some(predicted()))
                            } else {
                                Ok(None)
                            }
                        }
                    };
                    if spec.epoch != epoch.load(Ordering::Relaxed) {
                        break;
                    }
                    chunks.push((x, z, result));
                }
                if spec.epoch == epoch.load(Ordering::Relaxed) {
                    let _ = out.send(Reply::Surface(spec.epoch, serial, chunks));
                }
            }
            Job::Search(spec, search) => {
                if search.serial != search_revision.load(Ordering::Relaxed) {
                    continue;
                }
                let Some(outcome) = crate::search::predict_cancelable(
                    w,
                    &crate::search::Query {
                        origin: search.origin,
                        radius: search.radius,
                        text: search.query,
                        scope: search.scope,
                        structures_predicted: search.predictions,
                    },
                    || {
                        spec.epoch != epoch.load(Ordering::Relaxed)
                            || search.serial != search_revision.load(Ordering::Relaxed)
                    },
                ) else {
                    continue;
                };
                let _ = out.send(Reply::Search(
                    spec.epoch,
                    search.serial,
                    outcome.markers,
                    outcome.limited,
                    outcome.note,
                ));
            }
            Job::Analyze(spec, query, serial) => {
                let cancelled = || !cancellable_task(spec.epoch, &epoch, serial, &task_revision);
                if cancelled() {
                    continue;
                }
                let result = crate::analysis::analyze(w, &query, cancelled)
                    .ok_or_else(|| "Area bounds or sampling budget are invalid".to_owned());
                if !cancelled() {
                    let _ = out.send(Reply::Analysis(spec.epoch, serial, result));
                }
            }
            Job::Locate(spec, query, serial) => {
                let cancelled = || !cancellable_task(spec.epoch, &epoch, serial, &task_revision);
                if cancelled() {
                    continue;
                }
                let result = crate::analysis::locate(w, &query, cancelled)
                    .ok_or_else(|| "Location search bounds or filters are invalid".to_owned());
                if !cancelled() {
                    let _ = out.send(Reply::Locate(spec.epoch, serial, result));
                }
            }
            Job::Export(spec, request, serial) => {
                let cancelled = || !cancellable_task(spec.epoch, &epoch, serial, &task_revision);
                if cancelled() {
                    continue;
                }
                let result =
                    crate::exports::execute_with_display(w, &request, &spec.display, cancelled);
                if !cancelled() {
                    let _ = out.send(Reply::Export(spec.epoch, serial, result));
                }
            }
        }
    }
}

/// Runs the isolated generation service without creating a native window.
pub fn run_engine() -> Result<(), String> {
    engine::run()
}

#[cfg(test)]
mod tests {
    fn accurate_display() -> crate::display::DisplayPrefs {
        crate::display::DisplayPrefs {
            terrain: crate::display::TerrainDetailOptions {
                surface_litter: false,
                ..crate::display::TerrainDetailOptions::full()
            },
            ..crate::display::DisplayPrefs::default()
        }
    }

    use super::*;

    #[test]
    fn calendar_revisions_reuse_world_inputs_but_never_save_revisions() {
        let atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        let base = atlas.spec.clone();
        let mut calendar = base.clone();
        calendar.epoch += 1;
        calendar.display.season = Some(0.8);
        assert!(base.same_world(&calendar));
        let mut reload = calendar.clone();
        reload.source_revision += 1;
        assert!(!base.same_world(&reload));
        let mut seed = calendar.clone();
        seed.seed += 1;
        assert!(!base.same_world(&seed));
        let mut settings = calendar.clone();
        settings.settings.temperature_scale += 100.;
        assert!(!base.same_world(&settings));
        let mut dimension = calendar.clone();
        dimension.dimension = Dimension::Moon;
        assert!(!base.same_world(&dimension));
        let mut source = calendar.clone();
        source.world_path = Some(PathBuf::from("different-world"));
        assert!(!base.same_world(&source));
    }

    #[test]
    fn calendar_changes_preserve_completed_feature_scans_and_destination_heights() {
        let mut atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        let key = FeatureTileKey {
            x: 0,
            z: 0,
            tier: 0,
            scale: 0,
            ores: true,
            structures: true,
        };
        let pin = marker(1, MarkerKind::Structure, 12., 14., "tfg");
        atlas.cache_feature_result(key, vec![pin.clone()], false);
        atlas.destination_probe = Some(Probe {
            point: Point { x: 12., z: 14. },
            ground_y: Some(70),
            ..Default::default()
        });
        let mut prefs = atlas.spec.display.clone();
        prefs.season = Some(0.8);
        atlas.set_display_options(&prefs);
        assert_eq!(atlas.feature_tiles.get(&key).unwrap().markers[0].id, pin.id);
        assert_eq!(
            atlas.probe_at(Point { x: 12., z: 14. }).unwrap().ground_y,
            Some(70)
        );
    }

    #[test]
    fn destination_height_reply_survives_pointer_movement_and_is_cleared_on_world_change() {
        let mut atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        let target = Point { x: -0.2, z: 7.9 };
        atlas.destination_request = Some((target, 42));
        atlas.hover_point = Some(Point { x: 500., z: 500. });
        atlas
            .out
            .send(Reply::Probe(
                atlas.spec.epoch,
                42,
                Probe {
                    point: Point { x: -1., z: 7. },
                    ground_y: Some(119),
                    ..Default::default()
                },
            ))
            .unwrap();
        atlas.poll();
        assert_eq!(atlas.probe_at(target).and_then(|p| p.ground_y), Some(119));
        assert!(atlas.destination_request.is_none());
        assert!(atlas.probe_at(Point { x: 0., z: 7. }).is_none());
        atlas.set_world(1, Settings::default(), None);
        assert!(atlas.probe_at(target).is_none());
    }

    #[test]
    fn dry_inland_biome_columns_keep_land_colors_and_layer_values() {
        use biome::ids::id;
        let world = World::new(0, Settings::default());
        for (x, z, expected_id, expected_height) in [
            (10880, -16000, id::RIVER, 146.),
            (10752, -15616, id::OLD_MOUNTAIN_LAKE, 85.),
            (12800, -15232, id::LAKE, 68.),
        ] {
            assert_eq!(world.biome_id(x, z), expected_id);
            assert_eq!(world.height(x, z), Some(expected_height));
            assert!(!world.probe(x, z).is_water());
            assert!(!world.sample(x, z, false).is_water());
            assert!(!world.is_surface_water(x, z));
            assert!(!world.is_surface_fluid(x, z));
            let display_id = map_biome_id(&world, x, z);
            assert!(crate::surface_water::is_inland_water_biome(display_id));
            for layer in Layer::ALL {
                let sample = sample_layer(&world, x, z, display_id, layer);
                assert!(!sample.is_bare_surface(), "{layer:?} at {x},{z}");
                assert_eq!(
                    sample.biome,
                    biome::biome_name(display_id),
                    "retain biome identity"
                );
                let rgb = raster_color(&world, layer, x, z);
                assert_ne!(
                    rgb,
                    [71, 157, 178, 255],
                    "dry {layer:?} cannot have the river-water color"
                );
                assert_ne!(
                    rgb,
                    [43, 105, 133, 255],
                    "dry {layer:?} cannot have the water color"
                );
            }
            assert_ne!(
                raster_color(&world, Layer::Biomes, x, z),
                raster_color(&world, Layer::Ground, x, z)
            );
            assert_eq!(
                raster_color(&world, Layer::Water, x, z),
                [185, 188, 158, 255]
            );
            let stacked = DisplayPrefs {
                layer_stack: Some(vec![
                    crate::display::LayerBlend {
                        layer: Layer::Height,
                        opacity: 1.,
                    },
                    crate::display::LayerBlend {
                        layer: Layer::Water,
                        opacity: 1.,
                    },
                ]),
                ..accurate_display()
            };
            assert_eq!(
                raster_color_with_display(&world, Layer::Biomes, x, z, &stacked),
                raster_color(&world, Layer::Height, x, z),
                "dry water overlay must leave terrain visible"
            );
        }
        for (x, z) in [(-50, 80), (8780, 10992), (8827, 11107)] {
            assert!(world.probe(x, z).is_water(), "saved wet control at {x},{z}");
            let rgb = raster_color(&world, Layer::Water, x, z);
            assert!(rgb[2] > rgb[0] && rgb[1] > rgb[0]);
        }
    }

    #[test]
    fn covered_inland_water_is_dark_blue_without_enabling_ground_colors() {
        let world = World::new(0, Settings::default());
        for (x, z) in [(10880, -16000), (10752, -15616), (12800, -15232)] {
            let id = map_biome_id(&world, x, z);
            let sample = sample_layer(&world, x, z, id, Layer::Biomes);
            assert!(!sample.is_fluid(), "the overlying mountain remains solid");
            let plain = DisplayPrefs {
                height_shading: false,
                ..accurate_display()
            };
            let expected = [
                UNDERGROUND_WATER_COLOR[0],
                UNDERGROUND_WATER_COLOR[1],
                UNDERGROUND_WATER_COLOR[2],
                255,
            ];
            assert_eq!(
                raster_color_with_display(&world, Layer::Biomes, x, z, &plain),
                expected
            );
            let stack = DisplayPrefs {
                layer_stack: Some(vec![crate::display::LayerBlend {
                    layer: Layer::Biomes,
                    opacity: 1.,
                }]),
                ..plain.clone()
            };
            assert_eq!(
                raster_color_with_display(&world, Layer::Biomes, x, z, &stack),
                expected
            );
            let ground = raster_color_with_display(&world, Layer::Ground, x, z, &plain);
            assert_ne!(expected, ground);
            let grass = DisplayPrefs {
                grass_colors: true,
                grass_opacity: 1.,
                ..stack.clone()
            };
            assert_ne!(
                raster_color_with_display(&world, Layer::Biomes, x, z, &grass),
                expected
            );
            let accessible = DisplayPrefs {
                palette: crate::display::Palette::Colorblind,
                ..stack
            };
            assert_eq!(
                raster_color_with_display(&world, Layer::Biomes, x, z, &accessible),
                expected
            );
            assert!(!world.is_surface_water(x, z));
        }
        // The saved dry low river bank must not turn into an underground river.
        assert!(!crate::surface_water::is_underground_river(
            &world, 8833, 11107, 62
        ));
    }

    #[test]
    fn dry_inland_palette_fallback_never_uses_a_water_biome_swatch() {
        for id in [
            biome::ids::id::RIVER,
            biome::ids::id::LAKE,
            biome::ids::id::MOUNTAIN_LAKE,
        ] {
            let sample = Probe {
                biome: biome::biome_name(id).into(),
                height: Some(100.),
                ..Default::default()
            };
            for layer in [Layer::Biomes, Layer::Ground, Layer::Water] {
                assert_eq!(color(layer, id, &sample), Color32::from_rgb(185, 188, 158));
                let wet = Probe {
                    surface_water: true,
                    ..sample.clone()
                };
                assert_eq!(color(layer, id, &wet), Color32::from_rgb(71, 157, 178));
            }
        }
    }

    #[test]
    fn inland_dry_land_masks_and_export_colors_agree_after_switching_layers() {
        let world = World::new(0, Settings::default());
        let key = TileKey {
            zoom: 0,
            x: 42,
            z: -63,
            layer: Layer::Biomes,
            detail: false,
        };
        for display in [
            accurate_display(),
            DisplayPrefs {
                layer_stack: Some(vec![
                    crate::display::LayerBlend {
                        layer: Layer::Height,
                        opacity: 1.,
                    },
                    crate::display::LayerBlend {
                        layer: Layer::Water,
                        opacity: 1.,
                    },
                ]),
                ..accurate_display()
            },
        ] {
            let tile = render_tile(&world, key, &display, || false).unwrap();
            let step = key.span() / COARSE as f64;
            let raster = RasterSampler::new(&world, key.layer, &display, [step, step]);
            let mut checked = 0;
            for z in 0..COARSE {
                for x in 0..COARSE {
                    let index = z * COARSE + x;
                    if !crate::surface_water::is_inland_water_biome(tile.biomes[index]) {
                        continue;
                    }
                    let (wx, wz) = tile_sample(key, COARSE, x as i32, z as i32);
                    if world.height(wx, wz).is_none_or(|height| height <= 63.)
                        || world.volcanic_surface(wx, wz).is_some()
                    {
                        continue;
                    }
                    assert!(
                        !tile.surface_water[index],
                        "dry river/lake tile cell at {wx},{wz}"
                    );
                    if checked < 16 {
                        assert_eq!(
                            tile.pixels[(z + 1) * (COARSE + 2) + x + 1].to_array(),
                            raster.color(wx, wz, &|| false)
                        );
                    }
                    checked += 1;
                }
            }
            assert!(
                checked > 0,
                "tile must cover actual inland-biome land above sea level"
            );
        }
    }

    #[test]
    fn observed_seed_zero_lowland_is_water_on_every_layer_without_raising_the_ground() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/flooded-lowlands/seed0-991-990.json"
        ))
        .unwrap();
        let x = fixture["requested_point"]["x"].as_i64().unwrap() as i32;
        let z = fixture["requested_point"]["z"].as_i64().unwrap() as i32;
        let world = World::new(0, Settings::default());
        assert_eq!(world.height(x, z), Some(59.));
        let probe = world.probe(x, z);
        assert_eq!(probe.biome, "tfg:earth/lowlands");
        assert!(!probe.ocean);
        assert!(probe.is_water());
        assert_eq!(
            probe.height,
            Some(59.),
            "flooding must not turn the terrain model into a flat sea surface"
        );
        let mut display = DisplayPrefs {
            height_shading: true,
            hillshade_strength: 1.,
            temperature_enabled: true,
            temperature_min: -50.,
            temperature_max: -40.,
            ..accurate_display()
        };
        for palette in [
            crate::display::Palette::Natural,
            crate::display::Palette::Colorblind,
        ] {
            display.palette = palette;
            for layer in Layer::ALL {
                let shaded = raster_color_with_display(&world, layer, x, z, &display);
                assert!(shaded[2] > shaded[0], "water hue remains blue in {layer:?}");
                let mut flat_display = display.clone();
                flat_display.height_shading = false;
                assert_eq!(
                    raster_color_with_display(&world, layer, x, z, &flat_display),
                    [43, 105, 133, 255],
                    "land climate highlights must not recolor lowland water"
                );
            }
        }
    }

    #[test]
    fn reported_floodplain_shelf_stays_land_while_adjacent_riverbed_stays_water_on_every_layer() {
        let world = World::new(0, Settings::default());
        let bank = world.probe(8833, 11107);
        assert_eq!(bank.biome, "tfg:earth/plains");
        assert_eq!(bank.height, Some(61.));
        assert!(
            !bank.is_water(),
            "the observed dry grass block at Y62 must not be painted as water"
        );
        let bed = world.probe(8827, 11107);
        assert_eq!(bed.height, Some(60.));
        assert!(
            bed.is_water(),
            "observed water at Y62 must still cover the neighboring bed"
        );
        for layer in Layer::ALL {
            assert_ne!(
                raster_color(&world, layer, 8833, 11107),
                [43, 105, 133, 255],
                "dry bank in {layer:?}"
            );
            let mut display = accurate_display();
            display.height_shading = false;
            assert_eq!(
                raster_color_with_display(&world, layer, 8827, 11107, &display),
                [43, 105, 133, 255],
                "unshaded wet bed in {layer:?}"
            );
        }
    }

    #[test]
    fn close_biome_tiles_resolve_a_dry_boundary_inside_one_quart_cell() {
        let world = World::new(0, Settings::default());
        let key = TileKey {
            layer: Layer::Biomes,
            ..test_key(1, -1, 0, true)
        };
        let tile = render_tile(&world, key, &accurate_display(), || false).unwrap();
        for dz in 0..4 {
            for dx in 0..4 {
                let (x, z) = (380 + dx, -248 + dz);
                assert_eq!(world.biome_id(x, z), biome::ids::id::HILLS);
                let expected = if dx == 0 && dz >= 2 {
                    biome::ids::id::SHORE
                } else {
                    biome::ids::id::HILLS
                };
                let index = (z + 256) as usize * DETAIL + (x - 256) as usize;
                assert_eq!(tile.biomes[index], expected, "block {x},{z}");
                assert!(
                    !tile.surface_water[index],
                    "biome display must not move the shoreline water mask"
                );
                assert_eq!(
                    sample_probe(&world, x, z, false).biome,
                    biome::biome_name(expected)
                );
            }
        }
    }

    #[test]
    fn water_coverage_is_reused_across_layers_and_cancelled_tiles_do_not_publish_masks() {
        WATER_MASK_CACHE.access(|cache| cache.clear());
        BIOME_GRID_CACHE.access(|cache| cache.clear());
        let world = World::new(0, Settings::default());
        let key = TileKey {
            zoom: 0,
            x: 61,
            z: 61,
            layer: Layer::Biomes,
            detail: false,
        };
        let cache_key = HeightGridKey::new(&world, key).with_terrain(&accurate_display().terrain);
        let biome_key = HeightGridKey::new(&world, key).with_biomes(true);
        assert!(render_tile(&world, key, &accurate_display(), || true).is_none());
        assert!(WATER_MASK_CACHE.access(|cache| !cache.contains(&cache_key)));
        assert!(BIOME_GRID_CACHE.access(|cache| !cache.contains(&biome_key)));
        let start = Instant::now();
        let first = render_tile(&world, key, &accurate_display(), || false).unwrap();
        let cold = start.elapsed();
        let mask = WATER_MASK_CACHE
            .access(|cache| cache.peek(&cache_key).cloned())
            .unwrap();
        let biome_grid = BIOME_GRID_CACHE
            .access(|cache| cache.peek(&biome_key).cloned())
            .unwrap();
        assert!(mask.iter().any(|water| *water));
        let start = Instant::now();
        let second = render_tile(
            &world,
            TileKey {
                layer: Layer::Rainfall,
                ..key
            },
            &accurate_display(),
            || false,
        )
        .unwrap();
        let reused = WATER_MASK_CACHE
            .access(|cache| cache.peek(&cache_key).cloned())
            .unwrap();
        assert!(Arc::ptr_eq(&mask, &reused));
        let reused_biomes = BIOME_GRID_CACHE
            .access(|cache| cache.peek(&biome_key).cloned())
            .unwrap();
        assert!(Arc::ptr_eq(&biome_grid, &reused_biomes));
        assert_eq!(first.biomes, second.biomes);
        assert_eq!(first.surface_water, second.surface_water);
        for (index, water) in mask.iter().enumerate() {
            if *water {
                assert_eq!(first.pixels[index], second.pixels[index]);
            }
        }
        eprintln!(
            "water tile cold={cold:?}; cached layer={:?}",
            start.elapsed()
        );
    }

    #[test]
    fn completed_surface_grids_are_reused_by_a_different_engine_worker() {
        let key = TileKey {
            zoom: 0,
            x: 241,
            z: -463,
            layer: Layer::Biomes,
            detail: false,
        };
        let sample = move |layer| {
            std::thread::spawn(move || {
                let world = World::new(834_759, Settings::default());
                let tile_key = TileKey { layer, ..key };
                let raw_key = HeightGridKey::new(&world, tile_key);
                let biome_key = raw_key.with_biomes(true);
                let cache_key = raw_key.with_terrain(&accurate_display().terrain);
                let display = DisplayPrefs {
                    height_shading: true,
                    ..accurate_display()
                };
                let tile = render_tile(&world, tile_key, &display, || false).unwrap();
                let biomes = BIOME_GRID_CACHE
                    .access(|cache| cache.get(&biome_key).cloned())
                    .unwrap();
                let water = WATER_MASK_CACHE
                    .access(|cache| cache.get(&cache_key).cloned())
                    .unwrap();
                let volcanic = VOLCANIC_SURFACE_CACHE
                    .access(|cache| cache.get(&cache_key).cloned())
                    .unwrap();
                let heights = HEIGHT_GRID_CACHE
                    .access(|cache| cache.get(&raw_key).cloned())
                    .unwrap();
                (tile, biomes, water, volcanic, heights)
            })
            .join()
            .unwrap()
        };
        let (first, b1, w1, v1, h1) = sample(Layer::Biomes);
        let (second, b2, w2, v2, h2) = sample(Layer::Rainfall);
        assert!(Arc::ptr_eq(&b1, &b2));
        assert!(Arc::ptr_eq(&w1, &w2));
        assert!(Arc::ptr_eq(&v1, &v2));
        assert!(Arc::ptr_eq(&h1, &h2));
        assert_eq!(first.biomes, second.biomes);
        assert_eq!(first.surface_water, second.surface_water);
    }

    #[test]
    fn complete_saved_chunks_remove_false_candidates_from_map_and_search_only() {
        let mut atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        let mut absent = marker(1, MarkerKind::Structure, 15864., 15848., "tfg");
        absent.id = "tfg:illagers/malay_village_living@991,990".into();
        let unknown = marker(2, MarkerKind::Structure, 15880., 15848., "tfg");
        let mut saved = absent.clone();
        saved.confirmed = true;
        saved.id = "actual:saved_start".into();
        saved.namespace = "actual".into();
        let ore = marker(3, MarkerKind::Ore, 15864., 15848., "tfg");
        let mut volcano = marker(4, MarkerKind::Volcano, 15864., 15848., "tfg");
        volcano.id = "terralens:volcano/cinder_cone@15864,15848".into();
        atlas.predicted = vec![
            absent.clone(),
            unknown.clone(),
            ore.clone(),
            volcano.clone(),
        ];
        atlas.rebuild_markers();
        assert!(atlas.markers.iter().any(|m| m.id == absent.id));
        let mut coverage = prism::ChunkCoverage {
            region_x: 30,
            region_z: 30,
            slots: [0; 16],
        };
        let slot = 30 * 32 + 31;
        coverage.slots[slot / 64] |= 1_u64 << (slot % 64);
        atlas
            .out
            .send(Reply::Structures(
                atlas.scan_epoch,
                Ok(prism::StructureScan {
                    markers: vec![saved.clone()],
                    completed: vec![coverage],
                    ..Default::default()
                }),
            ))
            .unwrap();
        atlas.poll();
        assert!(atlas.structure_candidate_absent(&absent));
        assert!(!atlas.structure_candidate_absent(&unknown));
        assert!(!atlas.structure_candidate_absent(&saved));
        assert!(!atlas.structure_candidate_absent(&ore));
        assert!(!atlas.structure_candidate_absent(&volcano));
        assert!(atlas.markers.iter().any(|m| m.id == volcano.id));
        assert!(
            atlas
                .visible_markers(
                    Bounds {
                        min_x: 15860.,
                        max_x: 15870.,
                        min_z: 15840.,
                        max_z: 15855.
                    },
                    false,
                    true
                )
                .iter()
                .any(|m| m.id == volcano.id)
        );
        assert!(
            atlas
                .visible_markers(
                    Bounds {
                        min_x: 15860.,
                        max_x: 15870.,
                        min_z: 15840.,
                        max_z: 15855.
                    },
                    true,
                    false
                )
                .iter()
                .all(|m| m.id != volcano.id)
        );
        let revision = atlas.structure_preview_revision();
        atlas.inspect_structure(&volcano);
        assert_eq!(atlas.structure_preview_revision(), revision);
        assert!(!atlas.structure_preview_pending(&volcano));
        assert!(!atlas.markers.iter().any(|m| m.id == absent.id));
        assert!(atlas.markers.iter().any(|m| m.id == unknown.id));
        assert!(atlas.markers.iter().any(|m| m.id == saved.id));
        atlas.search = Some(Search {
            origin: absent.point(),
            radius: 1000.,
            query: String::new(),
            predictions: true,
            scope: crate::search::Scope::All,
            serial: 1,
        });
        atlas.search_candidates = vec![absent.clone(), unknown.clone(), ore, volcano.clone()];
        atlas.update_search(false);
        assert!(!atlas.search_results.iter().any(|m| m.id == absent.id));
        assert!(atlas.search_results.iter().any(|m| m.id == unknown.id));
        assert!(atlas.search_results.iter().any(|m| m.id == saved.id));
        assert!(atlas.search_results.iter().any(|m| m.id == volcano.id));
        atlas.set_dimension(Dimension::Moon, 64);
        assert!(
            !atlas.structure_candidate_absent(&absent),
            "completion evidence must not cross dimensions"
        );
    }

    #[test]
    fn structure_inspection_keeps_instance_cache_across_zoom_and_rejects_stale_results() {
        use crate::structure_preview::{PreviewSource, StructurePreview};
        let mut atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        let instance = marker(1, MarkerKind::Structure, -24., 8., "test");
        let key = PreviewKey::new(&instance);
        atlas.preview_pending = Some((key.clone(), 9));
        atlas
            .out
            .send(Reply::StructurePreview(
                atlas.spec.epoch,
                8,
                Err("stale inspection".into()),
            ))
            .unwrap();
        atlas.poll();
        assert!(atlas.structure_preview_pending(&instance));
        assert!(atlas.structure_preview_error(&instance).is_none());
        atlas
            .out
            .send(Reply::StructurePreview(
                atlas.spec.epoch,
                9,
                Ok(StructurePreview::empty(&instance.id, PreviewSource::Saved)),
            ))
            .unwrap();
        atlas.poll();
        assert!(!atlas.structure_preview_pending(&instance));
        assert!(atlas.structure_preview(&instance).is_some());
        let revision = atlas.structure_preview_revision();
        for bpp in [0.125, 4., 512., 1.] {
            atlas.bpp = bpp;
            atlas.center = Point {
                x: 100_000.,
                z: -200_000.,
            };
            atlas.inspect_structure(&instance);
            assert!(atlas.structure_preview(&instance).is_some());
            assert_eq!(atlas.structure_preview_revision(), revision);
            assert_eq!(atlas.preview_serial, 0, "cached pan/zoom must never rescan");
        }
        atlas.set_dimension(Dimension::Moon, 64);
        assert!(atlas.structure_preview(&instance).is_none());
        atlas
            .out
            .send(Reply::StructurePreview(
                atlas.spec.epoch - 1,
                9,
                Ok(StructurePreview::empty(&instance.id, PreviewSource::Saved)),
            ))
            .unwrap();
        atlas.poll();
        assert!(atlas.structure_preview(&instance).is_none());
    }

    #[test]
    fn structure_inspection_cache_survives_beneath_bands_but_not_a_new_world() {
        use crate::structure_preview::{PreviewSource, StructurePreview};
        let mut atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        atlas.set_dimension(Dimension::Beneath, 95);
        let instance = marker(1, MarkerKind::Structure, 8., 8., "test");
        atlas.structure_previews.put(
            PreviewKey::new(&instance),
            Ok(StructurePreview::empty(&instance.id, PreviewSource::Saved)),
        );
        atlas.set_dimension(Dimension::Beneath, 159);
        assert!(atlas.structure_preview(&instance).is_some());
        atlas.set_world(4, Settings::default(), None);
        assert!(atlas.structure_preview(&instance).is_none());
    }

    #[test]
    fn legend_sampling_and_percentages_are_bounded_and_use_loaded_denominator() {
        const SIDE: usize = 64;
        assert_eq!(SIDE * SIDE, 4096);
        let entry = LegendEntry {
            id: 7,
            name: "test".into(),
            color: [1, 2, 3],
            count: 1,
        };
        assert_eq!(entry.percent(4), 25.0);
        assert_eq!(entry.percent(0), 0.0);
    }

    #[test]
    fn dimension_biome_highlighting_preserves_overworld_oceans() {
        let base = Color32::from_rgb(40, 80, 120);
        let display = DisplayPrefs {
            highlighted_biomes: vec![biome::ids::id::PLAINS],
            ..accurate_display()
        };
        assert_eq!(
            highlighted_biome_color(base, biome::ids::id::OCEAN, &display, true),
            base
        );
        assert_ne!(
            highlighted_biome_color(base, biome::ids::id::OCEAN, &display, false),
            base
        );
        assert_ne!(
            highlighted_biome_color(base, biome::ids::id::PLAINS, &display, true),
            base
        );
    }

    #[test]
    fn biome_swatch_uses_overworld_and_beneath_palettes() {
        let display = accurate_display();
        assert_ne!(
            biome_legend_color(1000, Dimension::Beneath, &display),
            biome_legend_color(0, Dimension::Overworld, &display)
        );
        assert_eq!(
            biome_legend_color(1000, Dimension::Beneath, &display),
            crate::beneath::biome_color(1000)
        );
    }

    #[test]
    fn planetary_biome_rasters_match_their_legend_and_ignore_earth_filters() {
        for dimension in [Dimension::Moon, Dimension::Mars] {
            let world = World::for_dimension(0, Settings::default(), dimension, 64);
            let probe = world.sample(338, 514, false);
            let id = world.biome_id(338, 514);
            for palette in [
                crate::display::Palette::Natural,
                crate::display::Palette::Colorblind,
            ] {
                let display = DisplayPrefs {
                    palette,
                    ..accurate_display()
                };
                let pixel = display_color(&world, Layer::Biomes, id, &probe, &display, 338, 514);
                assert_eq!(
                    pixel.to_array()[..3],
                    biome_legend_color(id, dimension, &display)
                );
                assert_eq!(
                    display_color(&world, Layer::Temperature, id, &probe, &display, 338, 514),
                    pixel,
                    "A stale Earth layer cannot add synthetic climate colors to a planet"
                );
            }
        }
    }

    #[test]
    fn layer_legend_scales_match_displayed_numeric_ranges() {
        let natural = layer_scale(Layer::Temperature, crate::display::Palette::Natural).unwrap();
        assert_eq!((natural.min, natural.max, natural.unit), (-25., 35., "°C"));
        let accessible =
            layer_scale(Layer::Temperature, crate::display::Palette::Colorblind).unwrap();
        assert_eq!((accessible.min, accessible.max), (-30., 30.));
        assert_eq!(
            forest_legend_color(0, crate::display::Palette::Natural),
            [199, 194, 158]
        );
        assert_eq!(
            forest_legend_color(9, crate::display::Palette::Colorblind),
            [18, 59, 81]
        );
        assert!(layer_scale(Layer::Forest, crate::display::Palette::Natural).is_none());
    }

    #[test]
    fn rock_swatch_matches_both_raster_palettes() {
        let name = "granite";
        let hash = name
            .bytes()
            .fold(0u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
        let h = (hash % 70) as u8;
        assert_eq!(
            rock_legend_color(name, crate::display::Palette::Natural),
            [121 + h, 121 + h / 2, 114 + h / 3]
        );
        assert_eq!(
            color_from_rgb(rock_legend_color(name, crate::display::Palette::Colorblind)),
            display_color(
                &World::new(1, Settings::default()),
                Layer::Geology,
                biome::ids::id::PLAINS,
                &Probe {
                    rocks: [name.into(), "diorite".into(), "gabbro".into()],
                    ..Probe::default()
                },
                &DisplayPrefs {
                    palette: crate::display::Palette::Colorblind,
                    ..accurate_display()
                },
                0,
                0,
            )
        );
    }

    #[test]
    fn task_revision_and_world_epoch_cancel_background_work() {
        let epoch = AtomicU64::new(8);
        let revision = AtomicU64::new(12);
        assert!(cancellable_task(8, &epoch, 12, &revision));
        revision.store(13, Ordering::Relaxed);
        assert!(!cancellable_task(8, &epoch, 12, &revision));
        revision.store(12, Ordering::Relaxed);
        epoch.store(9, Ordering::Relaxed);
        assert!(!cancellable_task(8, &epoch, 12, &revision));
    }

    #[test]
    fn dimension_and_height_changes_invalidate_old_markers_and_replies() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        let old_epoch = atlas.spec.epoch;
        atlas
            .saved
            .push(marker(1, MarkerKind::Structure, 0., 0., "overworld"));
        atlas
            .search_results
            .push(marker(2, MarkerKind::Ore, 0., 0., "overworld"));
        atlas.set_dimension(Dimension::Beneath, 96);
        assert!(atlas.spec.epoch > old_epoch);
        assert!(atlas.saved.is_empty() && atlas.search_results.is_empty());
        atlas
            .out
            .send(Reply::Structures(
                old_epoch,
                Ok(prism::StructureScan {
                    markers: vec![marker(3, MarkerKind::Structure, 0., 0., "stale")],
                    ..Default::default()
                }),
            ))
            .unwrap();
        atlas.poll();
        assert!(
            atlas.saved.is_empty(),
            "Late Overworld data must not enter the Beneath"
        );
        let before_y = atlas.spec.epoch;
        atlas.set_dimension(Dimension::Beneath, 20);
        assert!(atlas.spec.epoch > before_y);
        assert_eq!(atlas.slice_y(), 31);
        let unchanged = atlas.spec.epoch;
        atlas.set_dimension(Dimension::Beneath, 20);
        assert_eq!(
            atlas.spec.epoch, unchanged,
            "Unchanged selection must not restart work each frame"
        );
    }

    #[test]
    fn height_scrubbing_keeps_completed_feature_pins_and_resets_them_on_dimension_change() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        atlas.set_dimension(Dimension::Beneath, 96);
        let key = FeatureTileKey::at(Point::default(), 0, true, true);
        let mut pin = marker(1, MarkerKind::Structure, 8., 8., "tfg");
        pin.id = "tfg:beneath/tower@0,0".into();
        atlas.cache_feature_result(key, vec![pin], false);
        atlas.refresh_visible_features();
        let bounds = Bounds {
            min_x: 0.,
            min_z: 0.,
            max_x: 16.,
            max_z: 16.,
        };
        for y in [0, 80, 128, 240] {
            atlas.set_dimension(Dimension::Beneath, y);
            assert_eq!(atlas.feature_tiles.len(), 1, "Y={y}");
            assert_eq!(atlas.feature_lru.len(), 1, "Y={y}");
            assert_eq!(atlas.predicted.len(), 1, "Y={y}");
            let visible = atlas.visible_markers(bounds, false, true);
            assert_eq!(visible.len(), 1, "Y={y}");
            assert_eq!(visible[0].id, "tfg:beneath/tower@0,0");
        }
        atlas.set_dimension(Dimension::Moon, Dimension::Moon.default_y());
        assert!(atlas.feature_tiles.is_empty() && atlas.predicted.is_empty());
        assert!(atlas.visible_markers(bounds, false, true).is_empty());
    }

    #[test]
    fn height_scrubbing_retains_saved_index_and_accepts_its_inflight_scan() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        atlas.set_dimension(Dimension::Beneath, 96);
        atlas.set_world(7, Settings::default(), Some(PathBuf::from("test-save")));
        let scan_epoch = atlas.scan_epoch;
        atlas
            .saved
            .push(marker(1, MarkerKind::Structure, 0., 0., "nether"));
        atlas.saved_index = MarkerIndex::build(&atlas.saved);
        for y in [90, 80, 20, 195] {
            atlas.set_dimension(Dimension::Beneath, y);
        }
        assert_eq!(
            atlas.scan_epoch, scan_epoch,
            "Y changes must not restart the save scan"
        );
        assert_eq!(atlas.saved.len(), 1);
        atlas
            .out
            .send(Reply::Structures(
                scan_epoch,
                Ok(prism::StructureScan {
                    markers: vec![marker(2, MarkerKind::Structure, 8., 8., "nether")],
                    ..Default::default()
                }),
            ))
            .unwrap();
        atlas.poll();
        assert_eq!(
            atlas.saved[0].id, "2",
            "The original scan remains valid at a new height"
        );
        assert!(!atlas.loading_scan);
        atlas.set_dimension(Dimension::Overworld, 64);
        atlas
            .out
            .send(Reply::Structures(
                scan_epoch,
                Ok(prism::StructureScan {
                    markers: vec![marker(3, MarkerKind::Structure, 8., 8., "nether")],
                    ..Default::default()
                }),
            ))
            .unwrap();
        atlas.poll();
        assert!(
            atlas.saved.is_empty(),
            "A real dimension switch invalidates the old scan"
        );
    }

    fn marker(id: usize, kind: MarkerKind, x: f64, z: f64, namespace: &str) -> Marker {
        Marker {
            id: format!("{id}"),
            label: format!("marker {id}"),
            namespace: namespace.to_owned(),
            x,
            z,
            y: None,
            kind,
            confirmed: false,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [255, 255, 255],
            bounds: None,
        }
    }

    fn world_location(
        id: usize,
        dimension: &str,
        kind: MarkerKind,
        x: f64,
        z: f64,
        y: f64,
    ) -> crate::model::WorldLocation {
        let mut marker = marker(id, kind, x, z, "world-info");
        marker.y = Some(y);
        crate::model::WorldLocation {
            dimension: dimension.into(),
            marker,
        }
    }

    #[test]
    fn world_locations_follow_dimension_and_survive_beneath_slice_changes() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        atlas.set_world_locations(vec![
            world_location(1, "minecraft:overworld", MarkerKind::Spawn, 2., 3., 64.),
            world_location(2, "minecraft:the_nether", MarkerKind::Player, 8., 9., -20.),
        ]);
        assert_eq!(
            atlas
                .markers()
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["1"]
        );

        atlas.set_dimension(Dimension::Beneath, 200);
        assert_eq!(
            atlas
                .markers()
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["2"]
        );
        assert_eq!(atlas.markers()[0].y, Some(-20.));
        atlas.set_dimension(Dimension::Beneath, 32);
        assert_eq!(
            atlas
                .markers()
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["2"]
        );
        atlas.set_dimension(Dimension::Overworld, 64);
        assert_eq!(
            atlas
                .markers()
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["1"]
        );
    }

    #[test]
    fn world_locations_are_spatially_visible_with_ore_and_structure_layers_off() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        atlas.set_world_locations(vec![
            world_location(1, "minecraft:overworld", MarkerKind::Spawn, 5., 6., 64.),
            world_location(
                2,
                "minecraft:overworld",
                MarkerKind::Player,
                500.,
                600.,
                64.,
            ),
        ]);
        atlas.set_marker_options(false, false, "");
        let visible = atlas.visible_markers(
            Bounds {
                min_x: 0.,
                min_z: 0.,
                max_x: 10.,
                max_z: 10.,
            },
            false,
            false,
        );
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].kind, MarkerKind::Spawn);
    }

    #[test]
    fn world_locations_clear_on_world_switch_and_participate_in_everything_search() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        atlas.set_world_locations(vec![world_location(
            1,
            "minecraft:overworld",
            MarkerKind::Spawn,
            10.,
            0.,
            64.,
        )]);
        atlas.find(
            Point { x: 0., z: 0. },
            100.,
            "marker 1".into(),
            false,
            crate::search::Scope::All,
        );
        assert_eq!(atlas.search_results().len(), 1);
        assert_eq!(atlas.search_results()[0].kind, MarkerKind::Spawn);

        atlas.set_world(8, Settings::default(), None);
        assert!(atlas.markers().is_empty());
        assert!(atlas.world_locations.is_empty());
    }

    #[test]
    fn saved_marker_index_preserves_namespace_and_distance_dedup_rules() {
        let saved = vec![
            marker(0, MarkerKind::Structure, 0., 0., "mod:a"),
            marker(1, MarkerKind::Structure, 0., 0., "mod:b"),
        ];
        let index = MarkerIndex::build(&saved);
        assert!(
            index.has_structure_near(&saved, &marker(2, MarkerKind::Structure, 31.9, 0., "mod:a"))
        );
        assert!(
            !index.has_structure_near(&saved, &marker(3, MarkerKind::Structure, 32., 0., "mod:a"))
        );
        assert!(
            !index.has_structure_near(&saved, &marker(4, MarkerKind::Structure, 1., 0., "mod:c"))
        );
    }

    #[test]
    fn visible_marker_query_returns_every_marker_inside_bounds() {
        let markers: Vec<_> = (0..7000)
            .map(|i| marker(i, MarkerKind::Ore, i as f64, 0., "ore"))
            .collect();
        let index = MarkerIndex::build(&markers);
        let visible = index.visible(
            &markers,
            Bounds {
                min_x: -1.,
                min_z: -1.,
                max_x: 7000.,
                max_z: 1.,
            },
            true,
            true,
        );
        assert_eq!(visible.len(), 7000);
        assert_eq!(visible[0].x, 0.);
        assert_eq!(visible[6999].x, 6999.);
    }

    #[test]
    fn structure_footprint_remains_visible_when_its_start_is_outside_the_view() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        let mut structure = marker(1, MarkerKind::Structure, 1000., 0., "saved");
        structure.bounds = Some(crate::model::StructureBounds {
            min_x: 0,
            min_y: 32,
            min_z: 0,
            max_x: 1100,
            max_y: 180,
            max_z: 16,
        });
        atlas.markers.push(structure);
        atlas.marker_index = MarkerIndex::build(&atlas.markers);
        let view = Bounds {
            min_x: -10.,
            min_z: -10.,
            max_x: 10.,
            max_z: 10.,
        };
        assert_eq!(atlas.visible_markers(view, false, true).len(), 1);
        assert!(atlas.visible_markers(view, true, false).is_empty());
    }

    #[test]
    fn structure_marker_is_indexed_at_bounds_center_while_start_stays_identity_anchor() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        let mut structure = marker(1, MarkerKind::Structure, 10_000., 10_000., "saved");
        structure.bounds = Some(crate::model::StructureBounds {
            min_x: 0,
            min_y: 0,
            min_z: 0,
            max_x: 20,
            max_y: 20,
            max_z: 20,
        });
        assert_eq!(
            structure.point(),
            Point {
                x: 10_000.,
                z: 10_000.
            }
        );
        assert_eq!(structure.map_point(), Point { x: 10.5, z: 10.5 });
        atlas.markers.push(structure.clone());
        atlas.marker_index = MarkerIndex::build(&atlas.markers);

        let view = Bounds {
            min_x: 9.,
            min_z: 9.,
            max_x: 12.,
            max_z: 12.,
        };
        let visible = atlas.visible_markers(view, false, true);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].point(), structure.point());
        assert_eq!(visible[0].map_point(), Point { x: 10.5, z: 10.5 });
        assert_eq!(visible[0].key(), structure.key());
    }

    #[test]
    fn saved_structure_dedup_uses_start_anchor_even_when_bounds_centers_differ() {
        let mut saved = marker(1, MarkerKind::Structure, 100., 100., "mod:a");
        saved.bounds = Some(crate::model::StructureBounds {
            min_x: 10_000,
            min_y: 0,
            min_z: 10_000,
            max_x: 10_100,
            max_y: 20,
            max_z: 10_100,
        });
        let index = MarkerIndex::build(&[saved.clone()]);
        let mut predicted = marker(2, MarkerKind::Structure, 120., 100., "mod:a");
        predicted.bounds = Some(crate::model::StructureBounds {
            min_x: -10_000,
            min_y: 0,
            min_z: -10_000,
            max_x: -9_900,
            max_y: 20,
            max_z: -9_900,
        });
        assert!(index.has_structure_near(&[saved], &predicted));
    }

    #[test]
    fn feature_tiles_cover_the_view_and_reuse_overlap_after_a_pan() {
        let bounds = Bounds {
            min_x: -3000.,
            min_z: -2000.,
            max_x: 3000.,
            max_z: 2000.,
        };
        let first = visible_feature_tiles(bounds, Point::default(), 4, true, true);
        assert_eq!(first.len(), 24);
        let coords: HashSet<_> = first.iter().map(|key| (key.x, key.z)).collect();
        assert!(coords.contains(&(-3, -2)));
        assert!(coords.contains(&(2, 1)));
        assert!(coords.contains(&(-3, 1)));
        assert!(coords.contains(&(2, -2)));

        let shifted = Bounds {
            min_x: -2500.,
            min_z: -2000.,
            max_x: 3500.,
            max_z: 2000.,
        };
        let after_pan = visible_feature_tiles(shifted, Point { x: 500., z: 0. }, 4, true, true);
        let reused = after_pan
            .iter()
            .filter(|key| coords.contains(&(key.x, key.z)))
            .count();
        assert!(
            reused >= 24,
            "panning should retain cached interior feature tiles"
        );

        let coarse = Bounds {
            min_x: -1700. * 2048. / 2.,
            min_z: -1000. * 2048. / 2.,
            max_x: 1700. * 2048. / 2.,
            max_z: 1000. * 2048. / 2.,
        };
        let planning_started = Instant::now();
        let broad = visible_feature_tiles(coarse, Point::default(), 12, true, true);
        assert!(planning_started.elapsed() < Duration::from_millis(200));
        assert!(
            broad.len() <= 300,
            "wide zoom should plan a small bounded tile grid"
        );
        assert_eq!(
            features::lod_tier(4096.),
            13,
            "automatic markers are hidden at maximum zoom-out"
        );
        assert_eq!(features::lod_max_blocks_per_point(13), 0.0);

        let root = FeatureTileKey::at(
            Point {
                x: -100_000.,
                z: 100_000.,
            },
            12,
            true,
            true,
        );
        let children = root.children();
        assert!(children.iter().all(|child| child.root() == root));
        let root_bounds = root.bounds();
        assert_eq!(
            children
                .iter()
                .map(|child| {
                    let bounds = child.bounds();
                    (bounds.max_x - bounds.min_x) * (bounds.max_z - bounds.min_z)
                })
                .sum::<f64>(),
            (root_bounds.max_x - root_bounds.min_x) * (root_bounds.max_z - root_bounds.min_z)
        );
    }

    #[test]
    fn feature_scans_start_as_soon_as_a_quarter_of_map_tiles_are_ready() {
        assert!(!feature_scans_ready(100, 0));
        assert!(!feature_scans_ready(100, 24));
        assert!(feature_scans_ready(100, 25));
        assert!(feature_scans_ready(1, 1));
    }

    #[test]
    fn feature_coverage_reuses_fine_spatial_results_across_lod_roots() {
        let coarse = FeatureTileKey::at(Point { x: 0., z: 0. }, 2, true, true);
        let mut cached = HashMap::new();
        for fine in coarse.children() {
            cached.insert(
                fine,
                FeatureTileData {
                    markers: Vec::new(),
                    scan_tier: 1,
                },
            );
        }
        assert!(
            coarse
                .children()
                .iter()
                .all(|key| region_covered_at_tier(*key, &cached))
        );
        assert!(
            next_uncovered_feature_tile(coarse, &HashSet::new(), &cached, coarse.center())
                .is_none(),
            "a coarser zoom should reuse the union of complete fine scans"
        );
        let ores_only: HashMap<_, _> = cached
            .iter()
            .map(|(key, value)| {
                let key = FeatureTileKey {
                    structures: false,
                    ..*key
                };
                (
                    key,
                    FeatureTileData {
                        markers: Vec::new(),
                        scan_tier: value.scan_tier,
                    },
                )
            })
            .collect();
        assert!(
            next_uncovered_feature_tile(coarse, &HashSet::new(), &ores_only, coarse.center(),)
                .is_some(),
            "an ore-only scan cannot satisfy requested structure coverage"
        );

        let fine = FeatureTileKey::at(Point { x: 0., z: 0. }, 1, true, true);
        let mut coarse_only = HashMap::new();
        coarse_only.insert(
            coarse,
            FeatureTileData {
                markers: Vec::new(),
                scan_tier: 2,
            },
        );
        let enrichment =
            next_uncovered_feature_tile(fine, &HashSet::new(), &coarse_only, fine.center());
        assert!(enrichment.is_some(), "zooming in needs finer scan coverage");
        coarse_only.insert(
            fine,
            FeatureTileData {
                markers: Vec::new(),
                scan_tier: 1,
            },
        );
        assert!(
            next_uncovered_feature_tile(fine, &HashSet::new(), &coarse_only, fine.center())
                .is_none(),
            "the enriched fine result must be reused when zooming back in"
        );
    }

    #[test]
    fn adjacent_feature_tile_edges_do_not_trigger_recursive_coverage_walks() {
        let neighbor = FeatureTileKey::at(Point { x: 0., z: 0. }, 3, true, true);
        let requested = FeatureTileKey::at(Point { x: 512., z: 0. }, 3, true, true);
        let mut cached = HashMap::new();
        cached.insert(
            neighbor,
            FeatureTileData {
                markers: Vec::new(),
                scan_tier: 3,
            },
        );
        let next =
            next_uncovered_feature_tile(requested, &HashSet::new(), &cached, requested.center())
                .unwrap();
        assert_eq!(next, requested, "touching edges have no shared scan area");
    }

    #[test]
    fn cold_continental_feature_roots_keep_the_adaptive_scan_size() {
        let root = FeatureTileKey::at(Point { x: 0., z: 0. }, 12, true, true);
        let started = Instant::now();
        let next =
            next_uncovered_feature_tile(root, &HashSet::new(), &HashMap::new(), root.center());
        assert_eq!(next, Some(root));
        assert!(started.elapsed() < Duration::from_millis(10));

        let split = HashSet::from([root]);
        let child = next_uncovered_feature_tile(root, &split, &HashMap::new(), root.center());
        assert!(root.children().contains(&child.unwrap()));
    }

    #[test]
    fn cached_raw_markers_survive_lod_hide_and_reappear_without_rescanning() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        atlas.bounds = Bounds {
            min_x: 0.,
            min_z: 0.,
            max_x: 16.,
            max_z: 16.,
        };
        let marker = marker(12, MarkerKind::Ore, 8., 8., "tfg");
        let mut marker = marker;
        marker.id = "normal_native_gold@0,0".into();
        atlas.feature_tiles.insert(
            FeatureTileKey::at(Point::default(), 0, true, true),
            FeatureTileData {
                markers: vec![marker],
                scan_tier: 0,
            },
        );
        atlas.refresh_visible_features();
        assert_eq!(atlas.predicted.len(), 1);
        let prefs = accurate_display();
        assert!(!prefs.marker_visible(&atlas.predicted[0], 4096.));
        assert!(prefs.marker_visible(&atlas.predicted[0], 0.5));

        atlas.feature_plan_key = Some((13, 0, 0, 0, 0, true, true));
        atlas.refresh_visible_features();
        assert_eq!(atlas.predicted.len(), 1);
        assert_eq!(
            atlas.feature_tiles.len(),
            1,
            "LOD only changes display filtering"
        );
    }

    #[test]
    fn cached_markers_entering_the_view_within_a_root_need_no_new_scan() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        atlas.bounds = Bounds {
            min_x: 0.,
            min_z: 0.,
            max_x: 16.,
            max_z: 16.,
        };
        let marker = marker(13, MarkerKind::Structure, 20., 8., "tfg");
        atlas.feature_tiles.insert(
            FeatureTileKey::at(Point::default(), 0, true, true),
            FeatureTileData {
                markers: vec![marker.clone()],
                scan_tier: 0,
            },
        );
        atlas.refresh_visible_features();
        assert!(
            atlas
                .visible_markers(atlas.bounds, true, true)
                .iter()
                .all(|visible| visible.id != marker.id)
        );

        // Pan remains inside the same feature roots, so no LOD plan change or scan is needed.
        atlas.bounds = Bounds {
            min_x: 16.,
            min_z: 0.,
            max_x: 32.,
            max_z: 16.,
        };
        assert!(
            atlas
                .visible_markers(atlas.bounds, true, true)
                .iter()
                .any(|visible| visible.id == marker.id)
        );
    }

    #[test]
    fn cached_children_under_one_lod_root_remain_available_after_same_root_pan() {
        let mut atlas = Atlas::new(&egui::Context::default(), 7, Settings::default());
        let root = FeatureTileKey::at(Point::default(), 2, true, true);
        atlas.feature_visible_keys = vec![root];
        atlas.bounds = Bounds {
            min_x: 0.,
            min_z: 0.,
            max_x: 16.,
            max_z: 16.,
        };
        let entering = marker(14, MarkerKind::Structure, 200., 8., "tfg");
        for child in root.children() {
            atlas.feature_tiles.insert(
                child,
                FeatureTileData {
                    markers: if child.x % 2 == 1 && child.z == 0 {
                        vec![entering.clone()]
                    } else {
                        Vec::new()
                    },
                    scan_tier: 1,
                },
            );
        }
        atlas.refresh_visible_features();
        assert!(
            atlas
                .visible_markers(atlas.bounds, true, true)
                .iter()
                .all(|visible| visible.id != entering.id)
        );

        atlas.bounds = Bounds {
            min_x: 192.,
            min_z: 0.,
            max_x: 208.,
            max_z: 16.,
        };
        assert!(
            atlas
                .visible_markers(atlas.bounds, true, true)
                .iter()
                .any(|visible| visible.id == entering.id),
            "the canonical cache keeps sibling-child markers indexed during same-root pans"
        );
    }

    #[test]
    fn truncated_feature_tiles_progress_through_all_children() {
        let root = FeatureTileKey::at(
            Point {
                x: -100_000.,
                z: 100_000.,
            },
            12,
            true,
            true,
        );
        let mut splits = HashSet::from([root]);
        for child in root.children() {
            splits.insert(child);
        }
        let mut cached = HashMap::new();
        let mut leaves = HashSet::new();
        while let Some(next) = next_missing_feature_tile(root, &splits, &cached, root.center()) {
            assert!(leaves.insert((next.x, next.z, next.scale)));
            cached.insert(next, Vec::new());
        }
        assert_eq!(leaves.len(), 16);
        let area: f64 = leaves
            .iter()
            .map(|&(x, z, scale)| {
                let tile = FeatureTileKey {
                    x,
                    z,
                    scale,
                    ..root
                };
                let bounds = tile.bounds();
                (bounds.max_x - bounds.min_x) * (bounds.max_z - bounds.min_z)
            })
            .sum();
        let bounds = root.bounds();
        assert_eq!(
            area,
            (bounds.max_x - bounds.min_x) * (bounds.max_z - bounds.min_z)
        );
    }

    #[test]
    fn feature_predictions_match_small_tile_union_at_zoom_and_pan_edges() {
        for bpp in [16., 32.] {
            let tier = features::lod_tier(bpp);
            let base_span = FeatureTileKey::span_for(tier);
            let scale = (base_span / 512.).log2().round() as u8;
            let factor = 1_i64 << scale;
            let mut world = World::new(7787531943268589495, Settings::default());
            for point in [
                Point {
                    x: -16_384.,
                    z: -8_192.,
                },
                Point {
                    x: 36_864.,
                    z: 28_672.,
                },
            ] {
                let base = FeatureTileKey::at(point, tier, true, true);
                let leaf = FeatureTileKey {
                    x: base.x * factor,
                    z: base.z * factor,
                    scale,
                    ..base
                };
                let full =
                    features::predict_visible(&mut world, leaf.bounds(), true, true, bpp, &|| {
                        false
                    });
                assert!(
                    !full.coverage.truncated,
                    "512-block feature leaf at {bpp} bpp"
                );
                let expected: HashSet<_> = full
                    .markers
                    .iter()
                    .map(|marker| marker.id.clone())
                    .collect();
                let mut actual = HashSet::new();
                for child in leaf.children() {
                    let batch = features::predict_visible(
                        &mut world,
                        child.bounds(),
                        true,
                        true,
                        bpp,
                        &|| false,
                    );
                    assert!(
                        !batch.coverage.truncated,
                        "256-block feature leaf at {bpp} bpp"
                    );
                    actual.extend(batch.markers.into_iter().map(|marker| marker.id));
                }
                assert_eq!(actual, expected, "bpp={bpp}, pan={point:?}");
            }
        }
    }

    #[test]
    fn region_hover_grid_is_bounded_for_large_and_wide_windows() {
        for (width, height) in [(1440., 900.), (7680., 4320.), (20_000., 300.)] {
            let (_, cols, rows) = region_grid(width, height);
            assert!(cols > 0 && rows > 0);
            assert!(cols.saturating_mul(rows) <= MAX_REGION_CELLS);
        }
    }

    fn test_key(x: i32, z: i32, zoom: i32, detail: bool) -> TileKey {
        TileKey {
            x,
            z,
            zoom,
            detail,
            layer: Layer::Temperature,
        }
    }

    #[test]
    fn offscreen_prefetch_does_not_count_as_visible_loading() {
        let bounds = Bounds {
            min_x: 0.,
            min_z: 0.,
            max_x: 256.,
            max_z: 256.,
        };
        let inside = test_key(0, 0, 0, true);
        assert!(tile_intersects_view(inside, bounds, 0, Layer::Temperature));
        assert!(!tile_intersects_view(
            test_key(1, 0, 0, false),
            bounds,
            0,
            Layer::Temperature
        ));
        assert!(!tile_intersects_view(
            test_key(-1, 0, 0, false),
            bounds,
            0,
            Layer::Temperature
        ));
        assert!(!tile_intersects_view(inside, bounds, 1, Layer::Temperature));
        assert!(!tile_intersects_view(inside, bounds, 0, Layer::Biomes));
    }

    #[test]
    fn spatial_traversal_visits_every_padded_texel_once_and_keeps_small_neighbours_together() {
        for detail in [false, true] {
            let side = if detail { DETAIL + 2 } else { COARSE + 2 };
            let order = tile_sample_order(detail);
            assert_eq!(order.len(), side * side);
            assert_eq!(
                order.iter().copied().collect::<HashSet<_>>().len(),
                side * side
            );
            assert!(order.iter().all(|&(x, z)| x < side && z < side));
            assert_eq!(&order[..4], &[(0, 0), (1, 0), (0, 1), (1, 1)]);
            assert!(order[..16].iter().all(|&(x, z)| x < 4 && z < 4));
        }
    }

    #[test]
    fn slow_preview_does_not_block_other_tiles_from_refining() {
        let a = test_key(0, 0, 0, false);
        let af = TileKey { detail: true, ..a };
        let b = test_key(1, 0, 0, false);
        let bf = TileKey { detail: true, ..b };
        let requests = refinement_requests(&[(0., a, af), (1., b, bf)], |key| *key == b);
        assert_eq!(requests, [a, bf]);
        let requests = refinement_requests(&[(0., a, af), (1000., b, bf)], |key| *key == a);
        assert_eq!(
            requests[0], af,
            "central detail must progress ahead of distant missing previews"
        );
        let requests =
            refinement_requests(&[(0., a, af), (1., b, bf)], |key| *key == af || *key == bf);
        assert!(
            requests.is_empty(),
            "fine coverage must never regenerate a preview"
        );
    }

    #[test]
    fn neighbouring_tile_gutters_share_the_same_world_sample_positions() {
        for resolution in [COARSE, DETAIL] {
            for zoom in [-3, 0, 5, 12] {
                for index in [-17, -1, 0, 13] {
                    let key = test_key(index, index, zoom, false);
                    let east = TileKey {
                        x: index + 1,
                        ..key
                    };
                    let south = TileKey {
                        z: index + 1,
                        ..key
                    };
                    for i in -1..=resolution as i32 {
                        assert_eq!(
                            tile_sample(key, resolution, resolution as i32 - 1, i),
                            tile_sample(east, resolution, -1, i)
                        );
                        assert_eq!(
                            tile_sample(key, resolution, resolution as i32, i),
                            tile_sample(east, resolution, 0, i)
                        );
                        assert_eq!(
                            tile_sample(key, resolution, i, resolution as i32 - 1),
                            tile_sample(south, resolution, i, -1)
                        );
                        assert_eq!(
                            tile_sample(key, resolution, i, resolution as i32),
                            tile_sample(south, resolution, i, 0)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn close_detail_tiles_keep_one_texel_per_block_and_seamless_gutters() {
        for bpp in [0.125, 0.5, 1.0, 1.414, 1.999, 2.0] {
            let zoom = tile_zoom_for_bpp(bpp);
            assert_eq!(zoom, 0, "close zoom bpp={bpp}");
            let key = test_key(-7, -13, zoom, true);
            assert_eq!(key.span() / DETAIL as f64, 1.0);
            for i in -1..=DETAIL as i32 {
                let east = TileKey {
                    x: key.x + 1,
                    ..key
                };
                let south = TileKey {
                    z: key.z + 1,
                    ..key
                };
                assert_eq!(
                    tile_sample(key, DETAIL, DETAIL as i32 - 1, i),
                    tile_sample(east, DETAIL, -1, i)
                );
                assert_eq!(
                    tile_sample(key, DETAIL, i, DETAIL as i32 - 1),
                    tile_sample(south, DETAIL, i, -1)
                );
            }
        }
        assert_eq!(tile_zoom_for_bpp(2.001), 1);
        assert_eq!(tile_zoom_for_bpp(4.0), 2);
    }

    #[test]
    fn tile_filter_defaults_to_nearest_and_can_smooth_loading_previews() {
        let close = test_key(0, 0, 0, true);
        let coarse = TileKey {
            detail: false,
            ..close
        };
        let broad = test_key(0, 0, 2, true);
        for key in [close, coarse, broad] {
            assert_eq!(
                tile_texture_options(key, LoadingFilter::default()),
                TextureOptions::NEAREST
            );
        }
        assert_eq!(
            tile_texture_options(close, LoadingFilter::Smooth),
            TextureOptions::NEAREST
        );
        assert_eq!(
            tile_texture_options(coarse, LoadingFilter::Smooth),
            TextureOptions::LINEAR
        );
        assert_eq!(
            tile_texture_options(broad, LoadingFilter::Smooth),
            TextureOptions::LINEAR
        );
    }

    #[test]
    fn detail_limit_caps_sampling_without_limiting_zoom() {
        for detail in MapDetail::ALL {
            for bpp in [0.125, 0.5, 1., 2., 2.001, 4., 7.99, 32., 4096.] {
                let zoom = tile_zoom_for_detail(bpp, detail);
                let spacing = test_key(0, 0, zoom, true).span() / DETAIL as f64;
                let limit = 2_f64.powi(detail.min_tile_zoom());
                assert!(spacing >= limit);
                if bpp <= 2. {
                    assert_eq!(spacing, limit);
                }
                assert!(zoom >= tile_zoom_for_bpp(bpp));
            }
        }
    }

    #[test]
    fn filter_and_detail_changes_keep_tiles_and_feature_placements() {
        let ctx = egui::Context::default();
        let mut atlas = Atlas::new(&ctx, 0, Settings::default());
        let key = test_key(0, 0, 0, false);
        let corner = Color32::from_rgb(20, 50, 80);
        let texture = ctx.load_texture(
            "test-cached-map",
            ColorImage::new([2, 2], vec![corner; 4]),
            TextureOptions::NEAREST,
        );
        let id = texture.id();
        atlas.tiles.put(
            key,
            GpuTile {
                texture,
                corner,
                resolution: 0,
                biomes: vec![],
                surface_water: vec![],
            },
        );
        let _ = ctx.tex_manager().write().take_delta();
        let epoch = atlas.spec.epoch;
        let feature_serial = atlas.feature_serial;
        let search_serial = atlas.search_revision.load(Ordering::Relaxed);
        let mut prefs = atlas.spec.display.clone();
        prefs.loading_filter = LoadingFilter::Smooth;
        prefs.max_detail = MapDetail::FourBlocks;
        atlas.set_display_options(&prefs);
        assert_eq!(atlas.spec.epoch, epoch);
        assert_eq!(atlas.feature_serial, feature_serial);
        assert_eq!(atlas.search_revision.load(Ordering::Relaxed), search_serial);
        assert_eq!(atlas.tiles.peek(&key).unwrap().texture.id(), id);
        let delta = ctx.tex_manager().write().take_delta();
        assert_eq!(delta.set.len(), 1);
        assert_eq!(delta.set[0].0, id);
        assert_eq!(delta.set[0].1.pos, Some([0, 0]));
        assert_eq!(delta.set[0].1.image.size(), [1, 1]);
        assert_eq!(delta.set[0].1.options, TextureOptions::LINEAR);
        assert_eq!(atlas.spec.display, prefs);
    }

    #[test]
    fn detail_limit_excludes_finer_cached_tiles_from_semantics_and_coverage() {
        let ctx = egui::Context::default();
        let mut atlas = Atlas::new(&ctx, 0, Settings::default());
        let key = TileKey {
            layer: Layer::Biomes,
            ..test_key(0, 0, 2, true)
        };
        let corner = Color32::WHITE;
        for z in 0..2 {
            for x in 0..2 {
                let child = TileKey {
                    zoom: 1,
                    x,
                    z,
                    ..key
                };
                atlas.tiles.put(
                    child,
                    GpuTile {
                        texture: ctx.load_texture(
                            "cached-child",
                            ColorImage::new([3, 3], vec![corner; 9]),
                            TextureOptions::NEAREST,
                        ),
                        corner,
                        resolution: 1,
                        biomes: vec![biome::ids::id::PLAINS],
                        surface_water: vec![false],
                    },
                );
            }
        }
        let point = Point { x: 1., z: 1. };
        assert!(
            atlas
                .semantic_sample_at_cached(point, 2, Layer::Biomes)
                .is_some()
        );
        assert!(atlas.tile_has_coverage(key, 0));
        atlas.spec.display.max_detail = MapDetail::FourBlocks;
        assert_eq!(
            atlas.semantic_sample_at_cached(point, 2, Layer::Biomes),
            None
        );
        assert!(!atlas.tile_has_coverage(key, 0));
    }

    #[test]
    fn hillshade_shades_ocean_terrain_and_leaves_missing_height_unchanged() {
        let ocean = Color32::from_rgb(31, 77, 104);
        let slope = [Some(20.), Some(10.), Some(0.)];
        let shaded_ocean = apply_hillshade(ocean, slope, 1.0, 0.8);
        assert_ne!(shaded_ocean, ocean, "underwater relief changes brightness");
        assert!(
            shaded_ocean.b() > shaded_ocean.r(),
            "underwater relief retains a blue water hue"
        );
        let land = Color32::from_rgb(140, 130, 120);
        assert_eq!(
            apply_hillshade(land, [Some(0.), None, Some(0.)], 1.0, 0.8),
            land,
            "missing height leaves the source pixel unchanged"
        );
    }

    #[test]
    fn shaded_adjacent_tile_gutters_have_identical_shared_texels() {
        let resolution = DETAIL;
        let key = test_key(-3, -5, 0, true);
        let east = TileKey {
            x: key.x + 1,
            ..key
        };
        let south = TileKey {
            z: key.z + 1,
            ..key
        };
        let make_height_grid = |key: TileKey| {
            let side = resolution + 4;
            (0..side * side)
                .map(|index| {
                    let x = index % side;
                    let z = index / side;
                    let (wx, wz) = tile_sample(key, resolution, x as i32 - 2, z as i32 - 2);
                    Some(wx as f64 * 0.07 + wz as f64 * 0.11)
                })
                .collect::<Vec<_>>()
        };
        let left_heights = make_height_grid(key);
        let right_heights = make_height_grid(east);
        let north_heights = make_height_grid(key);
        let south_heights = make_height_grid(south);
        let base = Color32::from_rgb(130, 145, 110);
        for i in 0..=resolution + 1 {
            let water = Color32::from_rgb(43, 105, 133);
            let right = shade_tile_pixel(water, &right_heights, resolution + 4, 0, i, 1.0, 0.6);
            let left_water = shade_tile_pixel(
                water,
                &left_heights,
                resolution + 4,
                resolution + 1,
                i,
                1.0,
                0.6,
            );
            assert_eq!(
                left_water, right,
                "water shades continuously across east edge"
            );
            let north_water = shade_tile_pixel(
                water,
                &north_heights,
                resolution + 4,
                i,
                resolution + 1,
                1.0,
                0.6,
            );
            let south_water =
                shade_tile_pixel(water, &south_heights, resolution + 4, i, 0, 1.0, 0.6);
            assert_eq!(
                north_water, south_water,
                "water shades continuously across south edge"
            );
            let top = shade_tile_pixel(
                base,
                &north_heights,
                resolution + 4,
                i,
                resolution,
                1.0,
                0.6,
            );
            let bottom = shade_tile_pixel(base, &south_heights, resolution + 4, i, 0, 1.0, 0.6);
            assert_eq!(top, bottom, "north/south seam at padded texel {i}");
        }
    }

    #[test]
    fn contours_join_at_shared_tile_samples_in_both_axes() {
        let prefs = crate::contours::ContourPrefs {
            enabled: true,
            interval: 5,
            ..Default::default()
        };
        let base = Color32::from_rgb(130, 145, 110);
        let mut line_samples = 0;
        let mut clear_samples = 0;
        for (zoom, detail) in [(0, true), (3, false)] {
            let resolution = if detail { DETAIL } else { COARSE };
            let side = resolution + 4;
            let key = test_key(-3, -5, zoom, detail);
            let east = TileKey {
                x: key.x + 1,
                ..key
            };
            let south = TileKey {
                z: key.z + 1,
                ..key
            };
            let make_grid = |key| {
                (0..side * side)
                    .map(|index| {
                        let (x, z) = tile_sample(
                            key,
                            resolution,
                            (index % side) as i32 - 2,
                            (index / side) as i32 - 2,
                        );
                        Some(70. + 8. * (x as f64 * 0.005).sin() + 4. * (z as f64 * 0.007).cos())
                    })
                    .collect::<Vec<_>>()
            };
            let grid = make_grid(key);
            let east_grid = make_grid(east);
            let south_grid = make_grid(south);
            for i in 0..resolution + 2 {
                for gutter in 0..=1 {
                    let left_x = resolution + gutter;
                    assert_eq!(
                        tile_sample(key, resolution, left_x as i32 - 1, i as i32 - 1),
                        tile_sample(east, resolution, gutter as i32 - 1, i as i32 - 1),
                    );
                    let left = contour_tile_pixel(base, &grid, side, left_x, i, &prefs);
                    let right = contour_tile_pixel(base, &east_grid, side, gutter, i, &prefs);
                    assert_eq!(left, right, "east/west contour seam at {i},{gutter}");
                    let top = contour_tile_pixel(base, &grid, side, i, left_x, &prefs);
                    let bottom = contour_tile_pixel(base, &south_grid, side, i, gutter, &prefs);
                    assert_eq!(top, bottom, "north/south contour seam at {i},{gutter}");
                    line_samples += usize::from(left != base) + usize::from(top != base);
                    clear_samples += usize::from(left == base) + usize::from(top == base);
                }
            }
        }
        assert!(
            line_samples > 0,
            "exercise visible contours at tile boundaries"
        );
        assert!(
            clear_samples > 0,
            "contours must leave terrain between lines clear"
        );
    }

    #[test]
    fn contour_tiles_and_export_pixels_agree_and_respect_underwater_visibility() {
        let world = World::new(0, Settings::default());
        let key = test_key(-3, 4, 2, false);
        let mut prefs = DisplayPrefs {
            height_shading: true,
            ..accurate_display()
        };
        let plain = render_tile(&world, key, &prefs, || false).unwrap();
        prefs.contours.enabled = true;
        prefs.contours.interval = 5;
        let land = render_tile(&world, key, &prefs, || false).unwrap();
        prefs.contours.underwater = true;
        let all = render_tile(&world, key, &prefs, || false).unwrap();
        assert_eq!(plain.biomes, all.biomes);
        assert_eq!(plain.surface_water, all.surface_water);
        let resolution = COARSE;
        let side = resolution + 2;
        let mut changed_land = 0;
        let mut changed_water = 0;
        for z in 1..=resolution {
            for x in 1..=resolution {
                let index = z * side + x;
                if plain.surface_water[(z - 1) * resolution + x - 1] {
                    assert_eq!(land.pixels[index], plain.pixels[index]);
                    changed_water += usize::from(all.pixels[index] != plain.pixels[index]);
                } else {
                    changed_land += usize::from(land.pixels[index] != plain.pixels[index]);
                }
                if index.is_multiple_of(37) {
                    let (bx, bz) = tile_sample(key, resolution, x as i32 - 1, z as i32 - 1);
                    let exported = raster_color_with_display_step(
                        &world,
                        key.layer,
                        bx,
                        bz,
                        &prefs,
                        key.span() / resolution as f64,
                    );
                    let pixel = all.pixels[index];
                    assert_eq!(exported, [pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
                }
            }
        }
        assert!(
            changed_land > 0,
            "land contours must be visible without changing biomes"
        );
        assert!(
            changed_water > 0,
            "underwater toggle must expose seafloor contours"
        );
    }

    #[test]
    fn seed_zero_ocean_surface_uses_underwater_relief() {
        let world = World::new(0, Settings::default());
        let (x, z) = (-2345, -678);
        assert!(world.probe(x, z).is_water());
        assert!(world.height(x, z).unwrap() < 63.);
        let mut shaded = accurate_display();
        shaded.height_shading = true;
        shaded.hillshade_strength = 0.8;
        let mut flat = shaded.clone();
        flat.height_shading = false;
        for layer in Layer::ALL {
            let under_relief = raster_color_with_display_step(&world, layer, x, z, &shaded, 16.);
            let water_base = raster_color_with_display_step(&world, layer, x, z, &flat, 16.);
            assert_ne!(
                under_relief, water_base,
                "underwater ground is shaded for {layer:?}"
            );
            assert!(
                under_relief[2] > under_relief[0],
                "water hue remains blue for {layer:?}"
            );
        }
    }

    #[test]
    fn natural_hot_spring_water_is_part_of_every_map_layer() {
        let world = World::new(0, Settings::default());
        let (x, z) = (14386, 13643);
        let spring = crate::hot_springs::sample(&world, x, z).expect("seed-0 active spring");
        assert_eq!(spring.block.as_ref(), "tfc:fluid/spring_water");
        assert_eq!(spring.y, 96);
        assert_eq!(
            world.volcanic_surface(x, z),
            Some(crate::volcanic_lava::Surface::Water(96))
        );
        let probe = world.probe(x, z);
        assert!(probe.is_water() && probe.is_fluid());
        assert_eq!(probe.surface_water_y, Some(96));
        let bed = crate::ground::pre_forest_sample(&world, x, z).unwrap();
        assert_eq!(
            bed.y, 95,
            "forest origins use the pool bed, not the later surface-water plane"
        );
        assert!(bed.block.starts_with("tfc:rock/gravel/"));
        let display = accurate_display();
        for layer in Layer::ALL {
            assert_eq!(
                raster_color_with_display(&world, layer, x, z, &display),
                [43, 105, 133, 255],
                "natural spring water on {layer:?}"
            );
        }
    }

    #[test]
    fn dry_spring_basins_affect_materials_and_relief_without_a_marker() {
        let world = World::new(0, Settings::default());
        let (x, z) = (-319245_i32, 16110_i32);
        let surface = crate::ground::sample(&world, x, z).expect("seed-0 dry spring basin");
        assert!(surface.block.starts_with("tfc:rock/gravel/"));
        assert_eq!(surface.y, 84);
        assert!(
            crate::hot_springs::sample_before_forest(&world, x, z).is_none(),
            "later dry springs must not change forest eligibility"
        );
        let mut display = DisplayPrefs {
            height_shading: true,
            ..accurate_display()
        };
        display.contours.enabled = true;
        display.contours.interval = 1;
        let key = TileKey {
            zoom: 0,
            x: x.div_euclid(256),
            z: z.div_euclid(256),
            layer: Layer::Biomes,
            detail: true,
        };
        let tile = render_tile(&world, key, &display, || false).unwrap();
        let raster = RasterSampler::new(&world, key.layer, &display, [1., 1.]);
        assert_eq!(raster.relief_height(x, z), Some(84.));
        assert!(
            world.height(x, z).unwrap() > 84.,
            "excavation must lower the terrain surface"
        );
        for (wx, wz) in [(x, z), (x - 1, z), (x + 1, z + 1)] {
            let lx = wx.rem_euclid(256) as usize;
            let lz = wz.rem_euclid(256) as usize;
            assert_eq!(
                tile.pixels[(lz + 1) * (DETAIL + 2) + lx + 1].to_array(),
                raster.color(wx, wz, &|| false)
            );
        }
    }

    #[test]
    fn volcanic_lava_keeps_its_surface_color_on_every_layer_and_overlay() {
        let world = World::new(0, Settings::default());
        let (x, z) = (5511, -6368);
        let probe = world.probe(x, z);
        assert_eq!(probe.surface_lava, Some(92));
        assert!(!probe.is_water() && probe.is_fluid());
        assert_eq!(probe.height, Some(91.));
        let mut prefs = DisplayPrefs {
            height_shading: true,
            hillshade_strength: 1.,
            palette: crate::display::Palette::Colorblind,
            temperature_enabled: true,
            temperature_min: -30.,
            temperature_max: -29.,
            ..accurate_display()
        };
        prefs.contours.enabled = true;
        prefs.contours.interval = 1;
        prefs.contours.underwater = true;
        for layer in Layer::ALL {
            let rgb = crate::volcanic_lava::COLOR;
            assert_eq!(
                raster_color_with_display(&world, layer, x, z, &prefs),
                [rgb[0], rgb[1], rgb[2], 255],
                "lava must stay visible on {layer:?}, including highlighted/colorblind layers",
            );
        }
        assert_eq!(world.tree_estimate(&probe, 10_000.).total, 0.);
        assert!(world.is_surface_fluid(x, z));
        assert!(!world.is_surface_water(x, z));
        for dimension in [Dimension::Moon, Dimension::Mars, Dimension::Beneath] {
            assert!(
                World::for_dimension(0, Settings::default(), dimension, 96)
                    .surface_lava_y(x, z)
                    .is_none()
            );
        }
    }

    #[test]
    fn volcanic_lava_tile_masks_are_reused_across_layers_and_not_published_on_cancel() {
        let world = World::new(0, Settings::default());
        let key = test_key(21, -25, 0, false);
        let cache_key = HeightGridKey::new(&world, key).with_terrain(&accurate_display().terrain);
        VOLCANIC_SURFACE_CACHE.access(|cache| {
            cache.pop(&cache_key);
        });
        assert!(render_tile(&world, key, &accurate_display(), || true).is_none());
        assert!(VOLCANIC_SURFACE_CACHE.access(|cache| !cache.contains(&cache_key)));
        let mut prefs = DisplayPrefs {
            height_shading: true,
            ..accurate_display()
        };
        prefs.contours.enabled = true;
        prefs.contours.interval = 1;
        let first = render_tile(&world, key, &prefs, || false).unwrap();
        let mask = VOLCANIC_SURFACE_CACHE
            .access(|cache| cache.get(&cache_key).cloned())
            .unwrap();
        assert!(
            mask.iter().any(Option::is_some),
            "the tile must exercise a caldera"
        );
        let other = render_tile(
            &world,
            TileKey {
                layer: Layer::Rainfall,
                ..key
            },
            &prefs,
            || false,
        )
        .unwrap();
        let reused = VOLCANIC_SURFACE_CACHE
            .access(|cache| cache.get(&cache_key).cloned())
            .unwrap();
        assert!(Arc::ptr_eq(&mask, &reused));
        for (index, y) in mask.iter().enumerate() {
            if matches!(y, Some(crate::volcanic_lava::Surface::Lava(_))) {
                let expected = color_from_rgb(crate::volcanic_lava::COLOR);
                assert_eq!(first.pixels[index], expected);
                assert_eq!(other.pixels[index], expected);
            }
        }
    }

    #[test]
    fn height_grid_cache_key_reuses_layers_but_separates_world_inputs() {
        let world = World::for_dimension(
            17,
            Settings::default(),
            Dimension::Overworld,
            Dimension::Overworld.default_y(),
        );
        let tile = test_key(-2, 5, 0, true);
        let mut other_layer = TileKey {
            layer: Layer::Temperature,
            ..tile
        };
        let key = HeightGridKey::new(&world, tile);
        assert_eq!(key, HeightGridKey::new(&world, other_layer));
        let opened = World::new(17, Settings::default())
            .with_surface_source(Some(PathBuf::from("/virtual/world-0")), 1);
        let reloaded = World::new(17, Settings::default())
            .with_surface_source(Some(PathBuf::from("/virtual/world-0")), 2);
        let other_save = World::new(17, Settings::default())
            .with_surface_source(Some(PathBuf::from("/virtual/another-world")), 1);
        assert_ne!(key, HeightGridKey::new(&opened, tile));
        assert_ne!(
            HeightGridKey::new(&opened, tile),
            HeightGridKey::new(&reloaded, tile)
        );
        assert_ne!(
            HeightGridKey::new(&opened, tile),
            HeightGridKey::new(&other_save, tile)
        );
        other_layer.detail = false;
        assert_ne!(key, HeightGridKey::new(&world, other_layer));

        let other_seed = World::for_dimension(
            18,
            Settings::default(),
            Dimension::Overworld,
            Dimension::Overworld.default_y(),
        );
        assert_ne!(key, HeightGridKey::new(&other_seed, tile));
        let other_dimension = World::for_dimension(
            17,
            Settings::default(),
            Dimension::Moon,
            Dimension::Moon.default_y(),
        );
        assert_ne!(key, HeightGridKey::new(&other_dimension, tile));
        let mut changed_settings = Settings::default();
        changed_settings.continentalness += 0.01;
        let other_settings = World::for_dimension(
            17,
            changed_settings,
            Dimension::Overworld,
            Dimension::Overworld.default_y(),
        );
        assert_ne!(key, HeightGridKey::new(&other_settings, tile));
        let low_band = World::for_dimension(17, Settings::default(), Dimension::Beneath, 15);
        let high_band = World::for_dimension(17, Settings::default(), Dimension::Beneath, 95);
        assert_ne!(
            HeightGridKey::new(&low_band, tile),
            HeightGridKey::new(&high_band, tile)
        );
    }

    #[test]
    fn adjoining_rasters_share_edge_texels_and_keep_semantic_interior_unpadded() {
        let world = World::new(7787531943268589495, Settings::default());
        for detail in [false, true] {
            let key = test_key(-1, -2, 4, detail);
            let display = accurate_display();
            let left = render_tile(&world, key, &display, || false).unwrap();
            let right = render_tile(&world, TileKey { x: 0, ..key }, &display, || false).unwrap();
            let n = left.resolution;
            let side = n + 2;
            assert_eq!(left.biomes.len(), n * n);
            assert_eq!(left.pixels.len(), side * side);
            for row in 0..side {
                assert_eq!(left.pixels[row * side + n], right.pixels[row * side]);
                assert_eq!(
                    left.pixels[row * side + n + 1],
                    right.pixels[row * side + 1]
                );
            }
        }
    }

    #[test]
    fn projected_edges_match_exactly_at_fractional_zoom_and_negative_coordinates() {
        let rect = Rect::from_min_size(Pos2::new(10.25, 20.5), egui::vec2(1440., 900.));
        let center = Point {
            x: -32764.3,
            z: 9910.7,
        };
        for bpp in [0.125_f64, 0.7, 3.99, 31.9, 512., 4096.] {
            let key = test_key(-1, -1, bpp.log2().round() as i32, true);
            let left = tile_screen_rect(rect, center, bpp, key.origin(), key.span());
            let east = TileKey { x: 0, ..key };
            let south = TileKey { z: 0, ..key };
            assert_eq!(
                left.max.x,
                tile_screen_rect(rect, center, bpp, east.origin(), east.span())
                    .min
                    .x
            );
            assert_eq!(
                left.max.y,
                tile_screen_rect(rect, center, bpp, south.origin(), south.span())
                    .min
                    .y
            );
        }
    }

    #[test]
    fn completed_tiles_survive_pan_and_zoom_revisions_and_replace_their_preview() {
        let ctx = egui::Context::default();
        let mut atlas = Atlas::new(&ctx, 1, Settings::default());
        let coarse = test_key(0, 0, 0, false);
        let fine = TileKey {
            detail: true,
            ..coarse
        };
        let send = |atlas: &Atlas, key: TileKey, revision| {
            let n = if key.detail { DETAIL } else { COARSE };
            atlas
                .out
                .send(Reply::Tile(
                    atlas.spec.epoch,
                    revision,
                    CpuTile {
                        key,
                        resolution: n,
                        pixels: vec![Color32::BLACK; (n + 2) * (n + 2)],
                        biomes: vec![0; n * n],
                        surface_water: vec![false; n * n],
                    },
                ))
                .unwrap();
        };
        send(&atlas, coarse, 1);
        atlas.poll();
        assert!(atlas.tiles.contains(&coarse));
        atlas.view.fetch_add(10, Ordering::Relaxed);
        send(&atlas, fine, 1);
        atlas.poll();
        assert!(atlas.tiles.contains(&fine));
        assert!(
            !atlas.tiles.contains(&coarse),
            "preview storage must be reclaimed after refinement"
        );
        send(&atlas, coarse, 1);
        atlas.poll();
        assert!(atlas.tiles.contains(&fine));
        assert!(
            !atlas.tiles.contains(&coarse),
            "late preview cannot replace or duplicate detail"
        );
    }

    #[test]
    fn stale_world_skip_cannot_clear_a_new_tile_request() {
        let key = TileKey {
            zoom: 1,
            x: 2,
            z: -3,
            layer: Layer::Biomes,
            detail: true,
        };
        let mut pending = HashMap::from([(key, (9, 21))]);
        remove_pending_if_current(&mut pending, key, 8, 20);
        assert_eq!(pending.get(&key), Some(&(9, 21)));
        remove_pending_if_current(&mut pending, key, 9, 21);
        assert!(!pending.contains_key(&key));
    }

    #[test]
    fn skipped_tile_invalidates_an_identical_plan_so_the_same_view_retries_it() {
        let mut atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        let key = TileKey {
            zoom: 1,
            x: -2,
            z: 3,
            layer: Layer::Biomes,
            detail: false,
        };
        let wanted = HashSet::from([key]);
        assert!(atlas.schedule_tiles(wanted.clone(), vec![key]));
        assert!(
            !atlas.schedule_tiles(wanted.clone(), vec![key]),
            "the unchanged viewport should not republish a plan each frame"
        );
        let (epoch, revision) = atlas.pending[&key];
        atlas
            .out
            .send(Reply::Skipped(epoch, revision, key))
            .unwrap();
        atlas.poll();
        assert!(atlas.last_tile_plan.is_none());
        assert!(
            atlas.schedule_tiles(wanted, vec![key]),
            "an unchanged viewport must republish its skipped tile"
        );
    }

    #[test]
    fn accepted_tile_dropped_from_full_upload_queue_is_requested_again() {
        let mut atlas = Atlas::new(&egui::Context::default(), 0, Settings::default());
        let key = TileKey {
            zoom: 0,
            x: 5000,
            z: -5000,
            layer: Layer::Biomes,
            detail: false,
        };
        let wanted = HashSet::from([key]);
        assert!(atlas.schedule_tiles(wanted.clone(), vec![key]));
        let (epoch, revision) = atlas.pending[&key];
        for x in 0..TILE_CACHE as i32 {
            atlas.uploads.push_back(CpuTile {
                key: TileKey { x, z: 0, ..key },
                resolution: 1,
                pixels: vec![Color32::BLACK; 9],
                biomes: vec![0],
                surface_water: vec![false],
            });
        }
        atlas
            .out
            .send(Reply::Tile(
                epoch,
                revision,
                CpuTile {
                    key,
                    resolution: 1,
                    pixels: vec![Color32::BLACK; 9],
                    biomes: vec![0],
                    surface_water: vec![false],
                },
            ))
            .unwrap();
        atlas.poll();
        assert!(!atlas.tiles.contains(&key));
        assert!(!atlas.uploads.iter().any(|tile| tile.key == key));
        assert!(atlas.last_tile_plan.is_none());
        assert!(
            atlas.schedule_tiles(wanted, vec![key]),
            "a tile lost to upload backpressure must be enqueued again"
        );
    }

    #[test]
    fn visible_tiles_are_prioritized_by_distance_from_center() {
        let key = TileKey {
            zoom: 0,
            x: 0,
            z: 0,
            layer: Layer::Biomes,
            detail: false,
        };
        let mut visible = vec![(20., key, key), (2., key, key), (9., key, key)];
        prioritize_visible(&mut visible);
        assert_eq!(
            visible.iter().map(|v| v.0).collect::<Vec<_>>(),
            [2., 9., 20.]
        );
    }

    #[test]
    fn semantic_components_are_cached_per_revision_and_view() {
        let key = (2, 0, 0, 800, 600, Layer::Biomes);
        let cache = RegionCache {
            revision: 7,
            view: key,
            cols: 4,
            rows: 3,
            cell: 6.,
            values: vec![],
            biome_labels: vec![],
            continent_labels: vec![],
            biome_sizes: HashMap::new(),
            continent_sizes: HashMap::new(),
            biome_edges: HashSet::new(),
            continent_edges: HashSet::new(),
            overlays: HashMap::new(),
        };
        assert!(semantic_cache_valid(&cache, 7, key, 4, 3));
        assert!(!semantic_cache_valid(&cache, 8, key, 4, 3));
        assert!(!semantic_cache_valid(
            &cache,
            7,
            (2, 1, 0, 800, 600, Layer::Biomes),
            4,
            3
        ));
    }

    #[test]
    fn ocean_pixels_ignore_all_continent_layer_fields() {
        let mut probe = Probe {
            ocean: true,
            biome: "tfc:deep_ocean".into(),
            ..Probe::default()
        };
        let before = [
            Layer::Temperature,
            Layer::Rainfall,
            Layer::Forest,
            Layer::Height,
        ]
        .map(|layer| color(layer, crate::biome::ids::id::DEEP_OCEAN, &probe));
        probe.climate.temperature = 1000.;
        probe.climate.rainfall = 5000.;
        probe.forest = 4;
        probe.height = Some(300.);
        let after = [
            Layer::Temperature,
            Layer::Rainfall,
            Layer::Forest,
            Layer::Height,
        ]
        .map(|layer| color(layer, crate::biome::ids::id::DEEP_OCEAN, &probe));
        assert_eq!(before, after);
    }

    #[test]
    fn ocean_channels_split_continents_and_are_not_selectable() {
        let land = 5;
        let ocean = crate::biome::ids::id::OCEAN;
        let values = vec![Some(land), Some(land), Some(ocean), Some(land), Some(land)];
        let (labels, sizes, _) = component_labels(&values, 5, 1, true);
        assert_ne!(labels[0], labels[4]);
        assert_eq!(labels[2], 0);
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes.values().copied().collect::<Vec<_>>(), [2, 2]);
    }

    #[test]
    fn flooded_land_keeps_its_biome_but_splits_continent_selection() {
        let plains = crate::biome::ids::id::PLAINS;
        let source_sample = (plains, true);
        // The legend can still count the true generated biome ID.
        assert_eq!(source_sample.0, plains);
        // Region topology uses the independent water bit, just as ocean channels
        // are excluded, even if the generated biome itself is ordinary land.
        let values = [
            Some((plains, false)),
            Some((plains, true)),
            Some((plains, false)),
        ]
        .map(|sample| sample.and_then(region_biome_value));
        let (labels, sizes, _) = component_labels(&values, 3, 1, true);
        assert_ne!(labels[0], labels[2]);
        assert_eq!(labels[1], 0);
        assert_eq!(sizes.len(), 2);
        assert_eq!(
            biome::biome_name(source_sample.0),
            biome::biome_name(plains)
        );
    }
}

fn blend(a: [u8; 3], b: [u8; 3], t: f64) -> Color32 {
    let t = t.clamp(0., 1.);
    Color32::from_rgb(
        (a[0] as f64 + (b[0] as f64 - a[0] as f64) * t) as u8,
        (a[1] as f64 + (b[1] as f64 - a[1] as f64) * t) as u8,
        (a[2] as f64 + (b[2] as f64 - a[2] as f64) * t) as u8,
    )
}
fn ramp(t: f64, colors: &[[u8; 3]]) -> Color32 {
    let p = t.clamp(0., 1.) * (colors.len() - 1) as f64;
    let i = (p.floor() as usize).min(colors.len() - 2);
    blend(colors[i], colors[i + 1], p - i as f64)
}
fn color(layer: Layer, id: u16, p: &Probe) -> Color32 {
    if p.surface_magma.is_some() {
        return color_from_rgb(crate::volcanic_lava::MAGMA_COLOR);
    }
    if p.surface_lava.is_some() {
        return color_from_rgb(crate::volcanic_lava::COLOR);
    }
    if p.surface_water_y
        .is_some_and(|y| y > crate::surface_water::SEA_SURFACE_BLOCK_Y)
    {
        return Color32::from_rgb(43, 105, 133);
    }
    if p.ocean {
        return match p.biome.rsplit('/').next().unwrap_or("") {
            "deep_ocean_trench" => Color32::from_rgb(21, 49, 72),
            "deep_ocean" => Color32::from_rgb(26, 65, 90),
            "ocean_reef" => Color32::from_rgb(40, 101, 115),
            "ice_sheet_oceanic" => Color32::from_rgb(148, 189, 198),
            _ => Color32::from_rgb(33, 80, 106),
        };
    }
    if p.surface_water {
        return if p.biome.ends_with("river") || p.biome.ends_with("lake") {
            Color32::from_rgb(71, 157, 178)
        } else {
            Color32::from_rgb(43, 105, 133)
        };
    }
    match layer {
        Layer::Biomes => land_biome_color(id, p),
        Layer::Ground => crate::ground_view::probe_color(p)
            .map(color_from_rgb)
            .unwrap_or_else(|| land_biome_color(id, p)),
        Layer::Temperature => ramp(
            (p.climate.temperature + 25.) / 60.,
            &[
                [124, 161, 191],
                [180, 210, 208],
                [223, 223, 175],
                [212, 163, 105],
                [173, 85, 71],
            ],
        ),
        Layer::Rainfall => ramp(
            p.climate.rainfall / 500.,
            &[
                [210, 190, 145],
                [177, 191, 136],
                [94, 153, 127],
                [45, 116, 129],
                [48, 87, 134],
            ],
        ),
        Layer::Height => ramp(
            (p.height.unwrap_or(60.) - 60.) / 200.,
            &[
                [117, 160, 120],
                [159, 173, 133],
                [174, 157, 126],
                [157, 144, 136],
                [220, 226, 222],
            ],
        ),
        Layer::Water => Color32::from_rgb(185, 188, 158),
        Layer::Geology => color_from_rgb(rock_legend_color(
            &p.rocks[0],
            crate::display::Palette::Natural,
        )),
        Layer::Forest => color_from_rgb(forest_legend_color(
            p.forest,
            crate::display::Palette::Natural,
        )),
        Layer::Hotspots => ramp(
            p.hotspot.clamp(0., 1.),
            &[
                [172, 181, 159],
                [193, 163, 121],
                [194, 115, 78],
                [169, 62, 59],
            ],
        ),
    }
}

fn display_color(
    world: &World,
    layer: Layer,
    id: u16,
    sample: &Probe,
    display: &DisplayPrefs,
    x: i32,
    z: i32,
) -> Color32 {
    if world.dimension.is_planetary() {
        let base = match layer {
            Layer::Height if sample.height.is_some() => {
                let h = sample.height.unwrap();
                ramp(h / 192., planetary_height_stops(world.dimension))
            }
            Layer::Water => {
                if sample.biome.ends_with("martian_river") {
                    Color32::from_rgb(78, 119, 139)
                } else {
                    mix_color(
                        planetary_biome_color(id, display.palette),
                        Color32::from_gray(117),
                        0.48,
                    )
                }
            }
            _ => planetary_biome_color(id, display.palette),
        };
        return highlighted_biome_color(base, id, display, false);
    }
    if world.dimension == Dimension::Beneath {
        let base = beneath_biome_color(id, display.palette);
        return highlighted_biome_color(base, id, display, false);
    }
    // A covered lake retains its biome identity without implicitly enabling the
    // Ground layer. Its dark swatch describes the water biome below solid land.
    let underground_water = layer == Layer::Biomes
        && !sample.is_bare_surface()
        && (sample.biome.ends_with("lake")
            || sample.biome.ends_with("river")
                && crate::surface_water::is_underground_river(
                    world,
                    x,
                    z,
                    sample
                        .ground_y
                        .unwrap_or_else(|| world.height(x, z).unwrap_or(63.) as i32),
                ));
    let mut base = if underground_water {
        color_from_rgb(UNDERGROUND_WATER_COLOR)
    } else if let Some(season) = display.simulated_season().filter(|_| sample.is_water()) {
        let climate = world.region.climate(x, z);
        color_from_rgb(crate::map_colors::water_at(
            climate.temperature,
            climate.rainfall,
            z,
            world.region.settings.temperature_scale,
            Some(season),
        ))
    } else if display.simulated_season().is_some()
        && (layer == Layer::Ground
            || layer == Layer::Biomes && crate::surface_water::is_inland_water_biome(id))
        && !sample.is_bare_surface()
    {
        crate::ground_view::probe_color_at(
            sample,
            world.region.settings.temperature_scale,
            display.simulated_season(),
        )
        .map(color_from_rgb)
        .unwrap_or_else(|| color(layer, id, sample))
    } else {
        color(layer, id, sample)
    };
    if display.palette == crate::display::Palette::Colorblind
        && !sample.is_bare_surface()
        && !underground_water
    {
        // Distinct high-contrast categories and sequential blue/yellow scales,
        // rather than simulating a color-vision deficiency on the existing map.
        base = match layer {
            Layer::Temperature => ramp(
                ((sample.climate.temperature + 30.) / 60.).clamp(0., 1.),
                &[
                    [51, 78, 139],
                    [164, 193, 207],
                    [248, 219, 114],
                    [209, 119, 42],
                ],
            ),
            Layer::Rainfall => ramp(
                (sample.climate.rainfall / 500.).clamp(0., 1.),
                &[
                    [247, 225, 156],
                    [125, 193, 192],
                    [43, 114, 142],
                    [20, 55, 95],
                ],
            ),
            Layer::Forest => color_from_rgb(forest_legend_color(
                sample.forest,
                crate::display::Palette::Colorblind,
            )),
            Layer::Geology => color_from_rgb(rock_legend_color(
                &sample.rocks[0],
                crate::display::Palette::Colorblind,
            )),
            Layer::Biomes => {
                const COLORS: [[u8; 3]; 8] = [
                    [230, 159, 0],
                    [86, 180, 233],
                    [0, 158, 115],
                    [240, 228, 66],
                    [0, 114, 178],
                    [213, 94, 0],
                    [204, 121, 167],
                    [172, 181, 178],
                ];
                let key = id as usize;
                let c = COLORS[key % COLORS.len()];
                Color32::from_rgb(c[0], c[1], c[2])
            }
            _ => base,
        };
    }
    if !display.has_highlight_filter() || sample.is_bare_surface() {
        return base;
    }
    let climate = if display.temperature_enabled || display.rainfall_enabled {
        if matches!(layer, Layer::Temperature | Layer::Rainfall) {
            sample.climate.clone()
        } else {
            world.region.climate(x, z)
        }
    } else {
        crate::model::Climate::default()
    };
    let rocks = if display.highlighted_rocks.is_empty() {
        Vec::new()
    } else {
        if layer == Layer::Geology {
            sample.rocks.to_vec()
        } else {
            crate::geology::rock_stack(world.region.rock_point_at(x, z)).to_vec()
        }
    };
    if display.matches_land_sample(
        id,
        climate.temperature as f32,
        climate.rainfall as f32,
        &rocks,
    ) {
        mix_color(base, Color32::from_rgb(255, 222, 108), 0.58)
    } else {
        mix_color(base, Color32::from_rgb(23, 31, 35), 0.42)
    }
}

fn highlighted_biome_color(
    base: Color32,
    id: u16,
    display: &DisplayPrefs,
    preserve_ocean: bool,
) -> Color32 {
    if display.highlighted_biomes.is_empty() || preserve_ocean && biome::is_ocean(id) {
        base
    } else if display.biome_highlighted(id) {
        mix_color(base, Color32::from_rgb(255, 222, 108), 0.58)
    } else {
        mix_color(base, Color32::from_rgb(23, 31, 35), 0.42)
    }
}

fn planetary_biome_color(id: u16, palette: crate::display::Palette) -> Color32 {
    let rgb = if palette == crate::display::Palette::Colorblind {
        const COLORS: [[u8; 3]; 14] = [
            [230, 159, 0],
            [86, 180, 233],
            [0, 158, 115],
            [240, 228, 66],
            [0, 114, 178],
            [213, 94, 0],
            [204, 121, 167],
            [172, 181, 178],
            [141, 106, 28],
            [65, 160, 175],
            [114, 84, 157],
            [211, 168, 213],
            [156, 204, 180],
            [226, 187, 144],
        ];
        COLORS[id as usize % COLORS.len()]
    } else {
        crate::planetary::biome_color(id)
    };
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

fn beneath_biome_color(id: u16, palette: crate::display::Palette) -> Color32 {
    const ACCESSIBLE: [[u8; 3]; 11] = [
        [230, 159, 0],
        [86, 180, 233],
        [0, 158, 115],
        [240, 228, 66],
        [0, 114, 178],
        [213, 94, 0],
        [204, 121, 167],
        [172, 181, 178],
        [141, 106, 28],
        [65, 160, 175],
        [114, 84, 157],
    ];
    let c = if palette == crate::display::Palette::Colorblind {
        ACCESSIBLE[id.saturating_sub(1000) as usize % ACCESSIBLE.len()]
    } else {
        crate::beneath::biome_color(id)
    };
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Base map color for a biome legend swatch, using the same palette as raster rendering.
pub fn biome_legend_color(id: u16, dimension: Dimension, display: &DisplayPrefs) -> [u8; 3] {
    let c = if dimension.is_planetary() {
        highlighted_biome_color(
            planetary_biome_color(id, display.palette),
            id,
            display,
            false,
        )
    } else if dimension == Dimension::Beneath {
        highlighted_biome_color(beneath_biome_color(id, display.palette), id, display, false)
    } else if display.palette == crate::display::Palette::Colorblind {
        const COLORS: [[u8; 3]; 8] = [
            [230, 159, 0],
            [86, 180, 233],
            [0, 158, 115],
            [240, 228, 66],
            [0, 114, 178],
            [213, 94, 0],
            [204, 121, 167],
            [172, 181, 178],
        ];
        let name = biome::biome_name(id);
        if biome::is_ocean(id) {
            let sample = Probe {
                biome: name.to_owned(),
                ocean: true,
                ..Probe::default()
            };
            color(Layer::Biomes, id, &sample)
        } else {
            let rgb = COLORS[id as usize % COLORS.len()];
            Color32::from_rgb(rgb[0], rgb[1], rgb[2])
        }
    } else {
        let name = biome::biome_name(id);
        let sample = Probe {
            biome: name.to_owned(),
            ocean: biome::is_ocean(id),
            ..Probe::default()
        };
        color(Layer::Biomes, id, &sample)
    };
    let c = if dimension == Dimension::Overworld
        && !display.temperature_enabled
        && !display.rainfall_enabled
        && display.highlighted_rocks.is_empty()
    {
        highlighted_biome_color(c, id, display, true)
    } else {
        c
    };
    [c.r(), c.g(), c.b()]
}

fn mix_color(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 * (1. - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1. - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1. - t) + b.b() as f32 * t) as u8,
    )
}

// Slightly darker than the exposed river swatch [71,157,178].
const UNDERGROUND_WATER_COLOR: [u8; 3] = [48, 115, 140];

// Biome palettes describe water bodies, but a dry column still needs a land
// material color. Keep biome identity intact for hover, search and highlighting.
fn land_biome_color(id: u16, sample: &Probe) -> Color32 {
    if crate::surface_water::is_inland_water_biome(id) {
        crate::ground_view::probe_color(sample)
            .map(color_from_rgb)
            .unwrap_or(Color32::from_rgb(185, 188, 158))
    } else {
        biome_color(id, &sample.biome)
    }
}

fn biome_color(_id: u16, name: &str) -> Color32 {
    static COLORS: std::sync::OnceLock<HashMap<String, Color32>> = std::sync::OnceLock::new();
    let colors = COLORS.get_or_init(|| {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../assets/data/tfg/biomes.json"))
                .expect("bundled palette");
        v["colors"]
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(k, v)| {
                let n = if let Some(s) = v.as_str() {
                    u32::from_str_radix(s.trim_start_matches('#'), 16).ok()?
                } else {
                    v.as_u64()? as u32
                };
                Some((
                    k.clone(),
                    Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8),
                ))
            })
            .collect()
    });
    colors
        .get(name)
        .copied()
        .map(|c| blend([c.r(), c.g(), c.b()], [156, 173, 151], 0.16))
        .unwrap_or(Color32::from_rgb(139, 164, 127))
}
/// Exact raster swatch for a rock name under the current palette.
pub fn rock_legend_color(name: &str, palette: crate::display::Palette) -> [u8; 3] {
    let hash = name
        .bytes()
        .fold(0u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
    if palette == crate::display::Palette::Colorblind {
        const COLORS: [[u8; 3]; 8] = [
            [230, 159, 0],
            [86, 180, 233],
            [0, 158, 115],
            [240, 228, 66],
            [0, 114, 178],
            [213, 94, 0],
            [204, 121, 167],
            [172, 181, 178],
        ];
        COLORS[hash as usize % COLORS.len()]
    } else {
        let h = (hash % 70) as u8;
        [121 + h, 121 + h / 2, 114 + h / 3]
    }
}

/// Exact raster swatch for one of TFC's five forest categories.
pub fn forest_legend_color(category: u8, palette: crate::display::Palette) -> [u8; 3] {
    let (value, colors): (f64, &[[u8; 3]]) = if palette == crate::display::Palette::Colorblind {
        (
            category as f64 / 9.0,
            &[
                [229, 228, 204],
                [119, 189, 177],
                [32, 117, 126],
                [18, 59, 81],
            ],
        )
    } else {
        (
            category as f64 / 4.0,
            &[
                [199, 194, 158],
                [159, 181, 131],
                [103, 153, 106],
                [61, 117, 91],
                [32, 84, 73],
            ],
        )
    };
    let c = ramp(value, colors);
    [c.r(), c.g(), c.b()]
}

fn color_from_rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}
