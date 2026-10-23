//! Persistent map emphasis and marker visibility settings.
//!
//! These preferences are deliberately independent of tile generation. The renderer can
//! apply the same land-only predicate to semantic tile samples without changing biome data.
use crate::{
    atlas::Layer,
    biome,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayPrefs {
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
    pub hidden_ore_ids: Vec<String>,
    pub hidden_structure_ids: Vec<String>,
    pub feature_bar_ores: bool,
}

impl Default for DisplayPrefs {
    fn default() -> Self {
        Self {
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
            hidden_ore_ids: Vec::new(),
            hidden_structure_ids: Vec::new(),
            feature_bar_ores: false,
        }
    }
}

impl DisplayPrefs {
    /// Set one stable feature type's visibility. Occurrence coordinates are stripped so one
    /// click applies to every generated or saved instance of that type.
    pub fn set_feature_visible(&mut self, kind: &MarkerKind, id: &str, visible: bool) {
        let id = id.split('@').next().unwrap_or(id);
        let hidden = match kind {
            MarkerKind::Ore => &mut self.hidden_ore_ids,
            MarkerKind::Structure => &mut self.hidden_structure_ids,
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
            _ => true,
        }
    }

    pub fn visible_id_set(
        &self,
        kind: &MarkerKind,
        ids: impl Iterator<Item = String>,
    ) -> HashSet<String> {
        let hidden = match kind {
            MarkerKind::Ore => &self.hidden_ore_ids,
            MarkerKind::Structure => &self.hidden_structure_ids,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
