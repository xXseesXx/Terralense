//! Native eframe viewer. The atlas owns generation and tile workers; this module owns
//! the quiet map-first interface, pointer interaction, search and user preferences.
mod analysis_panel;
mod display_panel;
mod documents;
mod export_panel;
mod feature_bar;
mod file_dialogs;
mod icons;
mod legend;
mod map_tools;
mod marker_groups;
mod navigation;
#[cfg(feature = "diagnostics")]
mod responsiveness;
mod seed_list;
mod site_evidence;
mod structure_bounds;
mod tooltip;
mod ui_shell;
mod workspace;

use crate::atlas::{Atlas, Layer};
use crate::dimension::Dimension;
use crate::model::{Marker, MarkerKind, Point, WorldInfo};
use crate::search::Scope;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Layout, Pos2, Rect, RichText,
    Sense, Stroke, Vec2,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(29, 31, 34);
const PANEL: Color32 = Color32::from_rgb(39, 41, 45);
const PANEL_RAISED: Color32 = Color32::from_rgb(47, 50, 55);
const EDGE: Color32 = Color32::from_rgb(66, 69, 75);
const TEXT: Color32 = Color32::from_rgb(224, 235, 233);
const MUTED: Color32 = Color32::from_rgb(137, 157, 160);
const TEAL: Color32 = Color32::from_rgb(104, 218, 198);

fn default_beneath_y() -> i32 {
    96
}

fn beneath_band_top(y: i32) -> i32 {
    Dimension::Beneath.band(y).1
}

fn beneath_band_range(top: i32) -> (i32, i32) {
    Dimension::Beneath.band(top)
}

fn marker_destination(marker: &Marker) -> Point {
    marker.map_point()
}

#[derive(Clone)]
enum MapHit {
    Marker(Marker),
    Cluster(Point, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Workspace {
    Find,
    Seed,
    Details,
    Tools,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    navigation: navigation::Navigation,
    #[serde(default)]
    defaults_version: u8,
    tools: map_tools::MapToolsPrefs,
    display: crate::display::DisplayPrefs,
    world_id: String,
    source_path: Option<PathBuf>,
    recent_sources: Vec<PathBuf>,
    last_directory: Option<PathBuf>,
    dock_tab: u8,
    search_page: u8,
    location_mode: u8,
    control_dock: bool,
    #[serde(default)]
    dimension_centers: std::collections::HashMap<String, Point>,
    #[serde(default)]
    dimension: Dimension,
    #[serde(default = "default_beneath_y")]
    slice_y: i32,
    layer: usize,
    center_x: f64,
    center_z: f64,
    blocks_per_point: f64,
    show_ores: bool,
    show_structures: bool,
    ore_filter: String,
    show_predictions: bool,
    search_scope: Scope,
    seed_preview: Option<i64>,
    connected_highlight: bool,
    surface_detail: bool,
    allow_detail_rendering: bool,
    show_legend: bool,
    tooltip: tooltip::TooltipPrefs,
    #[serde(default)]
    seed_list: seed_list::SeedShortlist,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            navigation: navigation::Navigation::default(),
            defaults_version: 2,
            tools: map_tools::MapToolsPrefs::default(),
            display: crate::display::DisplayPrefs::default(),
            world_id: String::new(),
            source_path: None,
            recent_sources: Vec::new(),
            last_directory: None,
            dock_tab: 0,
            search_page: 1,
            location_mode: 0,
            control_dock: true,
            dimension_centers: std::collections::HashMap::new(),
            dimension: Dimension::Overworld,
            slice_y: 96,
            layer: 0,
            center_x: 0.,
            center_z: 0.,
            blocks_per_point: 32.,
            show_ores: true,
            show_structures: true,
            ore_filter: String::new(),
            show_predictions: false,
            search_scope: Scope::All,
            seed_preview: None,
            connected_highlight: false,
            surface_detail: false,
            allow_detail_rendering: false,
            show_legend: true,
            tooltip: tooltip::TooltipPrefs::default(),
            seed_list: seed_list::SeedShortlist::default(),
        }
    }
}

pub struct TerraLensApp {
    ore_icons: crate::ore_icons::IconCache,
    structure_icons: crate::structure_icons::IconCache,
    tools: map_tools::MapTools,
    location_query: crate::analysis::LocationQuery,
    exporter: export_panel::State,
    legend: legend::State,
    show_tools: bool,
    tools_tab: u8,
    qol_smoke: bool,
    qol_stage: u8,
    applied_display: crate::display::DisplayPrefs,
    pending_display: Option<(crate::display::DisplayPrefs, Instant)>,
    last_bounds: crate::model::Bounds,
    context_point: Option<Point>,
    session_input: String,
    session_open: bool,
    session_error: Option<String>,
    session_path: String,
    file_dialogs: file_dialogs::Dialogs,
    feature_bar: feature_bar::State,
    feature_entries: ui_shell::FeatureEntries,
    source_serial: u64,
    pending_source: Option<(u64, PathBuf)>,
    pending_source_view: Option<navigation::View>,
    pending_preferences: Option<Preferences>,
    pending_session_path: Option<PathBuf>,
    pending_session_write: Option<PathBuf>,
    pending_marker_export: Option<Vec<Marker>>,
    document_error: Option<String>,
    export_open: bool,
    help_open: bool,
    structure_query: String,
    structure_whole_save: bool,
    instance: PathBuf,
    worlds: Vec<WorldInfo>,
    #[cfg(feature = "diagnostics")]
    responsiveness: Option<responsiveness::Monitor>,
    #[cfg(feature = "diagnostics")]
    diagnostic_searching: bool,
    discovering: bool,
    discovery_error: Option<String>,
    selected: Option<usize>,
    atlas: Option<Atlas>,
    prefs: Preferences,
    selected_marker: Option<Marker>,
    structure_inspection: Option<crate::model::StructureBounds>,
    pointer_world: Option<Point>,
    search_x: String,
    search_z: String,
    search_radius: String,
    search_query: String,
    search_focus: bool,
    search_dirty: Option<Instant>,
    search_origin: Point,
    search_selected: Option<usize>,
    search_scroll: bool,
    search_select_after: bool,
    show_search: bool,
    show_seed: bool,
    show_preferences: bool,
    seed_input: String,
    seed_invalid: bool,
    seed_list_editor: seed_list::Editor,
    selection_drag_active: bool,
    preview_seed: Option<i64>,
    smoke_test: bool,
    smoke_query: Option<String>,
    smoke_inspect: bool,
    screenshot: Option<PathBuf>,
    initial_view: Option<(Point, f64)>,
    dimension_diagnostic: bool,
    screenshot_started_at: Option<Instant>,
    screenshot_sent: bool,
    screenshot_frame: u32,
    smoke_started: bool,
    smoke_started_at: Option<Instant>,
    smoke_selection_frame: Option<u32>,
}

impl Preferences {
    /// Apply the requested quieter defaults once to existing installations.
    fn upgrade_defaults(&mut self) {
        if self.defaults_version < 1 {
            self.connected_highlight = false;
            self.surface_detail = false;
            self.allow_detail_rendering = false;
            self.defaults_version = 1;
        }
        if self.defaults_version < 2 {
            self.display.apply_structure_visibility_defaults();
            self.defaults_version = 2;
        }
    }
}

impl TerraLensApp {
    fn is_monitoring(&self) -> bool {
        #[cfg(feature = "diagnostics")]
        {
            self.responsiveness.is_some()
        }
        #[cfg(not(feature = "diagnostics"))]
        {
            false
        }
    }
    fn sync_workspace_flags(&mut self) {
        let locations =
            self.prefs.control_dock && self.prefs.dock_tab == 0 && self.prefs.search_page == 1;
        let sites = locations
            && self.prefs.location_mode == 1
            && self.prefs.dimension == Dimension::Overworld;
        self.show_search = locations && !sites;
        self.show_seed =
            self.prefs.control_dock && self.prefs.dock_tab == 0 && self.prefs.search_page == 0;
        self.show_tools = self.prefs.control_dock && (self.prefs.dock_tab == 1 || sites);
    }

    fn set_workspace(&mut self, workspace: Option<Workspace>) {
        self.show_search = workspace == Some(Workspace::Find);
        self.show_seed = workspace == Some(Workspace::Seed);
        self.show_preferences = workspace == Some(Workspace::Details);
        self.show_tools = workspace == Some(Workspace::Tools);
        match workspace {
            Some(Workspace::Find) => {
                self.prefs.dock_tab = 0;
                self.prefs.search_page = 1;
                self.prefs.location_mode = 0;
                self.prefs.control_dock = true;
            }
            Some(Workspace::Seed) => {
                self.prefs.dock_tab = 0;
                self.prefs.search_page = 0;
                self.prefs.control_dock = true;
            }
            Some(Workspace::Tools) => {
                if self.tools_tab == 0 {
                    self.prefs.dock_tab = 1;
                } else if self.tools_tab == 1 {
                    self.prefs.dock_tab = 0;
                    self.prefs.search_page = 1;
                    self.prefs.location_mode = 1;
                } else {
                    self.export_open = true;
                }
                self.prefs.control_dock = true;
            }
            _ => {}
        }
    }

    fn world_supports_dimension(world: &WorldInfo, dimension: Dimension) -> bool {
        match dimension {
            Dimension::Overworld => world.compatible,
            Dimension::Beneath | Dimension::Moon | Dimension::Mars => world
                .dimensions
                .iter()
                .any(|d| d.id == dimension.id() && d.supported),
        }
    }

    fn scope_supported(dimension: Dimension, scope: Scope) -> bool {
        dimension == Dimension::Overworld
            || matches!(
                scope,
                Scope::All | Scope::Structures | Scope::Biomes | Scope::Ores
            )
    }

    pub(super) fn layer_supported(dimension: Dimension, layer: Layer) -> bool {
        match dimension {
            Dimension::Overworld => true,
            Dimension::Beneath => layer == Layer::Biomes,
            Dimension::Moon => matches!(layer, Layer::Biomes | Layer::Height),
            Dimension::Mars => matches!(layer, Layer::Biomes | Layer::Height | Layer::Water),
        }
    }

    pub(super) fn area_analysis_supported(dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }

