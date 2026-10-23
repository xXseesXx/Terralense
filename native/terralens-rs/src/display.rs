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
            hidden_structure_ids: default_hidden_structure_ids().to_vec(),
            feature_bar_ores: false,
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
                crate::structure_icons::ruin_topic(id).is_some()
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
    /// Used once when upgrading an existing installation; later user toggles are retained.
    pub fn apply_structure_visibility_defaults(&mut self) {
        for id in default_hidden_structure_ids() {
            self.set_feature_visible(&MarkerKind::Structure, id, false);
        }
    }

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
    fn quieter_structure_defaults_can_be_overridden_per_type_and_persisted() {
        let mut prefs = DisplayPrefs::default();
        for id in [
            "minecraft:nether_fossil",
            "tfg:ocean/ocean_moai_0",
            "tfg:illagers/forest_roaming",
            "tfc_ruins:ruin_small",
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
                if crate::structure_icons::ruin_topic(id).is_some() {
                    assert!(!prefs.marker_visible(&structure(id), 0.5), "{id}");
                }
            }
        }
        for id in [
            "minecraft:bastion_remnant",
            "tfg:beneath/tower",
            "tfg:mineshaft/mineshaft",
            "tfg:illagers/norse_village_living",
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
