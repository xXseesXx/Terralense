//! Persistent map emphasis and marker visibility settings.
//!
//! These preferences are deliberately independent of tile generation. The renderer can
//! apply the same land-only predicate to semantic tile samples without changing biome data.
use crate::{
    atlas::Layer,
    biome,
    dimension::Dimension,
    model::{Marker, MarkerKind},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    #[default]
    Natural,
    Colorblind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    #[default]
    Exploration,
    Prospecting,
    Settlement,
    Forestry,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadingFilter {
    #[default]
    Nearest,
    Smooth,
}

impl LoadingFilter {
    pub const ALL: [Self; 2] = [Self::Nearest, Self::Smooth];

    pub fn label(self) -> &'static str {
        match self {
            Self::Nearest => "Nearest neighbor",
            Self::Smooth => "Smooth",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapDetail {
    #[default]
    Block,
    TwoBlocks,
    FourBlocks,
    EightBlocks,
    Chunk,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainPreset {
    #[default]
    Fast,
    Balanced,
    Full,
    Custom,
}

impl TerrainPreset {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fast => "Fast",
            Self::Balanced => "Balanced",
            Self::Full => "Full",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct TerrainDetailOptions {
    pub precise_biome_boundaries: bool,
    pub physical_surface: bool,
    pub caves: bool,
    pub rivers_lakes: bool,
    pub volcanoes: bool,
    pub springs: bool,
    pub exposed_dikes: bool,
    pub large_rocks_special_soils: bool,
    pub surface_litter: bool,
}

impl Default for TerrainDetailOptions {
    fn default() -> Self {
        Self::full()
    }
}

impl TerrainDetailOptions {
    pub const fn fast() -> Self {
        Self {
            precise_biome_boundaries: false,
            physical_surface: false,
            caves: false,
            rivers_lakes: false,
            volcanoes: false,
            springs: false,
            exposed_dikes: false,
            large_rocks_special_soils: false,
            surface_litter: false,
        }
    }

    pub const fn balanced() -> Self {
        Self {
            precise_biome_boundaries: false,
            physical_surface: true,
            caves: false,
            rivers_lakes: true,
            volcanoes: true,
            springs: false,
            exposed_dikes: false,
            large_rocks_special_soils: true,
            surface_litter: false,
        }
    }

    pub const fn full() -> Self {
        Self {
            precise_biome_boundaries: true,
            physical_surface: true,
            caves: true,
            rivers_lakes: true,
            volcanoes: true,
            springs: true,
            exposed_dikes: true,
            large_rocks_special_soils: true,
            surface_litter: true,
        }
    }

    pub const fn preset(self) -> TerrainPreset {
        if self.equals(Self::fast()) {
            TerrainPreset::Fast
        } else if self.equals(Self::balanced()) {
            TerrainPreset::Balanced
        } else if self.equals(Self::full()) {
            TerrainPreset::Full
        } else {
            TerrainPreset::Custom
        }
    }

    const fn equals(self, other: Self) -> bool {
        self.precise_biome_boundaries == other.precise_biome_boundaries
            && self.physical_surface == other.physical_surface
            && self.caves == other.caves
            && self.rivers_lakes == other.rivers_lakes
            && self.volcanoes == other.volcanoes
            && self.springs == other.springs
            && self.exposed_dikes == other.exposed_dikes
            && self.large_rocks_special_soils == other.large_rocks_special_soils
            && self.surface_litter == other.surface_litter
    }
}

fn legacy_terrain_detail() -> TerrainDetailOptions {
    TerrainDetailOptions {
        surface_litter: false,
        ..TerrainDetailOptions::full()
    }
}

impl MapDetail {
    pub const ALL: [Self; 5] = [
        Self::Block,
        Self::TwoBlocks,
        Self::FourBlocks,
        Self::EightBlocks,
        Self::Chunk,
    ];

    pub const fn min_tile_zoom(self) -> i32 {
        match self {
            Self::Block => 0,
            Self::TwoBlocks => 1,
            Self::FourBlocks => 2,
            Self::EightBlocks => 3,
            Self::Chunk => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Block => "1 block",
            Self::TwoBlocks => "2 blocks",
            Self::FourBlocks => "4 blocks",
            Self::EightBlocks => "8 blocks",
            Self::Chunk => "16 blocks",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayPrefs {
    /// `None` preserves the legacy single-layer view for old sessions and API callers.
    pub layer_stack: Option<Vec<LayerBlend>>,
    pub grass_colors: bool,
    pub grass_opacity: f32,
    pub trees: bool,
    pub tree_opacity: f32,
    /// Fraction of the simulated calendar year; None retains annual-mean colors.
    pub season: Option<f32>,
    pub palette: Palette,
    pub preset: Preset,
    pub highlighted_biomes: Vec<u16>,
    pub highlighted_rocks: Vec<String>,
    pub temperature_enabled: bool,
    pub temperature_min: f32,
    pub temperature_max: f32,
    pub rainfall_enabled: bool,
    pub rainfall_min: f32,
    pub rainfall_max: f32,
    pub height_shading: bool,
    pub hillshade_strength: f32,
    pub contours: crate::contours::ContourPrefs,
    pub loading_filter: LoadingFilter,
    pub max_detail: MapDetail,
    pub hidden_ore_ids: Vec<String>,
    pub hidden_structure_ids: Vec<String>,
    pub hidden_volcano_ids: Vec<String>,
    pub feature_bar_ores: bool,
    /// Generation work for optional terrain detail. Legacy sessions retain full detail.
    #[serde(default = "legacy_terrain_detail")]
    pub terrain: TerrainDetailOptions,
}

impl Default for DisplayPrefs {
    fn default() -> Self {
        Self {
            layer_stack: None,
            grass_colors: false,
            grass_opacity: 1.0,
            trees: false,
            tree_opacity: 1.0,
            season: None,
            palette: Palette::Natural,
            preset: Preset::Exploration,
            highlighted_biomes: Vec::new(),
            highlighted_rocks: Vec::new(),
            temperature_enabled: false,
            temperature_min: -20.,
            temperature_max: 20.,
            rainfall_enabled: false,
            rainfall_min: 0.,
            rainfall_max: 500.,
            height_shading: false,
            hillshade_strength: 1.0,
            contours: Default::default(),
            loading_filter: LoadingFilter::Nearest,
            max_detail: MapDetail::Block,
            hidden_ore_ids: Vec::new(),
            hidden_structure_ids: default_hidden_structure_ids().to_vec(),
            hidden_volcano_ids: Vec::new(),
            feature_bar_ores: false,
            terrain: TerrainDetailOptions::fast(),
        }
    }
}

/// Resolve concrete IDs once so group defaults and individual toggles use the same catalog.
fn default_hidden_structure_ids() -> &'static [String] {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        let mut ids: Vec<_> = Dimension::ALL
            .into_iter()
            .flat_map(|dimension| crate::structure_support::catalog(dimension))
            .filter(|(id, _)| {
                crate::structure_loot::ruin_group(id)
                    == Some(crate::structure_loot::RuinGroup::Lootless)
                    || matches!(
                        id.as_str(),
                        "minecraft:nether_fossil"
                            | "tfg:illagers/forest_roaming"
                            | "tfg:moon/cheese_ores"
                            | "tfg:moon/moon_rabbit_houses"
                    )
            })
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    })
}

impl DisplayPrefs {
    pub fn terrain_preset(&self) -> TerrainPreset {
        self.terrain.preset()
    }

    /// Apply a sampling preset without changing marker visibility filters.
    pub fn apply_terrain_preset(&mut self, preset: TerrainPreset) {
        self.terrain = match preset {
            TerrainPreset::Fast => TerrainDetailOptions::fast(),
            TerrainPreset::Balanced => TerrainDetailOptions::balanced(),
            TerrainPreset::Full => TerrainDetailOptions::full(),
            TerrainPreset::Custom => return,
        };
        let biome_stack = Some(vec![LayerBlend {
            layer: Layer::Biomes,
            opacity: 1.0,
        }]);
        self.layer_stack = biome_stack;
        match preset {
            TerrainPreset::Fast => {
                self.grass_colors = false;
                self.trees = false;
                self.height_shading = false;
                self.contours.enabled = false;
            }
            TerrainPreset::Balanced => {
                self.grass_colors = true;
                self.trees = false;
                self.height_shading = true;
                self.contours.enabled = false;
            }
            TerrainPreset::Full => {
                self.max_detail = MapDetail::Block;
                self.grass_colors = true;
                self.trees = true;
                self.height_shading = true;
                self.contours.enabled = true;
            }
            TerrainPreset::Custom => {}
        }
    }

    pub fn simulated_season(&self) -> Option<f32> {
        self.season
            .filter(|value| value.is_finite())
            .map(|value| value.rem_euclid(1.))
    }

    pub fn supports_layer(dimension: Dimension, layer: Layer) -> bool {
        if layer == Layer::Ground {
            return dimension == Dimension::Overworld;
        }
        match dimension {
            Dimension::Overworld => true,
            Dimension::Beneath => layer == Layer::Biomes,
            Dimension::Moon => matches!(layer, Layer::Biomes | Layer::Height),
            Dimension::Mars => matches!(layer, Layer::Biomes | Layer::Height | Layer::Water),
        }
    }

    /// Enabled layers in stable back-to-front order. Call once when building a tile/export plan,
    /// never in a per-pixel loop. Empty explicit stacks remain empty (grass-only is valid).
    pub fn active_layers(&self, fallback: Layer, dimension: Dimension) -> Vec<LayerBlend> {
        let stack = self.layer_stack.as_deref().unwrap_or(&[]);
        if self.layer_stack.is_none() {
            return vec![LayerBlend {
                layer: fallback,
                opacity: 1.0,
            }];
        }
        let mut seen = HashSet::new();
        let mut layers: Vec<_> = stack
            .iter()
            .filter(|&blend| {
                seen.insert(blend.layer) && Self::supports_layer(dimension, blend.layer)
            })
            .map(|blend| LayerBlend {
                layer: blend.layer,
                opacity: blend.opacity.clamp(0.0, 1.0),
            })
            .collect();
        if !stack.is_empty() && layers.is_empty() {
            layers.push(LayerBlend {
                layer: Layer::Biomes,
                opacity: 1.0,
            });
        }
        layers
    }

    pub fn layer_enabled(&self, layer: Layer, fallback: Layer) -> bool {
        self.layer_stack
            .as_ref()
            .map_or(layer == fallback, |stack| {
                stack.iter().any(|blend| blend.layer == layer)
            })
    }

    pub fn set_layer_enabled(&mut self, layer: Layer, enabled: bool, fallback: Layer) {
        let stack = self.layer_stack.get_or_insert_with(|| {
            vec![LayerBlend {
                layer: fallback,
                opacity: 1.0,
            }]
        });
        if enabled {
            if !stack.iter().any(|blend| blend.layer == layer) {
                stack.push(LayerBlend {
                    layer,
                    opacity: if stack.is_empty() && !self.grass_colors {
                        1.0
                    } else {
                        0.55
                    },
                });
            }
        } else {
            stack.retain(|blend| blend.layer != layer);
        }
    }

    pub fn set_layer_opacity(&mut self, layer: Layer, opacity: f32, fallback: Layer) {
        let stack = self.layer_stack.get_or_insert_with(|| {
            vec![LayerBlend {
                layer: fallback,
                opacity: 1.0,
            }]
        });
        let opacity = opacity.clamp(0.0, 1.0);
        if let Some(blend) = stack.iter_mut().find(|blend| blend.layer == layer) {
            blend.opacity = opacity;
        } else {
            stack.push(LayerBlend { layer, opacity });
        }
    }

    /// Move among the current dimension's enabled layers. Negative offsets move
    /// toward the background; positive offsets move toward the foreground.
    /// Unsupported layers retain their slots and preferences across dimensions.
    pub fn move_layer(&mut self, layer: Layer, offset: i32, fallback: Layer, dimension: Dimension) {
        if offset == 0 || !Self::supports_layer(dimension, layer) {
            return;
        }
        let stack = self.layer_stack.get_or_insert_with(|| {
            vec![LayerBlend {
                layer: fallback,
                opacity: 1.,
            }]
        });
        let slots: Vec<usize> = stack
            .iter()
            .enumerate()
            .filter_map(|(index, blend)| {
                Self::supports_layer(dimension, blend.layer).then_some(index)
            })
            .collect();
        let Some(start) = slots.iter().position(|&index| stack[index].layer == layer) else {
            return;
        };
        let end = (start as i64 + i64::from(offset)).clamp(0, slots.len().saturating_sub(1) as i64)
            as usize;
        if start < end {
            for i in start..end {
                stack.swap(slots[i], slots[i + 1]);
            }
        } else {
            for i in (end..start).rev() {
                stack.swap(slots[i], slots[i + 1]);
            }
        }
    }

    /// Used once when upgrading an existing installation; later user toggles are retained.
    pub fn apply_structure_visibility_defaults(&mut self) {
        for id in default_hidden_structure_ids() {
            self.set_feature_visible(&MarkerKind::Structure, id, false);
        }
    }

    /// Undo the former name-based Ruins default once for buildings with possible loot.
    pub fn restore_building_visibility(&mut self) {
        self.hidden_structure_ids.retain(|id| {
            id != "tfg:illagers/norse_village_ruined"
                && crate::structure_loot::ruin_group(id)
                    != Some(crate::structure_loot::RuinGroup::Building)
        });
    }

    /// Set one stable feature type's visibility. Occurrence coordinates are stripped so one
    /// click applies to every generated or saved instance of that type.
    pub fn set_feature_visible(&mut self, kind: &MarkerKind, id: &str, visible: bool) {
        let id = id.split('@').next().unwrap_or(id);
        let hidden = match kind {
            MarkerKind::Ore => &mut self.hidden_ore_ids,
            MarkerKind::Structure => &mut self.hidden_structure_ids,
            MarkerKind::Volcano => &mut self.hidden_volcano_ids,
            _ => return,
        };
        if visible {
            hidden.retain(|hidden_id| hidden_id != id);
        } else if !hidden.iter().any(|hidden_id| hidden_id == id) {
            hidden.push(id.to_owned());
        }
    }

    /// Show only one type within its group; all other catalog types remain individually
    /// available to restore. This does not alter the global group visibility switches.
    pub fn solo_feature(
        &mut self,
        kind: &MarkerKind,
        ids: impl IntoIterator<Item = String>,
        id: &str,
    ) {
        let id = id.split('@').next().unwrap_or(id);
        let hidden = match kind {
            MarkerKind::Ore => &mut self.hidden_ore_ids,
            MarkerKind::Structure => &mut self.hidden_structure_ids,
            MarkerKind::Volcano => &mut self.hidden_volcano_ids,
            _ => return,
        };
        *hidden = ids
            .into_iter()
            .filter(|candidate| candidate != id)
            .collect();
    }

    pub fn set_feature_group_visible(&mut self, kind: &MarkerKind, ids: &[String], visible: bool) {
        for id in ids {
            self.set_feature_visible(kind, id, visible);
        }
    }

    pub fn same_raster(&self, other: &Self) -> bool {
        self.palette == other.palette
            && self.simulated_season() == other.simulated_season()
            && self.layer_stack == other.layer_stack
            && self.grass_colors == other.grass_colors
            && (!self.grass_colors || self.grass_opacity == other.grass_opacity)
            && self.trees == other.trees
            && (!self.trees || self.tree_opacity == other.tree_opacity)
            && self.highlighted_biomes == other.highlighted_biomes
            && self.highlighted_rocks == other.highlighted_rocks
            && self.temperature_enabled == other.temperature_enabled
            && (!self.temperature_enabled
                || self.temperature_min == other.temperature_min
                    && self.temperature_max == other.temperature_max)
            && self.rainfall_enabled == other.rainfall_enabled
            && (!self.rainfall_enabled
                || self.rainfall_min == other.rainfall_min
                    && self.rainfall_max == other.rainfall_max)
            && self.height_shading == other.height_shading
            && (!self.height_shading || self.hillshade_strength == other.hillshade_strength)
            && self.contours.same_raster(&other.contours)
            && self.terrain == other.terrain
    }

    pub fn has_highlight_filter(&self) -> bool {
        !self.highlighted_biomes.is_empty()
            || !self.highlighted_rocks.is_empty()
            || self.temperature_enabled
            || self.rainfall_enabled
    }

    pub fn biome_highlighted(&self, biome_id: u16) -> bool {
        self.highlighted_biomes.contains(&biome_id)
    }

    /// Returns true only for selected land samples. All active criteria are combined.
    /// Oceans are intentionally excluded even if an old preference contains an ocean ID.
    pub fn matches_land_sample(
        &self,
        biome_id: u16,
        temperature: f32,
        rainfall: f32,
        rocks: &[String],
    ) -> bool {
        if !self.has_highlight_filter() || biome::is_ocean(biome_id) {
            return false;
        }
        if !self.highlighted_biomes.is_empty() && !self.highlighted_biomes.contains(&biome_id) {
            return false;
        }
        if self.temperature_enabled
            && !(self.temperature_min..=self.temperature_max).contains(&temperature)
        {
            return false;
        }
        if self.rainfall_enabled && !(self.rainfall_min..=self.rainfall_max).contains(&rainfall) {
            return false;
        }
        if !self.highlighted_rocks.is_empty()
            && !rocks
                .iter()
                .any(|rock| self.highlighted_rocks.contains(rock))
        {
            return false;
        }
        true
    }

    pub fn marker_visible(&self, marker: &Marker, blocks_per_point: f64) -> bool {
        if blocks_per_point > crate::features::marker_max_blocks_per_point(marker) {
            return false;
        }
        match &marker.kind {
            MarkerKind::Ore => !self
                .hidden_ore_ids
                .iter()
                .any(|id| id == marker.id.split('@').next().unwrap_or(&marker.id)),
            MarkerKind::Structure => !self.hidden_structure_ids.iter().any(|id| {
                id == marker.id.split('@').next().unwrap_or(&marker.id) || id == &marker.label
            }),
            MarkerKind::Volcano => !self.hidden_volcano_ids.iter().any(|id| {
                id == marker.id.split('@').next().unwrap_or(&marker.id) || id == &marker.label
            }),
            _ => true,
        }
    }

    /// The Beneath Y band filters deposits, while structure and location pins span all bands.
    /// The map and its exports share this rule so changing height cannot lose a structure pin.
    pub fn marker_visible_in_view(
        &self,
        marker: &Marker,
        blocks_per_point: f64,
        dimension: Dimension,
        slice_y: i32,
    ) -> bool {
        self.marker_visible(marker, blocks_per_point)
            && (dimension != Dimension::Beneath
                || matches!(
                    marker.kind,
                    MarkerKind::Structure | MarkerKind::Spawn | MarkerKind::Player
                )
                || {
                    let (min, max) = dimension.band(slice_y);
                    marker.intersects_band(min, max)
                })
    }

    pub fn visible_id_set(
        &self,
        kind: &MarkerKind,
        ids: impl Iterator<Item = String>,
    ) -> HashSet<String> {
        let hidden = match kind {
            MarkerKind::Ore => &self.hidden_ore_ids,
            MarkerKind::Structure => &self.hidden_structure_ids,
            MarkerKind::Volcano => &self.hidden_volcano_ids,
            _ => return ids.collect(),
        };
        ids.filter(|id| !hidden.contains(id)).collect()
    }

    pub fn apply_preset(&mut self, preset: Preset) -> Layer {
        self.preset = preset;
        match preset {
            Preset::Exploration => Layer::Biomes,
            Preset::Prospecting => Layer::Geology,
            Preset::Settlement => Layer::Temperature,
            Preset::Forestry => Layer::Forest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerBlend {
    pub layer: Layer,
    pub opacity: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_calendar_roundtrips_and_invalidates_only_color_identity() {
        let average: DisplayPrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(average.simulated_season(), None);
        let autumn = DisplayPrefs {
            season: Some(0.8),
            ..average.clone()
        };
        let restored: DisplayPrefs =
            serde_json::from_str(&serde_json::to_string(&autumn).unwrap()).unwrap();
        assert_eq!(restored.simulated_season(), Some(0.8));
        assert!(!average.same_raster(&autumn));
        let january = DisplayPrefs {
            season: Some(0.),
            ..average.clone()
        };
        let wrapped = DisplayPrefs {
            season: Some(1.),
            ..average.clone()
        };
        assert!(january.same_raster(&wrapped));
        let invalid = DisplayPrefs {
            season: Some(f32::NAN),
            ..average.clone()
        };
        assert!(average.same_raster(&invalid));
    }

    #[test]
    fn hillshade_is_part_of_raster_identity_only_when_enabled() {
        let base = DisplayPrefs::default();
        let mut changed = base.clone();
        changed.hillshade_strength = 0.9;
        assert!(base.same_raster(&changed));
        changed.height_shading = true;
        assert!(!base.same_raster(&changed));
        let mut other_enabled = changed.clone();
        other_enabled.hillshade_strength = 0.7;
        assert!(!changed.same_raster(&other_enabled));
    }

    #[test]
    fn old_preferences_default_to_unshaded_relief() {
        let prefs: DisplayPrefs = serde_json::from_str("{}").unwrap();
        assert!(prefs.layer_stack.is_none());
        assert_eq!(prefs.terrain, legacy_terrain_detail());
        assert!(!prefs.terrain.surface_litter);
        assert!(!prefs.grass_colors);
        assert!(!prefs.trees);
        assert!(!prefs.height_shading);
        assert_eq!(prefs.hillshade_strength, 1.0);
        assert!(!prefs.contours.enabled);
        assert_eq!(prefs.loading_filter, LoadingFilter::Nearest);
        assert_eq!(prefs.max_detail, MapDetail::Block);
    }

    #[test]
    fn new_display_defaults_to_fast_terrain_sampling() {
        let prefs = DisplayPrefs::default();
        assert_eq!(prefs.terrain, TerrainDetailOptions::fast());
        assert_eq!(prefs.terrain_preset(), TerrainPreset::Fast);
    }

    #[test]
    fn explicit_terrain_options_persist_and_derive_their_preset() {
        let mut custom = TerrainDetailOptions::full();
        custom.springs = false;
        for options in [
            TerrainDetailOptions::fast(),
            TerrainDetailOptions::balanced(),
            TerrainDetailOptions::full(),
            custom,
        ] {
            let prefs = DisplayPrefs {
                terrain: options,
                ..DisplayPrefs::default()
            };
            let restored: DisplayPrefs =
                serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
            assert_eq!(restored.terrain, options);
            assert_eq!(restored.terrain_preset(), options.preset());
        }
    }

    #[test]
    fn terrain_presets_preserve_hidden_feature_filters() {
        let mut prefs = DisplayPrefs {
            hidden_ore_ids: vec!["tfc:ore/native_copper".into()],
            hidden_structure_ids: vec!["tfg:village/ruined".into()],
            hidden_volcano_ids: vec!["terralens:volcano/cinder".into()],
            ..DisplayPrefs::default()
        };
        let hidden_ores = prefs.hidden_ore_ids.clone();
        let hidden_structures = prefs.hidden_structure_ids.clone();
        let hidden_volcanoes = prefs.hidden_volcano_ids.clone();

        for preset in [
            TerrainPreset::Balanced,
            TerrainPreset::Full,
            TerrainPreset::Fast,
        ] {
            prefs.apply_terrain_preset(preset);
            assert_eq!(prefs.hidden_ore_ids, hidden_ores);
            assert_eq!(prefs.hidden_structure_ids, hidden_structures);
            assert_eq!(prefs.hidden_volcano_ids, hidden_volcanoes);
        }
    }

    #[test]
    fn terrain_option_changes_invalidate_the_raster() {
        let original = DisplayPrefs::default();
        let mut changed = original.clone();
        changed.terrain.volcanoes = true;
        assert!(!original.same_raster(&changed));
    }

    #[test]
    fn fast_to_full_preset_updates_visual_layers_and_detail() {
        let mut prefs = DisplayPrefs::default();
        assert!(!prefs.grass_colors);
        assert!(!prefs.trees);
        assert!(!prefs.height_shading);

        prefs.apply_terrain_preset(TerrainPreset::Full);
        assert_eq!(prefs.terrain, TerrainDetailOptions::full());
        assert_eq!(prefs.terrain_preset(), TerrainPreset::Full);
        assert_eq!(
            prefs.layer_stack.as_deref(),
            Some(
                &[LayerBlend {
                    layer: Layer::Biomes,
                    opacity: 1.0,
                }][..]
            )
        );
        assert!(prefs.grass_colors);
        assert!(prefs.trees);
        assert!(prefs.height_shading);
        assert!(prefs.contours.enabled);

        prefs.apply_terrain_preset(TerrainPreset::Fast);
        assert_eq!(prefs.terrain, TerrainDetailOptions::fast());
        assert!(!prefs.grass_colors);
        assert!(!prefs.trees);
        assert!(!prefs.height_shading);
        assert!(!prefs.contours.enabled);
    }

    #[test]
    fn layer_stack_toggles_preserve_layers_and_use_composable_defaults() {
        let mut prefs = DisplayPrefs::default();
        prefs.set_layer_enabled(Layer::Temperature, true, Layer::Biomes);
        assert_eq!(
            prefs.layer_stack.as_ref().unwrap(),
            &[
                LayerBlend {
                    layer: Layer::Biomes,
                    opacity: 1.0
                },
                LayerBlend {
                    layer: Layer::Temperature,
                    opacity: 0.55
                }
            ]
        );
        prefs.set_layer_enabled(Layer::Rainfall, true, Layer::Biomes);
        assert_eq!(
            prefs
                .active_layers(Layer::Biomes, Dimension::Overworld)
                .len(),
            3
        );
        prefs.set_layer_enabled(Layer::Temperature, false, Layer::Biomes);
        assert_eq!(
            prefs
                .active_layers(Layer::Biomes, Dimension::Overworld)
                .iter()
                .map(|entry| entry.layer)
                .collect::<Vec<_>>(),
            vec![Layer::Biomes, Layer::Rainfall]
        );

        let mut grass_base = DisplayPrefs::default();
        grass_base.grass_colors = true;
        grass_base.set_layer_enabled(Layer::Temperature, true, Layer::Biomes);
        assert_eq!(
            grass_base
                .active_layers(Layer::Biomes, Dimension::Overworld)
                .iter()
                .find(|blend| blend.layer == Layer::Temperature)
                .unwrap()
                .opacity,
            0.55
        );
    }

    #[test]
    fn layer_stack_roundtrips_and_filters_dimension_layers() {
        let mut prefs = DisplayPrefs::default();
        prefs.set_layer_enabled(Layer::Geology, true, Layer::Biomes);
        prefs.set_layer_opacity(Layer::Geology, 1.4, Layer::Biomes);
        prefs.grass_colors = true;
        prefs.trees = true;
        let decoded: DisplayPrefs =
            serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(decoded, prefs);
        assert_eq!(
            decoded.active_layers(Layer::Biomes, Dimension::Moon),
            vec![LayerBlend {
                layer: Layer::Biomes,
                opacity: 1.0
            }]
        );
        assert_eq!(
            decoded.active_layers(Layer::Biomes, Dimension::Overworld)[1].opacity,
            1.0
        );
        assert!(!prefs.same_raster(&DisplayPrefs::default()));
    }

    #[test]
    fn layer_order_moves_visible_rows_without_disturbing_other_dimensions() {
        let mut prefs = DisplayPrefs {
            layer_stack: Some(vec![
                LayerBlend {
                    layer: Layer::Biomes,
                    opacity: 1.,
                },
                LayerBlend {
                    layer: Layer::Temperature,
                    opacity: 0.3,
                },
                LayerBlend {
                    layer: Layer::Height,
                    opacity: 0.7,
                },
            ]),
            ..Default::default()
        };
        prefs.move_layer(Layer::Height, -1, Layer::Biomes, Dimension::Moon);
        let stack = prefs.layer_stack.as_ref().unwrap();
        assert_eq!(stack[0].layer, Layer::Height);
        assert_eq!(stack[0].opacity, 0.7);
        assert_eq!(stack[1].layer, Layer::Temperature);
        assert_eq!(stack[1].opacity, 0.3);
        assert_eq!(stack[2].layer, Layer::Biomes);
        let before = prefs.clone();
        prefs.move_layer(Layer::Temperature, 1, Layer::Biomes, Dimension::Moon);
        assert_eq!(prefs, before, "unsupported rows cannot be reordered here");
        prefs.move_layer(Layer::Height, i32::MAX, Layer::Biomes, Dimension::Moon);
        assert_eq!(prefs.layer_stack.as_ref().unwrap()[2].layer, Layer::Height);
        prefs.move_layer(Layer::Biomes, i32::MIN, Layer::Biomes, Dimension::Moon);
        assert_eq!(prefs.layer_stack.as_ref().unwrap()[0].layer, Layer::Biomes);
    }

    #[test]
    fn presets_add_their_focus_layer_without_replacing_the_composition() {
        let mut prefs = DisplayPrefs::default();
        prefs.set_layer_enabled(Layer::Temperature, true, Layer::Biomes);
        let preset_layer = prefs.apply_preset(Preset::Forestry);
        prefs.set_layer_enabled(preset_layer, true, Layer::Biomes);
        assert_eq!(preset_layer, Layer::Forest);
        assert_eq!(
            prefs
                .active_layers(Layer::Biomes, Dimension::Overworld)
                .iter()
                .map(|entry| entry.layer)
                .collect::<Vec<_>>(),
            vec![Layer::Biomes, Layer::Temperature, Layer::Forest]
        );
        assert_eq!(prefs.preset, Preset::Forestry);
    }

    #[test]
    fn sampling_preferences_persist_without_changing_raster_contents() {
        let base = DisplayPrefs::default();
        for detail in MapDetail::ALL {
            for filter in LoadingFilter::ALL {
                let changed = DisplayPrefs {
                    max_detail: detail,
                    loading_filter: filter,
                    ..base.clone()
                };
                assert!(base.same_raster(&changed));
                let restored: DisplayPrefs =
                    serde_json::from_str(&serde_json::to_string(&changed).unwrap()).unwrap();
                assert_eq!(changed, restored);
            }
        }
    }

    #[test]
    fn contour_preferences_persist_and_invalidate_visible_rasters() {
        let base = DisplayPrefs::default();
        let mut changed = base.clone();
        changed.contours.interval = 20;
        changed.contours.color = [90, 70, 50];
        assert!(
            base.same_raster(&changed),
            "hidden lines need no new raster"
        );
        changed.contours.enabled = true;
        assert!(!base.same_raster(&changed));
        let mut other = changed.clone();
        other.contours.interval = 5;
        assert!(!changed.same_raster(&other));
        let restored: DisplayPrefs =
            serde_json::from_str(&serde_json::to_string(&changed).unwrap()).unwrap();
        assert_eq!(changed, restored);
    }

    #[test]
    fn feature_filters_apply_to_every_occurrence_of_a_type() {
        let mut marker = Marker {
            id: "normal_native_copper@-20,13".into(),
            label: "Native copper".into(),
            namespace: "tfg".into(),
            x: 0.,
            z: 0.,
            y: None,
            kind: MarkerKind::Ore,
            confirmed: false,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [0; 3],
            bounds: None,
        };
        let prefs = DisplayPrefs {
            hidden_ore_ids: vec!["normal_native_copper".into()],
            ..Default::default()
        };
        assert!(!prefs.marker_visible(&marker, 32.));
        marker.id = "normal_native_copper@41,-92".into();
        assert!(!prefs.marker_visible(&marker, 32.));
        marker.id = "normal_native_gold@41,-92".into();
        assert!(prefs.marker_visible(&marker, 0.5));
    }
    #[test]
    fn selected_biome_highlight_never_marks_ocean() {
        let prefs = DisplayPrefs {
            highlighted_biomes: vec![biome::ids::id::OCEAN, biome::ids::id::PLAINS],
            ..DisplayPrefs::default()
        };
        assert!(!prefs.matches_land_sample(biome::ids::id::OCEAN, 4., 200., &[]));
        assert!(prefs.matches_land_sample(biome::ids::id::PLAINS, 4., 200., &[]));
        assert!(!prefs.matches_land_sample(biome::ids::id::HILLS, 4., 200., &[]));
    }

    #[test]
    fn active_climate_and_rock_conditions_are_combined() {
        let prefs = DisplayPrefs {
            temperature_enabled: true,
            temperature_min: 2.,
            temperature_max: 8.,
            rainfall_enabled: true,
            rainfall_min: 100.,
            rainfall_max: 300.,
            highlighted_rocks: vec!["granite".into()],
            ..DisplayPrefs::default()
        };
        let rocks = vec!["granite".to_owned(), "limestone".to_owned()];
        assert!(prefs.matches_land_sample(biome::ids::id::PLAINS, 4., 200., &rocks));
        assert!(!prefs.matches_land_sample(biome::ids::id::PLAINS, 9., 200., &rocks));
        assert!(!prefs.matches_land_sample(biome::ids::id::PLAINS, 4., 301., &rocks));
        assert!(!prefs.matches_land_sample(
            biome::ids::id::PLAINS,
            4.,
            200.,
            &["limestone".to_owned()]
        ));
    }

    fn structure(id: &str) -> Marker {
        Marker {
            id: format!("{id}@4,-7"),
            label: id.into(),
            namespace: "tfg".into(),
            x: 72.,
            z: -104.,
            y: Some(93.),
            kind: MarkerKind::Structure,
            confirmed: false,
            min_y: None,
            max_y: None,
            bounds: None,
            detail: String::new(),
            color: [230, 173, 85],
        }
    }

    #[test]
    fn volcano_visibility_is_separate_from_structures_and_strips_instance_coordinates() {
        let mut prefs = DisplayPrefs::default();
        let mut marker = structure("terralens:volcano/active_shield");
        marker.kind = MarkerKind::Volcano;
        prefs.set_feature_visible(&MarkerKind::Volcano, &marker.id, false);
        assert!(!prefs.marker_visible(&marker, 0.5));
        assert!(
            !prefs
                .hidden_structure_ids
                .iter()
                .any(|id| id.starts_with("terralens:volcano/"))
        );
        prefs.set_feature_visible(&MarkerKind::Volcano, &marker.id, true);
        assert!(prefs.marker_visible(&marker, 0.5));
    }

    #[test]
    fn quieter_structure_defaults_can_be_overridden_per_type_and_persisted() {
        let mut prefs = DisplayPrefs::default();
        for id in [
            "minecraft:nether_fossil",
            "tfg:ocean/ocean_moai_0",
            "tfg:illagers/forest_roaming",
            "tfg:aqueduct/aqueduct",
            "tfg:moon/cheese_ores",
            "tfg:moon/moon_rabbit_houses",
        ] {
            for confirmed in [false, true] {
                let mut marker = structure(id);
                marker.confirmed = confirmed;
                assert!(!prefs.marker_visible(&marker, 0.5), "{id}");
            }
        }
        for dimension in Dimension::ALL {
            for (id, _) in crate::structure_support::catalog(dimension) {
                match crate::structure_loot::ruin_group(id) {
                    Some(crate::structure_loot::RuinGroup::Lootless) => {
                        assert!(!prefs.marker_visible(&structure(id), 0.5), "{id}")
                    }
                    Some(crate::structure_loot::RuinGroup::Building) => {
                        assert!(prefs.marker_visible(&structure(id), 0.5), "{id}")
                    }
                    None => {}
                }
            }
        }
        for id in [
            "tfc_ruins:ruin_small",
            "tfc_ruins:ruin_beach",
            "tfc_ruins:ruin_rich",
            "minecraft:bastion_remnant",
            "tfg:beneath/tower",
            "tfg:mineshaft/mineshaft",
            "tfg:illagers/norse_village_living",
            "tfg:illagers/norse_village_ruined",
            "tfg:moon/moonbase",
        ] {
            assert!(prefs.marker_visible(&structure(id), 0.5), "{id}");
        }
        let moai = structure("tfg:ocean/ocean_moai_0");
        prefs.set_feature_visible(&MarkerKind::Structure, &moai.id, true);
        let restored: DisplayPrefs =
            serde_json::from_value(serde_json::to_value(prefs).unwrap()).unwrap();
        assert!(restored.marker_visible(&moai, 0.5));
        assert!(!restored.marker_visible(&structure("tfg:aqueduct/aqueduct"), 0.5));
    }

    #[test]
    fn beneath_structure_pins_span_all_bands_while_deposits_follow_height() {
        let prefs = DisplayPrefs::default();
        let candidate = structure("tfg:beneath/tower");
        let mut saved = candidate.clone();
        saved.confirmed = true;
        saved.bounds = Some(crate::model::StructureBounds {
            min_x: 64,
            max_x: 95,
            min_z: -112,
            max_z: -81,
            min_y: 80,
            max_y: 130,
        });
        let mut ore = candidate.clone();
        ore.kind = MarkerKind::Ore;
        ore.id = "beneath_native_copper@4,-7".into();
        ore.min_y = Some(80.);
        ore.max_y = Some(110.);
        for y in (0..256).step_by(16) {
            for marker in [&candidate, &saved] {
                assert!(
                    prefs.marker_visible_in_view(marker, 0.5, Dimension::Beneath, y),
                    "Y={y}"
                );
            }
            let (min, max) = Dimension::Beneath.band(y);
            assert_eq!(
                prefs.marker_visible_in_view(&ore, 0.5, Dimension::Beneath, y),
                ore.intersects_band(min, max),
                "Y={y}"
            );
        }
        let mut hidden = prefs;
        hidden.set_feature_visible(&MarkerKind::Structure, &candidate.id, false);
        assert!(!hidden.marker_visible_in_view(&candidate, 0.5, Dimension::Beneath, 0));
        assert!(!hidden.marker_visible_in_view(&saved, 0.5, Dimension::Beneath, 93));
    }
}