    fn set_beneath_band(&mut self, y: i32) {
        if self.prefs.dimension != Dimension::Beneath {
            return;
        }
        let top = beneath_band_top(y);
        if self.prefs.slice_y == top {
            return;
        }
        self.prefs.navigation.record(self.current_view());
        self.prefs.slice_y = top;
        if let Some(atlas) = &mut self.atlas {
            atlas.set_dimension(Dimension::Beneath, top);
        }
        let keep_selected = self.selected_marker.as_ref().is_some_and(|marker| {
            marker.kind == MarkerKind::Structure
                || marker
                    .bounds
                    .is_some_and(|bounds| bounds.intersects_band(top - 15, top))
        });
        if !keep_selected {
            self.selected_marker = None;
            self.search_selected = None;
            self.search_select_after = false;
        }
        self.search_dirty = self.show_search.then(Instant::now);
        self.prefs.navigation.record(self.current_view());
    }

    fn view_marker_blocks(&mut self, marker: &Marker) {
        if !self.prefs.allow_detail_rendering {
            return;
        }
        let Some((center, band)) =
            structure_bounds::inspection_view(marker, self.prefs.dimension, self.prefs.slice_y)
        else {
            return;
        };
        self.set_center(center);
        if let Some(y) = band {
            self.set_beneath_band(y);
        }
        self.prefs.surface_detail = true;
        self.prefs.layer = 0;
        self.prefs.blocks_per_point = 0.25;
        self.selected_marker = Some(marker.clone());
        self.structure_inspection = marker.bounds;
        // At block scale a floating key can cover the entire footprint in a narrow map.
        self.legend.set_open(false);
        self.prefs.show_legend = false;
        self.prefs.navigation.record(self.current_view());
    }

    fn switch_dimension(&mut self, previous: Dimension) {
        let next = self.prefs.dimension;
        if self
            .selected
            .and_then(|index| self.worlds.get(index))
            .is_some_and(|world| !Self::world_supports_dimension(world, next))
        {
            self.prefs.dimension = previous;
            return;
        }
        let old_key = format!("{}|{}", self.prefs.world_id, previous.id());
        self.prefs
            .dimension_centers
            .insert(old_key, self.current_center());
        self.prefs.dimension = previous;
        self.prefs.navigation.record(self.current_view());
        self.prefs.dimension = next;
        if next == Dimension::Beneath {
            self.prefs.slice_y = beneath_band_top(self.prefs.slice_y);
        }
        let next_key = format!("{}|{}", self.prefs.world_id, next.id());
        let restored = self
            .prefs
            .dimension_centers
            .get(&next_key)
            .copied()
            .or_else(|| {
                self.selected
                    .and_then(|i| self.worlds.get(i))
                    .and_then(|w| {
                        if next == Dimension::Overworld {
                            Some(w.center)
                        } else {
                            w.dimensions
                                .iter()
                                .find(|d| d.id == next.id())
                                .map(|d| d.center)
                        }
                    })
            });
        if let Some(point) = restored {
            self.prefs.center_x = point.x;
            self.prefs.center_z = point.z;
        }
        let selected_layer = Layer::ALL
            .get(self.prefs.layer)
            .copied()
            .unwrap_or(Layer::Biomes);
        if !Self::layer_supported(next, selected_layer) {
            self.prefs.layer = 0;
        }
        if !Self::scope_supported(next, self.prefs.search_scope) {
            self.prefs.search_scope = Scope::All;
        }
        self.selected_marker = None;
        self.search_selected = None;
        self.search_select_after = false;
        self.search_dirty = self.show_search.then(Instant::now);
        self.search_x = format!("{:.0}", self.prefs.center_x);
        self.search_z = format!("{:.0}", self.prefs.center_z);
        if let Some(atlas) = &mut self.atlas {
            atlas.set_dimension(self.prefs.dimension, self.prefs.slice_y);
        }
        self.prefs.navigation.record(self.current_view());
    }

    fn new(cc: &eframe::CreationContext<'_>, config: RunConfig) -> Self {
        install_style(&cc.egui_ctx);
        let mut prefs = cc
            .storage
            .and_then(|s| eframe::get_value::<Preferences>(s, "preferences"))
            .unwrap_or_default();
        prefs.upgrade_defaults();
        if let Some(seed) = config.initial_seed {
            prefs.seed_preview = Some(seed);
        }
        if let Some(dimension) = config.dimension {
            prefs.dimension = dimension;
        }
        if let Some(y) = config.slice_y {
            prefs.slice_y = beneath_band_top(y);
        }
        prefs.slice_y = beneath_band_top(prefs.slice_y);
        if config.surface_stress {
            prefs.seed_preview = Some(7787531943268589495);
            prefs.layer = 0;
            prefs.surface_detail = true;
            prefs.allow_detail_rendering = true;
        }
        if config.beneath_stress {
            prefs.dimension = Dimension::Beneath;
            prefs.seed_preview = None;
            prefs.layer = 0;
            prefs.surface_detail = true;
            prefs.allow_detail_rendering = true;
            prefs.show_ores = true;
        }
        if !Self::layer_supported(
            prefs.dimension,
            Layer::ALL
                .get(prefs.layer)
                .copied()
                .unwrap_or(Layer::Biomes),
        ) {
            prefs.layer = 0;
        }
        if !Self::scope_supported(prefs.dimension, prefs.search_scope) {
            prefs.search_scope = Scope::All;
        }
        prefs.navigation.normalize();
        prefs.seed_list.normalize();
        // Only reopen an explicitly chosen source. A fresh launch is a seed viewer.
        let instance = prefs.source_path.clone().unwrap_or_default();
        let worlds = Vec::new();
        let restoring_source = config.source_path.is_none() && prefs.source_path.is_some();
        let source = config
            .source_path
            .clone()
            .or_else(|| prefs.source_path.clone())
            .or_else(|| std::env::var_os("TERRALENS_INSTANCE").map(PathBuf::from));
        let preview_seed = prefs.seed_preview.or_else(|| source.is_none().then_some(0));
        let selected = None;
        let atlas = Atlas::new(
            &cc.egui_ctx,
            preview_seed.unwrap_or(0),
            crate::model::Settings::default(),
        );
        let screenshot_started_at = config.screenshot.as_ref().map(|_| Instant::now());
        let mut this = Self {
            ore_icons: crate::ore_icons::IconCache::default(),
            structure_icons: crate::structure_icons::IconCache::default(),
            tools: map_tools::MapTools::default(),
            location_query: crate::analysis::LocationQuery::default(),
            exporter: export_panel::State::default(),
            legend: legend::State::default(),
            show_tools: false,
            tools_tab: 0,
            qol_smoke: config.qol_smoke,
            qol_stage: 0,
            applied_display: crate::display::DisplayPrefs::default(),
            pending_display: None,
            last_bounds: crate::model::Bounds::default(),
            context_point: None,
            session_input: String::new(),
            session_open: false,
            session_error: None,
            session_path: String::new(),
            file_dialogs: file_dialogs::Dialogs::default(),
            feature_bar: feature_bar::State::default(),
            feature_entries: ui_shell::FeatureEntries::default(),
            source_serial: 0,
            pending_source: None,
            pending_source_view: None,
            pending_preferences: None,
            pending_session_path: None,
            pending_session_write: None,
            pending_marker_export: None,
            document_error: None,
            export_open: false,
            help_open: false,
            structure_query: String::new(),
            structure_whole_save: false,
            instance,
            worlds,
            #[cfg(feature = "diagnostics")]
            responsiveness: config.responsiveness.map(|path| {
                responsiveness::Monitor::new(
                    &cc.egui_ctx,
                    path,
                    config.surface_stress,
                    config.beneath_stress,
                )
            }),
            #[cfg(feature = "diagnostics")]
            diagnostic_searching: false,
            discovering: false,
            discovery_error: None,
            selected,
            atlas: Some(atlas),
            prefs,
            selected_marker: None,
            structure_inspection: None,
            pointer_world: None,
            search_x: String::new(),
            search_z: String::new(),
            search_radius: "10000".into(),
            search_query: String::new(),
            search_focus: false,
            search_dirty: None,
            search_origin: Point::default(),
            search_selected: None,
            search_scroll: false,
            search_select_after: false,
            show_search: config.workspace == Some(Workspace::Find),
            show_seed: config.workspace == Some(Workspace::Seed),
            show_preferences: config.workspace == Some(Workspace::Details),
            seed_input: preview_seed
                .map(|seed| seed.to_string())
                .unwrap_or_default(),
            seed_invalid: false,
            seed_list_editor: seed_list::Editor::default(),
            selection_drag_active: false,
            preview_seed,
            smoke_test: config.smoke_test,
            smoke_query: config.smoke_query,
            smoke_inspect: config.smoke_inspect,
            screenshot: config.screenshot,
            initial_view: config.initial_view,
            dimension_diagnostic: config.dimension_diagnostic,
            screenshot_started_at,
            screenshot_sent: false,
            screenshot_frame: 0,
            smoke_started: false,
            smoke_started_at: None,
            smoke_selection_frame: None,
        };
        if this.smoke_test {
            this.prefs.blocks_per_point = 32.;
            this.prefs.show_predictions = false;
            if this.smoke_query.is_some() {
                this.prefs.show_ores = true;
            }
        }
        if let Some(i) = this.selected {
            this.activate_world(i, &cc.egui_ctx);
        } else if let Some(seed) = preview_seed {
            this.activate_seed(seed, &cc.egui_ctx);
        }
        if let Some(workspace) = config.workspace {
            this.set_workspace(Some(workspace));
        }
        this.legend.set_open(this.prefs.show_legend);
        if let Some(path) = source {
            let view = restoring_source.then(|| this.current_view());
            this.open_source_file(path, view);
        }
        this
    }

    fn poll_worlds(&mut self, ctx: &egui::Context) {
        self.poll_source(ctx);
        if !self.discovering {
            if let Some((point, bpp)) = self.initial_view.take() {
                self.prefs.center_x = point.x;
                self.prefs.center_z = point.z;
                self.prefs.blocks_per_point = bpp;
            }
        }
    }

    fn activate_world(&mut self, index: usize, ctx: &egui::Context) {
        self.cancel_pending_source();
        let Some(world) = self.worlds.get(index).cloned() else {
            self.selected = None;
            return;
        };
        if !Self::world_supports_dimension(&world, self.prefs.dimension) {
            self.prefs.dimension = Dimension::ALL
                .into_iter()
                .find(|dimension| Self::world_supports_dimension(&world, *dimension))
                .unwrap_or(Dimension::Overworld);
            self.prefs.layer = 0;
        }
        let changed_world = self.prefs.world_id != world.id;
        if changed_world && !self.prefs.world_id.is_empty() {
            self.prefs.navigation.record(self.current_view());
        }
        self.preview_seed = None;
        self.prefs.seed_preview = None;
        self.selected = Some(index);
        self.prefs.world_id = world.id.clone();
        self.seed_input = world.seed.to_string();
        if changed_world || (self.prefs.center_x == 0. && self.prefs.center_z == 0.) {
            let key = format!("{}|{}", self.prefs.world_id, self.prefs.dimension.id());
            let point = self
                .prefs
                .dimension_centers
                .get(&key)
                .copied()
                .or_else(|| {
                    if self.prefs.dimension == Dimension::Overworld {
                        Some(world.center)
                    } else {
                        world
                            .dimensions
                            .iter()
                            .find(|d| d.id == self.prefs.dimension.id())
                            .map(|d| d.center)
                    }
                })
                .unwrap_or_default();
            self.prefs.center_x = point.x;
            self.prefs.center_z = point.z;
        }
        if self.atlas.is_none() {
            self.atlas = Some(Atlas::new(ctx, world.seed, world.settings.clone()));
        }
        if let Some(atlas) = &mut self.atlas {
            atlas.set_world(world.seed, world.settings, Some(world.path));
            atlas.set_dimension(self.prefs.dimension, self.prefs.slice_y);
            atlas.set_world_locations(world.map_locations);
        }
        self.selected_marker = None;
        self.search_selected = None;
        self.search_select_after = false;
        self.search_dirty = self.show_search.then(Instant::now);
        self.search_x = format!("{:.0}", self.prefs.center_x);
        self.search_z = format!("{:.0}", self.prefs.center_z);
        self.tools.clear_selection();
        self.prefs.navigation.record(self.current_view());
    }

    fn activate_seed(&mut self, seed: i64, ctx: &egui::Context) {
        self.cancel_pending_source();
        let changed = self.preview_seed != Some(seed);
        if changed && !self.prefs.world_id.is_empty() {
            self.prefs.navigation.record(self.current_view());
        }
        self.preview_seed = Some(seed);
        self.selected = None;
        self.prefs.seed_preview = Some(seed);
        self.prefs.world_id = format!("seed:{seed}");
        if changed {
            self.prefs.center_x = 0.;
            self.prefs.center_z = 0.;
        }
        let settings = crate::model::Settings::default();
        if self.atlas.is_none() {
            self.atlas = Some(Atlas::new(ctx, seed, settings.clone()));
        }
        if let Some(atlas) = &mut self.atlas {
            atlas.set_world(seed, settings, None);
            atlas.set_dimension(self.prefs.dimension, self.prefs.slice_y);
        }
        self.selected_marker = None;
        self.search_selected = None;
        self.search_select_after = false;
        self.search_dirty = self.show_search.then(Instant::now);
        self.search_x = format!("{:.0}", self.prefs.center_x);
        self.search_z = format!("{:.0}", self.prefs.center_z);
        self.tools.clear_selection();
        self.prefs.navigation.record(self.current_view());
    }

    fn activate_shortlisted_seed(&mut self, seed: i64, ctx: &egui::Context) {
        let previous = self.current_view();
        let navigation = std::mem::take(&mut self.prefs.navigation);
        let center = self.current_center();
        let bpp = self.prefs.blocks_per_point;
        let layer = self.prefs.layer;
        let dimension = self.prefs.dimension;
        let slice_y = self.prefs.slice_y;
        self.activate_seed(seed, ctx);
        self.prefs.center_x = center.x;
        self.prefs.center_z = center.z;
        self.prefs.blocks_per_point = bpp;
        self.prefs.layer = layer;
        self.prefs.dimension = dimension;
        self.prefs.slice_y = slice_y;
        if let Some(atlas) = &mut self.atlas {
            atlas.set_dimension(dimension, slice_y);
        }
        self.prefs.navigation = navigation;
        self.prefs.navigation.record(previous);
        self.prefs.navigation.record(self.current_view());
    }

    fn current_center(&self) -> Point {
        Point {
            x: self.prefs.center_x,
            z: self.prefs.center_z,
        }
    }
    fn set_center(&mut self, p: Point) {
        self.prefs.navigation.record(self.current_view());
        self.prefs.center_x = p.x;
        self.prefs.center_z = p.z;
        self.prefs.navigation.record(self.current_view());
    }

    fn selected_marker_panel(&mut self, ui: &mut egui::Ui) {
        let Some(marker) = self.selected_marker.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tooltip::display_marker_name(&marker.label))
                    .strong()
                    .color(TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("×").on_hover_text("Unpin").clicked() {
                    self.selected_marker = None;
                }
            });
        });
        let classification = match marker.kind {
            MarkerKind::Structure if marker.confirmed => "saved",
            MarkerKind::Structure | MarkerKind::Ore => "",
            MarkerKind::Spawn | MarkerKind::Player => "saved position",
            MarkerKind::Tree => "sampled habitat",
            _ => "sampled",
        };
        let y = marker
            .y
            .map(|value| format!("Y {:.0}", value))
            .or_else(|| {
                marker
                    .bounds
                    .map(|bounds| format!("Y {}–{}", bounds.min_y, bounds.max_y))
            })
            .unwrap_or_else(|| {
                if self.prefs.dimension == Dimension::Beneath {
                    let (min, max) = self.prefs.dimension.band(self.prefs.slice_y);
                    format!("Y {min}–{max}")
                } else {
                    String::new()
                }
            });
        ui.label(
            RichText::new(format!(
                "{}{} · {}, {}{}",
                marker.namespace,
                if classification.is_empty() {
                    String::new()
                } else {
                    format!(" · {classification}")
                },
                marker.map_point().x.floor() as i64,
                marker.map_point().z.floor() as i64,
                if y.is_empty() {
                    String::new()
                } else {
                    format!(" · {y}")
                }
            ))
            .size(10.)
            .color(MUTED),
        );
        ui.horizontal_wrapped(|ui| {
            let inspecting =
                self.structure_inspection.is_some() && self.structure_inspection == marker.bounds;
            if inspecting
                && ui
                    .small_button("Full terrain")
                    .on_hover_text(
                        "Leave the structure cutaway and show the full surface or cave layer",
                    )
                    .clicked()
            {
                self.structure_inspection = None;
            } else if !inspecting
                && marker.kind == MarkerKind::Structure
                && marker.bounds.is_some()
                && ui
                    .add_enabled(
                        self.prefs.allow_detail_rendering,
                        egui::Button::new("View blocks").small(),
                    )
                    .on_hover_text(if self.prefs.allow_detail_rendering {
                        "Inspect saved structure blocks at their recorded footprint"
                    } else {
                        "Enable block detail rendering in Details"
                    })
                    .clicked()
            {
                self.view_marker_blocks(&marker);
            }
            if ui.small_button("Center").clicked() {
                self.set_center(marker_destination(&marker));
            }
            if ui.small_button("Search here").clicked() {
                let destination = marker_destination(&marker);
                self.search_x = format!("{:.0}", destination.x);
                self.search_z = format!("{:.0}", destination.z);
                self.open_find();
                self.search_dirty = Some(Instant::now());
            }
            ui.menu_button("…", |ui| {
                if ui.button("Save spot").clicked() {
                    let mut view = self.current_view();
                    view.center = marker_destination(&marker);
                    self.prefs
                        .navigation
                        .add_bookmark(view, marker.label.clone());
                    ui.close();
                }
                if ui.button("Copy coordinates").clicked() {
                    ui.ctx().copy_text(format!(
                        "{}, {}",
                        marker.map_point().x.floor() as i64,
                        marker.map_point().z.floor() as i64
                    ));
                    ui.close();
                }
                if ui.button("Copy Xaero waypoint").clicked() {
                    let y = marker
                        .y
                        .map(|value| value.round() as i32)
                        .unwrap_or_else(|| {
                            if self.prefs.dimension == Dimension::Beneath {
                                self.prefs.slice_y
                            } else {
                                64
                            }
                        });
                    ui.ctx().copy_text(crate::exports::xaero_share(
                        &marker.label,
                        marker_destination(&marker),
                        y,
                        11,
                    ));
                    ui.close();
                }
            });
        });
        ui.collapsing("Details", |ui| tooltip::show_marker(ui, &marker));
        ui.separator();
    }

    fn open_find(&mut self) {
        self.set_workspace(Some(Workspace::Find));
        self.search_focus = true;
        if self
            .atlas
            .as_ref()
            .is_some_and(|a| a.search_status().is_empty())
        {
            self.search_dirty = Some(Instant::now());
        }
    }

    fn find_shortcuts(&mut self, ctx: &egui::Context) {
        if consume_find_shortcut(ctx) {
            self.open_find();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.show_preferences = false;
            self.export_open = false;
            self.help_open = false;
            self.selected_marker = None;
            self.search_focus = false;
            ctx.memory_mut(|m| m.surrender_focus(Id::new("atlas-find-query")));
            return;
        }
        if !self.show_search {
            return;
        }
        let query_focused = ctx.memory(|m| m.has_focus(Id::new("atlas-find-query")));
        if query_focused || !ctx.wants_keyboard_input() {
            let previous =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter));
            let next = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            if previous || next {
                if self.search_dirty.is_some()
                    || self
                        .atlas
                        .as_ref()
                        .is_some_and(|a| a.search_status().is_empty())
                {
                    self.submit_search(true);
                } else if self.atlas.as_ref().is_some_and(Atlas::is_searching) {
                    self.search_select_after = true;
                } else {
                    self.cycle_search(previous);
                }
            }
        }
    }

    fn submit_search(&mut self, select_first: bool) {
        let origin = Point {
            x: self.search_x.trim().parse().unwrap_or(f64::NAN),
            z: self.search_z.trim().parse().unwrap_or(f64::NAN),
        };
        let radius = self.search_radius.trim().parse().unwrap_or(f64::NAN);
        if let Some(atlas) = &mut self.atlas {
            atlas.find(
                origin,
                radius,
                self.search_query.trim().to_owned(),
                self.prefs.show_predictions,
                self.prefs.search_scope,
            );
        }
        self.search_origin = origin;
        self.search_dirty = None;
        self.search_selected = None;
        self.search_select_after = select_first;
    }

    fn select_search_result(&mut self, index: usize) {
        if let Some(marker) = self
            .atlas
            .as_ref()
            .and_then(|a| a.search_results().get(index))
            .cloned()
        {
            if self.prefs.dimension == Dimension::Beneath {
                let marker_y = marker
                    .y
                    .map(|y| y.round() as i32)
                    .or_else(|| marker.bounds.map(|b| b.min_y));
                let (min, max) = self.prefs.dimension.band(self.prefs.slice_y);
                if let Some(y) =
                    marker_y.filter(|_| marker.bounds.is_none_or(|b| !b.intersects_band(min, max)))
                {
                    self.set_beneath_band(y);
                }
            }
            self.search_selected = Some(index);
            self.search_scroll = true;
            let center = marker.map_point();
            self.set_center(center);
            self.selected_marker = Some(marker);
            if self.smoke_test {
                self.smoke_selection_frame = Some(self.screenshot_frame);
            }
        }
    }

    fn cycle_search(&mut self, previous: bool) {
        let count = self.atlas.as_ref().map_or(0, |a| a.search_results().len());
        if count > 0 {
            let index = next_search_index(self.search_selected, count, previous);
            self.select_search_result(index);
        }
    }

    fn search_panel(&mut self, ui: &mut egui::Ui) {
        let mut input = ui
            .horizontal(|ui| {
                let input = egui::TextEdit::singleline(&mut self.search_query)
                    .id(Id::new("atlas-find-query"))
                    .hint_text("Ore, structure, biome, rock, tree…")
                    .desired_width((ui.available_width() - 32.).max(80.))
                    .show(ui);
                if ui
                    .small_button("×")
                    .on_hover_text("Close Find · Esc")
                    .clicked()
                {
                    self.set_workspace(None);
                }
                input
            })
            .inner;
        if self.search_focus {
            input.response.request_focus();
            input
                .state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(self.search_query.chars().count()),
                )));
            input.state.store(ui.ctx(), input.response.id);
            self.search_focus = false;
        }
        let mut changed = input.response.changed();
        let enter = input.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("search-scope")
                .selected_text(self.prefs.search_scope.label())
                .width((ui.available_width() - 96.).max(110.))
                .show_ui(ui, |ui| {
                    for scope in Scope::ALL
                        .into_iter()
                        .filter(|scope| Self::scope_supported(self.prefs.dimension, *scope))
                    {
                        changed |= ui
                            .selectable_value(&mut self.prefs.search_scope, scope, scope.label())
                            .changed();
                    }
                });
            if ui
                .small_button("Map")
                .on_hover_text("Search from the current map center")
                .clicked()
            {
                self.search_x = format!("{:.0}", self.prefs.center_x);
                self.search_z = format!("{:.0}", self.prefs.center_z);
                changed = true;
            }
            ui.menu_button("…", |ui| {
                ui.horizontal(|ui| {
                    changed |= number_field(ui, "X", &mut self.search_x);
                    changed |= number_field(ui, "Z", &mut self.search_z);
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Radius").size(10.).color(MUTED));
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut self.search_radius).desired_width(80.))
                        .changed();
                    ui.label(RichText::new("blocks").size(10.).color(MUTED));
                });
                if matches!(self.prefs.search_scope, Scope::All | Scope::Structures) {
                    changed |= ui
                        .checkbox(&mut self.prefs.show_predictions, "unexplored structures")
                        .changed();
                }
                if let Some(atlas) = &self.atlas {
                    let status = atlas.search_status();
                    if !status.is_empty() {
                        ui.separator();
                        ui.label(RichText::new(status).size(10.).color(MUTED));
                    }
                }
                ui.separator();
                ui.label(
                    RichText::new("Enter: next · Shift+Enter: previous · Esc: close")
                        .size(10.)
                        .color(MUTED),
                );
            })
            .response
            .on_hover_text("Search origin, radius and coverage");
        });
        ui.label(
            RichText::new(format!("From {}, {}", self.search_x, self.search_z))
                .size(10.)
                .color(MUTED),
        );
        if changed {
            self.search_dirty = Some(Instant::now());
            self.search_select_after = false;
        }
        if enter {
            input.response.request_focus();
            self.submit_search(true);
        } else if self
            .search_dirty
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(300))
        {
            self.submit_search(false);
        } else if let Some(dirty) = self.search_dirty {
            ui.ctx()
                .request_repaint_after(Duration::from_millis(300).saturating_sub(dirty.elapsed()));
        }
        if let Some(atlas) = &self.atlas {
            let searching = atlas.is_searching();
            let status = atlas.search_status().to_owned();
            // Only visible rows construct widgets; thousands of ore results stay cheap to browse.
            let results = atlas.search_results();
            if !status.is_empty() {
                let summary = status
                    .split('·')
                    .take(2)
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join(" · ");
                ui.label(RichText::new(summary).size(10.).color(MUTED))
                    .on_hover_text(status);
            }
            if searching {
                ui.spinner();
            }
            if results.is_empty() && !searching && self.search_dirty.is_none() {
                ui.label(
                    RichText::new("No matches. Try a shorter name or another search origin.")
                        .size(11.)
                        .color(MUTED),
                );
            }
            ui.add_space(3.);
            let mut chosen = None;
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt("search-results")
                .max_height((ui.available_height() - 8.).max(160.));
            if self.search_scroll {
                scroll = scroll.vertical_scroll_offset(
                    self.search_selected.unwrap_or(0) as f32 * (44. + ui.spacing().item_spacing.y),
                );
                self.search_scroll = false;
            }
            scroll.show_rows(ui, 44., results.len(), |ui, range| {
                for index in range {
                    let marker = &results[index];
                    let distance = marker.map_point().distance(self.search_origin);
                    let (row, response) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 42.), Sense::click());
                    let fill = if self.search_selected == Some(index) {
                        Color32::from_rgb(38, 77, 72)
                    } else {
                        PANEL_RAISED
                    };
                    let painter = ui.painter().with_clip_rect(row);
                    painter.rect_filled(row, CornerRadius::same(6), fill);
                    painter.rect_stroke(
                        row,
                        CornerRadius::same(6),
                        Stroke::new(1_f32, EDGE),
                        egui::StrokeKind::Inside,
                    );
                    let icon_rect =
                        Rect::from_min_size(row.left_top() + Vec2::new(7., 8.), Vec2::splat(25.));
                    if marker.kind == MarkerKind::Ore {
                        if let Some(texture) = self.ore_icons.get(ui.ctx(), &marker.id) {
                            painter.image(
                                texture.id(),
                                icon_rect,
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                Color32::WHITE,
                            );
                        }
                    } else if matches!(
                        marker.kind,
                        MarkerKind::Structure | MarkerKind::Spawn | MarkerKind::Player
                    ) {
                        let texture = self.structure_icons.get(ui.ctx(), &marker.id);
                        painter.image(
                            texture.id(),
                            icon_rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                            Color32::WHITE,
                        );
                    } else {
                        let color =
                            Color32::from_rgb(marker.color[0], marker.color[1], marker.color[2]);
                        painter.circle_filled(icon_rect.center(), 7., color);
                    }
                    let source = match marker.kind {
                        MarkerKind::Structure if marker.confirmed => "Saved",
                        MarkerKind::Structure => "",
                        MarkerKind::Ore => "ore",
                        MarkerKind::Biome => "Biome",
                        MarkerKind::Rock => "Rock",
                        MarkerKind::Forest => "Forest",
                        MarkerKind::Tree => "Tree habitat",
                        MarkerKind::Spawn => "World spawn",
                        MarkerKind::Player => "Saved player",
                    };
                    let vertical = if let (Some(low), Some(high)) = (marker.min_y, marker.max_y) {
                        format!("Y {:.0}–{:.0}", low, high)
                    } else if let Some(y) = marker.y {
                        format!("Y {:.0}", y)
                    } else {
                        String::new()
                    };
                    let name_rect = Rect::from_min_max(
                        Pos2::new(icon_rect.right() + 7., row.top() + 4.),
                        Pos2::new(row.right() - 73., row.top() + 21.),
                    );
                    painter.text(
                        name_rect.left_top(),
                        Align2::LEFT_TOP,
                        tooltip::display_marker_name(&marker.label),
                        FontId::proportional(11.),
                        TEXT,
                    );
                    painter.text(
                        Pos2::new(name_rect.left(), row.top() + 23.),
                        Align2::LEFT_TOP,
                        if source.is_empty() {
                            vertical.clone()
                        } else if vertical.is_empty() {
                            source.to_owned()
                        } else {
                            format!("{source} · {vertical}")
                        },
                        FontId::proportional(9.),
                        MUTED,
                    );
                    painter.text(
                        Pos2::new(row.right() - 7., row.center().y),
                        Align2::RIGHT_CENTER,
                        format!("{:.1} km", distance / 1000.),
                        FontId::monospace(9.),
                        TEXT,
                    );
                    let response = response.on_hover_text(format!(
                        "{}, {}\n{}",
                        marker.map_point().x.floor() as i64,
                        marker.map_point().z.floor() as i64,
                        marker.detail
                    ));
                    if self.search_selected == Some(index)
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        response.scroll_to_me(Some(Align::Center));
                    }
                    if response.clicked() {
                        chosen = Some(index);
                    }
                }
            });
            if let Some(index) = chosen {
                self.select_search_result(index);
            }
        }
    }

    fn seed_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let current_seed = self.preview_seed.or_else(|| {
            self.selected
                .and_then(|index| self.worlds.get(index))
                .map(|world| world.seed)
        });
        let actions =
            self.seed_list_editor
                .show(ui, &self.prefs.seed_list, current_seed, &self.seed_input);
        for action in actions {
            match action {
                seed_list::Action::AddCurrent(name) => {
                    if let Some(seed) = current_seed {
                        self.prefs.seed_list.add(&name, seed);
                    }
                }
                seed_list::Action::AddInput(name) => {
                    if let Ok(seed) = self.seed_input.trim().parse::<i64>() {
                        self.prefs.seed_list.add(&name, seed);
                    }
                }
                seed_list::Action::Activate(seed) => self.activate_shortlisted_seed(seed, ctx),
                seed_list::Action::Remove(index) => {
                    self.prefs.seed_list.remove(index);
                }
                seed_list::Action::Rename(index, name) => {
                    self.prefs.seed_list.rename(index, &name);
                }
                seed_list::Action::Previous => {
                    if let Some(seed) = self.prefs.seed_list.adjacent(current_seed, false) {
                        self.activate_shortlisted_seed(seed, ctx);
                    }
                }
                seed_list::Action::Next => {
                    if let Some(seed) = self.prefs.seed_list.adjacent(current_seed, true) {
                        self.activate_shortlisted_seed(seed, ctx);
                    }
                }
            }
        }
    }

    fn map_view(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let rect = Rect::from_min_max(
            rect.min,
            Pos2::new(rect.max.x, rect.max.y.max(rect.min.y + 120.)),
        );
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        self.last_bounds = map_bounds(rect, self.current_center(), self.prefs.blocks_per_point);
        let center = self.current_center();
        if self.selected.is_none() && self.preview_seed.is_none() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(23, 34, 40));
            let message = if self.discovering {
                "Reading Prism saves…"
            } else if let Some(error) = self.discovery_error.as_deref() {
                error
            } else {
                "Choose a save or enter a seed from the top bar"
            };
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                message,
                FontId::proportional(14.),
                MUTED,
            );
            return;
        }
        let monitoring = self.is_monitoring();
        let Some(atlas) = &mut self.atlas else {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(23, 34, 40));
            let message = self.worlds.iter().find(|w| !w.compatible);
            if let Some(world) = message {
                ui.painter().text(
                    rect.center() - Vec2::new(0., 13.),
                    Align2::CENTER_CENTER,
                    format!("{} cannot be previewed", world.name),
                    FontId::proportional(15.),
                    TEXT,
                );
                ui.painter().text(
                    rect.center() + Vec2::new(0., 12.),
                    Align2::CENTER_CENTER,
                    world
                        .warning
                        .as_deref()
                        .unwrap_or("Unsupported world generation"),
                    FontId::proportional(11.),
                    MUTED,
                );
            } else {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Choose a save or enter a seed from the top bar",
                    FontId::proportional(14.),
                    MUTED,
                );
            }
            return;
        };
        let layer = Layer::ALL
            .get(self.prefs.layer)
            .copied()
            .unwrap_or(Layer::Biomes);
        let surface_path = self
            .selected
            .and_then(|i| self.worlds.get(i))
            .map(|w| w.path.clone());
        atlas.set_surface_options(
            self.prefs.allow_detail_rendering && self.prefs.surface_detail,
            surface_path,
        );
        if self.structure_inspection
            != self
                .selected_marker
                .as_ref()
                .filter(|marker| marker.kind == MarkerKind::Structure)
                .and_then(|marker| marker.bounds)
        {
            self.structure_inspection = None;
        }
        atlas.set_structure_inspection(self.structure_inspection);
        atlas.paint(ui, rect, center, self.prefs.blocks_per_point, layer);
        let legend_snapshot = atlas.legend_snapshot(layer).clone();
        if let Some(layer) = self.legend.show(
            ui.ctx(),
            rect,
            &legend_snapshot,
            &mut self.prefs.display,
            self.prefs.dimension,
            layer,
        ) {
            self.prefs.layer = Layer::ALL
                .iter()
                .position(|candidate| *candidate == layer)
                .unwrap_or(0);
        }
        self.prefs.show_legend = self.legend.is_open();
        site_evidence::paint(
            ui,
            rect,
            center,
            self.prefs.blocks_per_point,
            atlas.locations(),
            self.selected_marker.as_ref(),
            self.prefs.control_dock
                && self.prefs.dock_tab == 0
                && self.prefs.search_page == 1
                && self.prefs.location_mode == 1
                && self.prefs.dimension == Dimension::Overworld,
        );

        let bounds = map_bounds(rect, center, self.prefs.blocks_per_point);
        let mut visible_markers = atlas.visible_markers(
            marker_groups::query_bounds(bounds, self.prefs.blocks_per_point),
            self.prefs.show_ores,
            self.prefs.show_structures,
        );
        visible_markers.retain(|marker| {
            self.prefs.display.marker_visible_in_view(
                marker,
                self.prefs.blocks_per_point,
                self.prefs.dimension,
                self.prefs.slice_y,
            )
        });
        if let Some(marker) = &self.selected_marker {
            if bounds.contains(marker.map_point().x, marker.map_point().z)
                || marker.bounds.is_some_and(|b| {
                    b.min_x as f64 <= bounds.max_x
                        && b.max_x as f64 + 1. >= bounds.min_x
                        && b.min_z as f64 <= bounds.max_z
                        && b.max_z as f64 + 1. >= bounds.min_z
                })
            {
                visible_markers.push(marker);
            }
        }
        let mut marker_keys = std::collections::HashSet::new();
        visible_markers.retain(|m| marker_keys.insert((m.id.as_str(), m.x as i64, m.z as i64)));
        let footprint_hit = structure_bounds::paint(
            ui,
            rect,
            center,
            self.prefs.blocks_per_point,
            self.prefs.dimension,
            self.prefs.slice_y,
            &visible_markers,
            self.selected_marker.as_ref(),
            response.hover_pos(),
        );
        // World-anchored groups retain their membership and centroid while panning.
        let screen_markers: Vec<_> = visible_markers
            .iter()
            .map(|&m| {
                (
                    m,
                    world_to_screen(rect, center, self.prefs.blocks_per_point, m.map_point()),
                )
            })
            .collect();
        let groups = marker_groups::groups(
            &visible_markers,
            self.prefs.blocks_per_point,
            self.selected_marker.as_ref(),
        );
        let pointer_pos = response.hover_pos().filter(|p| rect.contains(*p));
        let mut hit: Option<(f32, MapHit)> =
            footprint_hit.map(|marker| (7., MapHit::Marker(marker)));
        for indices in &groups {
            let count = indices.len();
            let group_point = marker_groups::center(&visible_markers, indices);
            let average = world_to_screen(rect, center, self.prefs.blocks_per_point, group_point);
            if !rect.expand(32.).contains(average) {
                continue;
            }
            if count > 1 {
                let all_ore = indices
                    .iter()
                    .all(|i| matches!(screen_markers[*i].0.kind, MarkerKind::Ore));
                let color = if all_ore {
                    Color32::from_rgb(117, 204, 191)
                } else {
                    Color32::from_rgb(238, 190, 112)
                };
                let representative = indices.iter().map(|i| screen_markers[*i].0).max_by(|a, b| {
                    crate::features::marker_max_blocks_per_point(a)
                        .total_cmp(&crate::features::marker_max_blocks_per_point(b))
                        .then_with(|| a.confirmed.cmp(&b.confirmed))
                        .then_with(|| a.id.cmp(&b.id))
                        .then_with(|| a.x.total_cmp(&b.x))
                        .then_with(|| a.z.total_cmp(&b.z))
                });
                let texture = representative.and_then(|marker| match marker.kind {
                    MarkerKind::Ore => self.ore_icons.get(ui.ctx(), &marker.id).map(|t| t.id()),
                    MarkerKind::Structure => {
                        Some(self.structure_icons.get(ui.ctx(), &marker.id).id())
                    }
                    _ => None,
                });
                if let Some(texture) = texture {
                    let icon_rect = Rect::from_center_size(average, Vec2::splat(34.));
                    ui.painter().image(
                        texture,
                        icon_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                        Color32::WHITE,
                    );
                }
                let badge = average
                    + if texture.is_some() {
                        Vec2::new(13., 12.)
                    } else {
                        Vec2::ZERO
                    };
                let text = cluster_count(count);
                // A tiny text shadow keeps counts readable without adding a badge shape.
                ui.painter().text(
                    badge + Vec2::splat(1.),
                    Align2::CENTER_CENTER,
                    &text,
                    FontId::monospace(11.),
                    Color32::BLACK,
                );
                ui.painter().text(
                    badge,
                    Align2::CENTER_CENTER,
                    text,
                    FontId::monospace(11.),
                    color,
                );
                if let Some(pointer) = pointer_pos {
                    if let Some(d) = hit_distance(pointer, average, 21.)
                        && hit.as_ref().is_none_or(|(old, _)| d < *old)
                    {
                        hit = Some((d, MapHit::Cluster(group_point, count)));
                    }
                }
                continue;
            }
            let marker = screen_markers[indices[0]].0;
            let pos = screen_markers[indices[0]].1;
            let color = Color32::from_rgb(marker.color[0], marker.color[1], marker.color[2]);
            let selected = self
                .selected_marker
                .as_ref()
                .is_some_and(|m| m.id == marker.id && m.point() == marker.point());
            let size = if selected { 8. } else { 6. };
            match &marker.kind {
                MarkerKind::Ore => {
                    let icon_rect =
                        Rect::from_center_size(pos, Vec2::splat(if selected { 34. } else { 28. }));
                    if let Some(texture) = self.ore_icons.get(ui.ctx(), &marker.id) {
                        ui.painter().image(
                            texture.id(),
                            icon_rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                            Color32::WHITE,
                        );
                    } else {
                        ui.painter().circle_filled(pos, size - 1., color);
                    }
                }
                MarkerKind::Structure | MarkerKind::Spawn | MarkerKind::Player => {
                    let icon_rect = Rect::from_center_size(
                        pos,
                        Vec2::splat(if selected {
                            38.
                        } else {
                            crate::structure_icons::MAP_ICON_SIZE as f32
                        }),
                    );
                    let painter = ui.painter();
                    let texture = self.structure_icons.get(ui.ctx(), &marker.id);
                    painter.image(
                        texture.id(),
                        icon_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                        Color32::WHITE,
                    );
                }
                _ => {
                    ui.painter().circle_filled(pos, size + 3., PANEL);
                    ui.painter()
                        .circle_stroke(pos, size, Stroke::new(2.0_f32, color));
                    ui.painter().circle_filled(pos, 2., TEXT);
                }
            }
            if let Some(pointer) = pointer_pos {
                if let Some(d) = hit_distance(
                    pointer,
                    pos,
                    if matches!(
                        marker.kind,
                        MarkerKind::Ore
                            | MarkerKind::Structure
                            | MarkerKind::Spawn
                            | MarkerKind::Player
                    ) {
                        if selected { 21. } else { 18. }
                    } else {
                        13.
                    },
                ) && hit.as_ref().is_none_or(|(old, _)| d < *old)
                {
                    hit = Some((d, MapHit::Marker(marker.clone())));
                }
            }
        }

        let pointer = response
            .hover_pos()
            .filter(|p| rect.contains(*p) && !response.dragged());
        self.pointer_world =
            pointer.map(|p| screen_to_world(rect, center, self.prefs.blocks_per_point, p));
        let surface_source = self.pointer_world.and_then(|p| atlas.surface_source(p));
        let surface_column = self
            .pointer_world
            .and_then(|p| atlas.surface_probe(p))
            .cloned();
        atlas.set_marker_options(
            self.prefs.show_ores,
            self.prefs.show_structures,
            &self.prefs.ore_filter,
        );
        atlas.set_probe_options(
            (self.prefs.dimension == Dimension::Overworld || self.prefs.dimension.is_planetary())
                && self.prefs.tooltip.height,
            self.prefs.dimension == Dimension::Overworld && self.prefs.tooltip.trees,
        );
        atlas.hover(
            if monitoring {
                Some(center)
            } else {
                self.pointer_world
            },
            self.prefs.connected_highlight,
        );

        let tools_output = self.tools.paint(
            ui,
            rect,
            center,
            self.prefs.blocks_per_point,
            &response,
            &self.prefs.tools,
        );
        if response.drag_started() {
            self.selection_drag_active = ui.input(|input| input.modifiers.alt);
        }
        let mut pending_fit = None;
        if response.drag_stopped() && self.selection_drag_active {
            self.prefs.dock_tab = 1;
            self.prefs.control_dock = true;
            self.show_search = false;
            self.show_seed = false;
            self.show_preferences = false;
            self.show_tools = true;
            pending_fit = tools_output.selection;
            self.selection_drag_active = false;
        }
        if self.show_search {
            if let Ok(radius) = self.search_radius.parse::<f64>() {
                let covered = match self.prefs.search_scope {
                    Scope::Structures => {
                        if self.prefs.show_predictions {
                            radius.min(8192.)
                        } else {
                            radius
                        }
                    }
                    Scope::Ores | Scope::All => radius.min(2048.),
                    _ => radius.min(4096.),
                };
                map_tools::paint_search_area(
                    ui,
                    rect,
                    center,
                    self.prefs.blocks_per_point,
                    self.search_origin,
                    radius,
                    covered,
                );
            }
        }
        if response.dragged() && !tools_output.consumes_drag {
            let delta = ui.input(|i| i.pointer.delta());
            let p = pan_center(center, delta, self.prefs.blocks_per_point);
            self.prefs.center_x = p.x;
            self.prefs.center_z = p.z;
        }
        let mut pending_beneath_band = None;
        if response.hovered() {
            if self.prefs.dimension == Dimension::Beneath && !ui.ctx().wants_keyboard_input() {
                let previous = ui
                    .ctx()
                    .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp));
                let next = ui
                    .ctx()
                    .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown));
                if previous {
                    pending_beneath_band = Some((self.prefs.slice_y - 31).max(0));
                } else if next {
                    pending_beneath_band = Some((self.prefs.slice_y + 1).min(255));
                }
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0. {
                let before =
                    pointer.map(|p| screen_to_world(rect, center, self.prefs.blocks_per_point, p));
                self.prefs.blocks_per_point = (self.prefs.blocks_per_point
                    * (-scroll as f64 * 0.0015).exp())
                .clamp(0.125, 4096.);
                if let (Some(p), Some(world_before)) = (pointer, before) {
                    let after = screen_to_world(
                        rect,
                        Point {
                            x: self.prefs.center_x,
                            z: self.prefs.center_z,
                        },
                        self.prefs.blocks_per_point,
                        p,
                    );
                    self.prefs.center_x += world_before.x - after.x;
                    self.prefs.center_z += world_before.z - after.z;
                }
            }
            if response.clicked() {
                if let Some((_, MapHit::Marker(marker))) = &hit {
                    self.selected_marker = Some(marker.clone());
                } else if let Some((_, MapHit::Cluster(point, _))) = &hit {
                    self.selected_marker = None;
                    self.prefs.center_x = point.x;
                    self.prefs.center_z = point.z;
                    self.prefs.blocks_per_point = (self.prefs.blocks_per_point * 0.5).max(0.125);
                } else {
                    self.selected_marker = None;
                }
            }
        }

        let painter = ui.painter().with_clip_rect(rect);
        if let Some((_, MapHit::Cluster(_, count))) = &hit {
            painter.text(
                response.hover_pos().unwrap_or(rect.center()) + Vec2::new(14., 14.),
                Align2::LEFT_TOP,
                format!("{} nearby markers · click to zoom", count),
                FontId::proportional(10.),
                TEXT,
            );
        } else if let Some(MapHit::Marker(marker)) = hit.as_ref().map(|(_, h)| h) {
            let p = response.hover_pos().unwrap_or(rect.center());
            let tooltip_rect = Rect::from_min_size(
                Pos2::new(
                    (p.x + 14.).min(rect.right() - 254.),
                    (p.y + 14.).min(rect.bottom() - 98.),
                ),
                Vec2::new(245., 82.),
            );
            painter.rect_filled(
                tooltip_rect,
                CornerRadius::same(7),
                Color32::from_rgba_unmultiplied(16, 25, 31, 246),
            );
            painter.rect_stroke(
                tooltip_rect,
                CornerRadius::same(7),
                Stroke::new(1.0_f32, EDGE),
                egui::StrokeKind::Inside,
            );
            painter.text(
                tooltip_rect.left_top() + Vec2::new(11., 9.),
                Align2::LEFT_TOP,
                tooltip::display_marker_name(&marker.label),
                FontId::proportional(12.),
                TEXT,
            );
            painter.text(
                tooltip_rect.left_top() + Vec2::new(11., 30.),
                Align2::LEFT_TOP,
                if marker.confirmed {
                    format!("{} · confirmed", marker.namespace)
                } else {
                    marker.namespace.clone()
                },
                FontId::proportional(10.),
                MUTED,
            );
            painter.text(
                tooltip_rect.left_top() + Vec2::new(11., 49.),
                Align2::LEFT_TOP,
                format!(
                    "{}, {}  ·  click to pin",
                    marker.map_point().x.floor() as i64,
                    marker.map_point().z.floor() as i64
                ),
                FontId::monospace(10.),
                MUTED,
            );
        } else if self.selected_marker.is_none() && surface_column.is_some() {
            if let (Some(column), Some(pointer)) = (&surface_column, pointer) {
                egui::Area::new(Id::new("surface-hover-card"))
                    .fixed_pos(pointer + Vec2::new(14., 14.))
                    .order(egui::Order::Tooltip)
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        tooltip::show_surface(
                            ui,
                            column,
                            self.pointer_world.unwrap_or(center),
                            surface_source == Some(crate::atlas::SurfaceSource::Saved),
                            self.prefs.dimension,
                            self.prefs.slice_y,
                        );
                    });
            }
        } else if self.selected_marker.is_none() {
            if let (Some(probe), Some(pointer)) = (atlas.probe(), pointer) {
                let p = pointer + Vec2::new(14., 14.);
                let trees = if self.prefs.dimension == Dimension::Overworld {
                    atlas.tree_estimate()
                } else {
                    None
                };
                let region_summary =
                    if self.prefs.dimension == Dimension::Overworld && self.prefs.tooltip.region {
                        atlas.selection_summary()
                    } else {
                        None
                    };
                egui::Area::new(Id::new("map-hover-card"))
                    .fixed_pos(p)
                    .order(egui::Order::Tooltip)
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        tooltip::show_probe(
                            ui,
                            probe,
                            trees,
                            region_summary,
                            self.prefs.blocks_per_point >= 32.,
                            self.prefs.dimension,
                            self.prefs.slice_y,
                            &self.prefs.tooltip,
                        )
                    });
            }
        }
        if atlas.is_loading() {
            let status = painter.layout(
                atlas.status().to_owned(),
                FontId::proportional(10.),
                MUTED,
                (rect.width() - 32.).clamp(24., 440.),
            );
            painter.rect_filled(
                Rect::from_min_size(rect.left_top(), status.size() + Vec2::new(22., 14.)),
                CornerRadius::same(7),
                Color32::from_rgba_unmultiplied(15, 24, 30, 214),
            );
            painter.galley(rect.left_top() + Vec2::new(11., 7.), status, MUTED);
        }
        if let Some(y) = pending_beneath_band {
            self.set_beneath_band(y);
        }
        if let Some(selection) = pending_fit {
            let width = (selection.max_x - selection.min_x).max(1.);
            let height = (selection.max_z - selection.min_z).max(1.);
            self.prefs.center_x = (selection.min_x + selection.max_x) * 0.5;
            self.prefs.center_z = (selection.min_z + selection.max_z) * 0.5;
            self.prefs.blocks_per_point = (width / rect.width() as f64)
                .max(height / rect.height() as f64)
                .mul_add(1.1, 0.)
                .clamp(0.125, 4096.);
            self.prefs.navigation.record(self.current_view());
        }
        self.paint_saved_spots(ui, rect, center);
        self.map_context(ui, &response, rect, center);
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .frame(panel_frame())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.);
                    if let Some(seed) = self.preview_seed {
                        ui.label(RichText::new("Seed preview").size(10.).strong().color(TEXT));
                        ui.label(
                            RichText::new(format!("seed {seed} · default settings"))
                                .size(10.)
                                .monospace()
                                .color(MUTED),
                        );
                    } else if let Some(world) = self.selected.and_then(|i| self.worlds.get(i)) {
                        ui.label(RichText::new(&world.name).size(10.).strong().color(TEXT));
                        ui.label(
                            RichText::new(format!("seed {}", world.seed))
                                .size(10.)
                                .monospace()
                                .color(MUTED),
                        );
                    } else {
                        let message = if self.worlds.is_empty() {
                            "No Prism saves found · enter a seed to preview"
                        } else {
                            "No supported save selected · enter a seed or open the save list"
                        };
                        ui.label(RichText::new(message).size(10.).color(MUTED));
                    }
                    if let Some(atlas) = &mut self.atlas {
                        if self.prefs.allow_detail_rendering
                            && self.prefs.surface_detail
                            && self.prefs.layer == 0
                            && self.prefs.blocks_per_point <= 0.5
                        {
                            ui.label(RichText::new(atlas.surface_status()).size(10.).color(MUTED));
                        }
                        if atlas.tasks_busy() {
                            ui.spinner();
                        }
                        if !atlas.task_status().is_empty() {
                            ui.label(RichText::new(atlas.task_status()).size(10.).color(MUTED));
                        }
                        if atlas.engine_error().is_some()
                            && ui
                                .small_button("Restart scans")
                                .on_hover_text(atlas.engine_error().unwrap_or_default())
                                .clicked()
                        {
                            atlas.restart_engine();
                            if self.discovering || self.worlds.is_empty() {
                                self.discovering = true;
                                atlas.discover_worlds(self.instance.clone());
                            }
                        }
                    }
                    if let Some(p) = self.pointer_world {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "X {:.0}   Z {:.0}   ·   {:.1} blocks / px",
                                    p.x, p.z, self.prefs.blocks_per_point
                                ))
                                .size(10.)
                                .monospace()
                                .color(MUTED),
                            );
                        });
                    }
                });
            });
    }
}

impl eframe::App for TerraLensApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "diagnostics")]
        let engine_pid = self.atlas.as_ref().and_then(Atlas::engine_pid);
        #[cfg(feature = "diagnostics")]
        if let Some(stress) = self
            .responsiveness
            .as_mut()
            .and_then(|m| m.begin_frame(engine_pid))
        {
            self.prefs.center_x = stress.center.x;
            self.prefs.center_z = stress.center.z;
            self.prefs.blocks_per_point = stress.bpp;
            if let Some(y) = stress.slice_y {
                self.prefs.slice_y = beneath_band_top(y);
                if let Some(atlas) = &mut self.atlas {
                    atlas.set_dimension(Dimension::Beneath, y);
                }
            }
            if stress.search && !self.diagnostic_searching {
                if let Some(atlas) = &mut self.atlas {
                    atlas.find(stress.center, 4096., "copper".into(), false, Scope::Ores);
                }
                self.set_workspace(Some(Workspace::Find));
            }
            self.diagnostic_searching = stress.search;
        }
        self.poll_worlds(ctx);
        if let Some(path) = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .find_map(|file| file.path.clone())
        }) {
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                if let Some(atlas) = &mut self.atlas {
                    atlas.export(crate::exports::Request::ReadSession { path });
                }
            } else {
                self.open_source_file(path, None);
            }
        }
        self.poll_export(ctx);
        self.sync_workspace_flags();
        self.workspace_shortcuts(ctx);
        self.workspace_popups(ctx);
        self.find_shortcuts(ctx);
        self.update_display(ctx);
        self.poll_file_dialogs(ctx);
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.feature_rail(ctx);
        self.side_panel(ctx);
        self.sync_workspace_flags();
        self.shell_windows(ctx);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(8.))
            .show(ctx, |ui| self.map_view(ui));
        if self.search_select_after && self.atlas.as_ref().is_some_and(|a| !a.is_searching()) {
            self.search_select_after = false;
            self.select_search_result(0);
        }
        self.run_smoke_scenario(ctx);
        if self.smoke_inspect && self.structure_inspection.is_none() {
            if let Some(marker) = self.selected_marker.clone().filter(|m| m.bounds.is_some()) {
                self.view_marker_blocks(&marker);
            }
        }
        self.run_qol_scenario(ctx);
        self.handle_screenshot(ctx);
        #[cfg(feature = "diagnostics")]
        if self
            .responsiveness
            .as_mut()
            .is_some_and(|m| m.end_frame(ctx))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.screenshot.is_some()
            || self.is_monitoring()
            || self.qol_smoke
            || self.dimension_diagnostic
        {
            return;
        }
        eframe::set_value(storage, "preferences", &self.prefs);
    }
}

pub fn run() -> eframe::Result {
    let config = RunConfig::from_args();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1440., 900.])
        .with_min_inner_size([940., 620.]);
    if let Some(size) = config.window_size {
        viewport = viewport
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size);
    }
    let options = eframe::NativeOptions {
        // Wayland frame callbacks pace redraws. A synchronous EGL swap can block
        // the event loop indefinitely while a compositor hides the surface.
        vsync: std::env::var_os("WAYLAND_DISPLAY").is_none(),
        viewport,
        persist_window: !config.dimension_diagnostic && config.screenshot.is_none(),
        ..Default::default()
    };
    eframe::run_native(
        "TerraLens · TFG World Atlas",
        options,
        Box::new(move |cc| Ok(Box::new(TerraLensApp::new(cc, config)))),
    )
}

#[derive(Default)]
struct RunConfig {
    source_path: Option<PathBuf>,
    window_size: Option<[f32; 2]>,
    workspace: Option<Workspace>,
    initial_seed: Option<i64>,
    dimension: Option<Dimension>,
    slice_y: Option<i32>,
    surface_stress: bool,
    beneath_stress: bool,
    initial_view: Option<(Point, f64)>,
    dimension_diagnostic: bool,
    qol_smoke: bool,
    #[cfg(feature = "diagnostics")]
    responsiveness: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    smoke_test: bool,
    smoke_query: Option<String>,
    smoke_inspect: bool,
}
impl RunConfig {
    #[cfg(not(feature = "diagnostics"))]
    fn from_args() -> Self {
        let mut args = std::env::args().skip(1);
        let first = args.next();
        Self {
            source_path: first.and_then(|first| {
                if first == "--open" {
                    args.next().map(PathBuf::from)
                } else if !first.starts_with('-') {
                    Some(PathBuf::from(first))
                } else {
                    None
                }
            }),
            ..Self::default()
        }
    }

    #[cfg(feature = "diagnostics")]
    fn from_args() -> Self {
        let mut config = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--open" => config.source_path = args.next().map(PathBuf::from),
                "--inspect-structure" => {
                    config.smoke_test = true;
                    config.smoke_inspect = true;
                    config.dimension_diagnostic = true;
                }
                "--workspace" => {
                    config.workspace = match args.next().as_deref() {
                        Some("find") => Some(Workspace::Find),
                        Some("seed") => Some(Workspace::Seed),
                        Some("details") => Some(Workspace::Details),
                        Some("tools") => Some(Workspace::Tools),
                        _ => None,
                    };
                    config.dimension_diagnostic = true;
                }
                "--window-size" => {
                    let width = args.next().and_then(|s| s.parse::<f32>().ok());
                    let height = args.next().and_then(|s| s.parse::<f32>().ok());
                    if let (Some(w), Some(h)) = (width, height) {
                        if w.is_finite() && h.is_finite() {
                            config.window_size = Some([w.clamp(940., 4096.), h.clamp(620., 2160.)]);
                        }
                    }
                    config.dimension_diagnostic = true;
                }
                "--seed" => {
                    config.initial_seed = args.next().and_then(|s| s.parse().ok());
                }
                "--dimension" => {
                    config.dimension = args.next().and_then(|s| Dimension::from_id(&s));
                    config.dimension_diagnostic = true;
                }
                "--slice-y" => {
                    config.slice_y = args.next().and_then(|s| s.parse().ok());
                    config.dimension_diagnostic = true;
                }
                "--surface-stress" => {
                    config.surface_stress = true;
                    config.responsiveness = args.next().map(PathBuf::from);
                }
                "--beneath-stress" => {
                    config.beneath_stress = true;
                    config.responsiveness = args.next().map(PathBuf::from);
                }
                "--view" => {
                    let coords: Vec<_> = args.by_ref().take(3).map(|v| v.parse::<f64>()).collect();
                    if let [Ok(x), Ok(z), Ok(bpp)] = coords.as_slice()
                        && x.is_finite()
                        && z.is_finite()
                        && bpp.is_finite()
                        && x.abs() <= 29_999_984.
                        && z.abs() <= 29_999_984.
                    {
                        config.initial_view =
                            Some((Point { x: *x, z: *z }, bpp.clamp(0.125, 4096.)));
                    }
                }
                "--responsiveness" => config.responsiveness = args.next().map(PathBuf::from),
                "--screenshot" => config.screenshot = args.next().map(PathBuf::from),
                "--qol-smoke" => config.qol_smoke = true,
                "--smoke-test" => config.smoke_test = true,
                "--smoke-query" => {
                    config.smoke_query = args.next();
                    config.smoke_test = true;
                }
                _ => {}
            }
        }
        config
    }
}

impl TerraLensApp {
    fn run_smoke_scenario(&mut self, ctx: &egui::Context) {
        if !self.smoke_test || self.discovering && self.preview_seed.is_none() {
            return;
        }
        if !self.smoke_started {
            let origin = self.current_center();
            self.search_x = format!("{:.0}", origin.x);
            self.search_z = format!("{:.0}", origin.z);
            self.set_workspace(Some(Workspace::Find));
            self.prefs.show_predictions = false;
            self.search_origin = origin;
            if let Some(query) = self.smoke_query.clone() {
                self.search_query = query;
                self.prefs.search_scope = Scope::All;
                self.submit_search(true);
            } else if let Some(atlas) = &mut self.atlas {
                atlas.search(origin, 100_000., String::new(), false);
                self.prefs.search_scope = Scope::Structures;
            }
            self.smoke_started = true;
            self.smoke_started_at = Some(Instant::now());
        } else if self.selected_marker.is_none() {
            if let Some(atlas) = &self.atlas {
                if !atlas.is_loading() {
                    let selected = atlas
                        .search_results()
                        .first()
                        .or_else(|| atlas.saved_structures().first())
                        .or_else(|| atlas.markers().first())
                        .cloned();
                    if let Some(marker) = selected {
                        self.set_center(marker.map_point());
                        self.selected_marker = Some(marker);
                        self.smoke_selection_frame = Some(self.screenshot_frame);
                    }
                }
            }
        }
        if self
            .smoke_started_at
            .is_some_and(|t| t.elapsed() > Duration::from_secs(45))
        {
            self.smoke_test = false;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        self.screenshot_frame = self.screenshot_frame.saturating_add(1);
        if self.screenshot.is_some() && !self.screenshot_sent && self.screenshot_frame > 8 {
            let ready =
                !self.discovering && self.atlas.as_ref().is_none_or(|atlas| !atlas.is_loading());
            let timed_out = self
                .screenshot_started_at
                .is_some_and(|t| t.elapsed() > Duration::from_secs(60));
            let scenario_ready = (!self.qol_smoke || self.qol_stage >= 3)
                && (!self.smoke_test
                    || self
                        .smoke_selection_frame
                        .is_some_and(|frame| self.screenshot_frame >= frame + 3));
            if (ready && scenario_ready) || timed_out {
                let path = self
                    .screenshot
                    .as_ref()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(path)));
                self.screenshot_sent = true;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
        let events = ctx.input(|input| input.raw.events.clone());
        for event in events {
            if let egui::Event::Screenshot {
                user_data, image, ..
            } = event
            {
                let Some(path) = user_data
                    .data
                    .as_deref()
                    .and_then(|data| data.downcast_ref::<String>())
                else {
                    continue;
                };
                if let Some(parent) = PathBuf::from(path)
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                {
                    if let Err(error) = std::fs::create_dir_all(parent) {
                        eprintln!("TerraLens screenshot directory failed: {error}");
                    }
                }
                let mut rgba = Vec::with_capacity(image.pixels.len() * 4);
                for pixel in &image.pixels {
                    rgba.extend_from_slice(&pixel.to_array());
                }
                if let Err(error) = image::save_buffer(
                    path,
                    &rgba,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    eprintln!("TerraLens screenshot failed: {error}");
                } else {
                    eprintln!("TerraLens screenshot saved to {path}");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn install_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = Color32::from_rgb(12, 19, 24);
    style.visuals.faint_bg_color = PANEL_RAISED;
    style.visuals.selection.bg_fill = Color32::from_rgb(38, 103, 95);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, TEAL);
    style.visuals.widgets.inactive.bg_fill = PANEL_RAISED;
    style.visuals.widgets.inactive.fg_stroke.color = TEXT;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 53, 60);
    style.visuals.widgets.hovered.fg_stroke.color = TEXT;
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(38, 103, 95);
    style.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    style.spacing.item_spacing = Vec2::new(5., 4.);
    style.spacing.button_padding = Vec2::new(6., 3.);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(2);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(2);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(2);
    style.visuals.selection.bg_fill = Color32::from_rgb(53, 91, 133);
    ctx.set_style(style);
}

fn panel_frame() -> Frame {
    Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0_f32, EDGE))
        .inner_margin(egui::Margin::symmetric(8, 5))
}
fn number_field(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.).color(MUTED));
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(66.)
                .font(egui::TextStyle::Monospace),
        )
        .changed()
    })
    .inner
}

fn consume_find_shortcut(ctx: &egui::Context) -> bool {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F))
}

fn next_search_index(current: Option<usize>, count: usize, previous: bool) -> usize {
    match (current.filter(|i| *i < count), previous) {
        (Some(i), true) => (i + count - 1) % count,
        (Some(i), false) => (i + 1) % count,
        (None, true) => count - 1,
        (None, false) => 0,
    }
}

fn cluster_count(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f32 / 1000.)
    } else {
        n.to_string()
    }
}

fn pan_center(center: Point, delta: Vec2, bpp: f64) -> Point {
    Point {
        x: center.x - delta.x as f64 * bpp,
        z: center.z - delta.y as f64 * bpp,
    }
}

fn hit_distance(pointer: Pos2, target: Pos2, radius: f32) -> Option<f32> {
    let distance = pointer.distance(target);
    (distance <= radius).then_some(distance)
}

fn map_bounds(rect: Rect, center: Point, bpp: f64) -> crate::model::Bounds {
    let half_w = rect.width() as f64 * bpp * 0.5;
    let half_h = rect.height() as f64 * bpp * 0.5;
    crate::model::Bounds {
        min_x: center.x - half_w,
        max_x: center.x + half_w,
        min_z: center.z - half_h,
        max_z: center.z + half_h,
    }
}
fn world_to_screen(rect: Rect, center: Point, bpp: f64, p: Point) -> Pos2 {
    Pos2::new(
        rect.center().x + ((p.x - center.x) / bpp) as f32,
        rect.center().y + ((p.z - center.z) / bpp) as f32,
    )
}
fn screen_to_world(rect: Rect, center: Point, bpp: f64, p: Pos2) -> Point {
    Point {
        x: center.x + (p.x - rect.center().x) as f64 * bpp,
        z: center.z + (p.y - rect.center().y) as f64 * bpp,
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;

    #[test]
    fn quieter_defaults_migrate_once_and_preserve_later_choices() {
        let mut prefs: Preferences = serde_json::from_value(serde_json::json!({
            "world_id": "kept", "connected_highlight": true,
            "surface_detail": true, "allow_detail_rendering": true
        }))
        .unwrap();
        prefs.upgrade_defaults();
        assert!(
            !prefs.connected_highlight && !prefs.surface_detail && !prefs.allow_detail_rendering
        );
        assert_eq!(prefs.world_id, "kept");
        prefs.connected_highlight = true;
        prefs.surface_detail = true;
        prefs.allow_detail_rendering = true;
        let mut restored: Preferences =
            serde_json::from_value(serde_json::to_value(prefs).unwrap()).unwrap();
        restored.upgrade_defaults();
        assert!(
            restored.connected_highlight
                && restored.surface_detail
                && restored.allow_detail_rendering
        );
    }

    #[test]
    fn structure_defaults_upgrade_once_without_resetting_later_visibility_choices() {
        let mut prefs: Preferences = serde_json::from_value(serde_json::json!({
            "defaults_version": 1,
            "connected_highlight": true, "surface_detail": true, "allow_detail_rendering": true,
            "display": { "hidden_structure_ids": ["tfg:mineshaft/mineshaft"] }
        }))
        .unwrap();
        prefs.upgrade_defaults();
        assert_eq!(prefs.defaults_version, 2);
        assert!(prefs.connected_highlight && prefs.surface_detail && prefs.allow_detail_rendering);
        for id in [
            "tfg:mineshaft/mineshaft",
            "minecraft:nether_fossil",
            "tfg:ocean/ocean_moai_0",
            "tfg:aqueduct/aqueduct",
        ] {
            assert!(
                prefs
                    .display
                    .hidden_structure_ids
                    .iter()
                    .any(|hidden| hidden == id),
                "{id}"
            );
        }
        prefs
            .display
            .set_feature_visible(&MarkerKind::Structure, "tfg:ocean/ocean_moai_0", true);
        let mut restored: Preferences =
            serde_json::from_value(serde_json::to_value(prefs).unwrap()).unwrap();
        restored.upgrade_defaults();
        assert!(
            !restored
                .display
                .hidden_structure_ids
                .iter()
                .any(|id| id == "tfg:ocean/ocean_moai_0")
        );
        assert!(
            restored
                .display
                .hidden_structure_ids
                .iter()
                .any(|id| id == "tfg:aqueduct/aqueduct")
        );
    }

    #[test]
    fn ctrl_f_is_consumed_once_without_matching_plain_f() {
        let ctx = egui::Context::default();
        let event = |modifiers| egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let _ = ctx.run(
            egui::RawInput {
                events: vec![event(egui::Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| {
                assert!(!consume_find_shortcut(ctx));
            },
        );
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let _ = ctx.run(
            egui::RawInput {
                events: vec![event(modifiers)],
                ..Default::default()
            },
            |ctx| {
                assert!(consume_find_shortcut(ctx));
                assert!(!consume_find_shortcut(ctx));
            },
        );
    }

    #[test]
    fn find_navigation_wraps_in_both_directions() {
        assert_eq!(next_search_index(None, 3, false), 0);
        assert_eq!(next_search_index(None, 3, true), 2);
        assert_eq!(next_search_index(Some(2), 3, false), 0);
        assert_eq!(next_search_index(Some(0), 3, true), 2);
        assert_eq!(next_search_index(Some(5), 3, false), 0);
        assert_eq!(next_search_index(Some(0), 1, true), 0);
    }

    #[test]
    fn nearest_pin_hit_is_within_radius_and_uses_nearest_target() {
        let pointer = Pos2::new(100., 100.);
        let farther = hit_distance(pointer, Pos2::new(108., 100.), 13.).unwrap();
        let nearer = hit_distance(pointer, Pos2::new(102., 101.), 13.).unwrap();
        assert!(nearer < farther);
        assert!(hit_distance(pointer, Pos2::new(114., 100.), 13.).is_none());
    }

    #[test]
    fn drag_pans_world_opposite_to_pointer_motion() {
        let center = Point { x: 500., z: -200. };
        let moved = pan_center(center, Vec2::new(10., -4.), 32.);
        assert_eq!(moved, Point { x: 180., z: -72. });
    }

    #[test]
    fn pointer_world_round_trips_through_screen_coordinates() {
        let rect = Rect::from_min_size(Pos2::new(10., 20.), Vec2::new(800., 600.));
        let center = Point {
            x: -12_345.,
            z: 67_890.,
        };
        let point = Point {
            x: -11_321.,
            z: 65_778.,
        };
        let screen = world_to_screen(rect, center, 8., point);
        let result = screen_to_world(rect, center, 8., screen);
        assert!((result.x - point.x).abs() < 0.01);
        assert!((result.z - point.z).abs() < 0.01);
    }

    #[test]
    fn planetary_layers_allow_only_real_planetary_views() {
        for dimension in [Dimension::Moon, Dimension::Mars] {
            assert!(TerraLensApp::layer_supported(dimension, Layer::Biomes));
            assert!(TerraLensApp::layer_supported(dimension, Layer::Height));
            for layer in [
                Layer::Temperature,
                Layer::Rainfall,
                Layer::Geology,
                Layer::Forest,
                Layer::Hotspots,
            ] {
                assert!(!TerraLensApp::layer_supported(dimension, layer));
            }
        }
        assert!(!TerraLensApp::layer_supported(
            Dimension::Moon,
            Layer::Water
        ));
        assert!(TerraLensApp::layer_supported(Dimension::Mars, Layer::Water));
    }

    #[test]
    fn non_overworld_search_scopes_exclude_tfc_only_layers() {
        for dimension in [Dimension::Beneath, Dimension::Moon, Dimension::Mars] {
            for scope in [Scope::All, Scope::Structures, Scope::Ores, Scope::Biomes] {
                assert!(TerraLensApp::scope_supported(dimension, scope));
            }
            for scope in [Scope::Geology, Scope::Forests, Scope::Trees] {
                assert!(!TerraLensApp::scope_supported(dimension, scope));
            }
        }
        assert!(TerraLensApp::scope_supported(
            Dimension::Overworld,
            Scope::Forests
        ));
    }

    #[test]
    fn area_analysis_requires_overworld_climate_and_terrain() {
        assert!(TerraLensApp::area_analysis_supported(Dimension::Overworld));
        for dimension in [Dimension::Beneath, Dimension::Moon, Dimension::Mars] {
            assert!(!TerraLensApp::area_analysis_supported(dimension));
        }
    }
}
