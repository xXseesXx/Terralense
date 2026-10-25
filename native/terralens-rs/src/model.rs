//! Shared domain types. Coordinates are Minecraft block coordinates; distances are horizontal.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub temperature_scale: f64,
    pub temperature_constant: f64,
    pub rainfall_scale: f64,
    pub rainfall_constant: f64,
    pub continentalness: f64,
    pub grass_density: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            temperature_scale: 20000.,
            temperature_constant: 0.,
            rainfall_scale: 20000.,
            rainfall_constant: 0.,
            continentalness: 0.5,
            grass_density: 1.,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub z: f64,
}
impl Point {
    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.z - other.z)
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Bounds {
    pub min_x: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_z: f64,
}
impl Bounds {
    pub fn contains(&self, x: f64, z: f64) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MarkerKind {
    Structure,
    Volcano,
    Spawn,
    Player,
    Ore,
    Biome,
    Rock,
    Forest,
    Tree,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Marker {
    pub id: String,
    pub label: String,
    pub namespace: String,
    pub x: f64,
    pub z: f64,
    pub y: Option<f64>,
    pub kind: MarkerKind,
    pub confirmed: bool,
    pub min_y: Option<f64>,
    pub max_y: Option<f64>,
    pub detail: String,
    pub color: [u8; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<StructureBounds>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldLocation {
    pub dimension: String,
    pub marker: Marker,
}

/// Inclusive saved structure bounds, as recorded by Minecraft structure starts/pieces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureBounds {
    pub min_x: i32,
    pub min_y: i32,
    pub min_z: i32,
    pub max_x: i32,
    pub max_y: i32,
    pub max_z: i32,
}
impl StructureBounds {
    /// Center of the inclusive block footprint, represented in block-coordinate space.
    pub fn center(self) -> Point {
        Point {
            x: (self.min_x as f64 + self.max_x as f64 + 1.0) * 0.5,
            z: (self.min_z as f64 + self.max_z as f64 + 1.0) * 0.5,
        }
    }

    pub fn intersects_band(self, min_y: i32, max_y: i32) -> bool {
        self.min_y <= max_y && self.max_y >= min_y
    }
    pub fn contains_column(self, x: i32, z: i32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }
}
impl Marker {
    pub fn point(&self) -> Point {
        Point {
            x: self.x,
            z: self.z,
        }
    }
    /// The map/search destination. Structure markers use their footprint center while
    /// `point()` retains the original saved start position for identity and provenance.
    pub fn map_point(&self) -> Point {
        if self.kind == MarkerKind::Structure
            && let Some(bounds) = self.bounds
        {
            return bounds.center();
        }
        self.point()
    }
    pub fn key(&self) -> String {
        format!("{}@{},{}", self.id, self.x as i64, self.z as i64)
    }
    /// Inclusive vertical extent, shared by the viewer and composed/CSV exports.
    pub fn intersects_band(&self, min_y: i32, max_y: i32) -> bool {
        if let Some(bounds) = self.bounds {
            bounds.intersects_band(min_y, max_y)
        } else if let (Some(low), Some(high)) = (self.min_y, self.max_y) {
            low <= max_y as f64 && high >= min_y as f64
        } else {
            self.y
                .is_none_or(|y| y >= min_y as f64 && y <= max_y as f64)
        }
    }
}

#[cfg(test)]
mod structure_bounds_tests {
    use super::*;

    #[test]
    fn inclusive_footprint_center_handles_negative_odd_even_and_extreme_coordinates() {
        assert_eq!(
            StructureBounds {
                min_x: 0,
                max_x: 0,
                min_y: 0,
                max_y: 0,
                min_z: 0,
                max_z: 0
            }
            .center(),
            Point { x: 0.5, z: 0.5 }
        );
        assert_eq!(
            StructureBounds {
                min_x: -4,
                max_x: -2,
                min_y: 0,
                max_y: 0,
                min_z: -8,
                max_z: -5
            }
            .center(),
            Point { x: -2.5, z: -6.0 }
        );
        assert_eq!(
            StructureBounds {
                min_x: i32::MIN,
                max_x: i32::MAX,
                min_y: 0,
                max_y: 0,
                min_z: i32::MIN,
                max_z: i32::MAX
            }
            .center(),
            Point { x: 0.0, z: 0.0 }
        );
    }

    #[test]
    fn map_point_is_structure_footprint_center_without_changing_start_identity() {
        let marker = Marker {
            id: "mod:structure".into(),
            label: "structure".into(),
            namespace: "mod".into(),
            x: 100.0,
            z: -50.0,
            y: None,
            kind: MarkerKind::Structure,
            confirmed: true,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [0; 3],
            bounds: Some(StructureBounds {
                min_x: 0,
                max_x: 2,
                min_y: 4,
                max_y: 8,
                min_z: -4,
                max_z: -2,
            }),
        };
        assert_eq!(marker.point(), Point { x: 100.0, z: -50.0 });
        assert_eq!(marker.key(), "mod:structure@100,-50");
        assert_eq!(marker.map_point(), Point { x: 1.5, z: -2.5 });
        let mut non_structure = marker.clone();
        non_structure.kind = MarkerKind::Player;
        assert_eq!(non_structure.map_point(), non_structure.point());
        non_structure.bounds = None;
        assert_eq!(non_structure.map_point(), non_structure.point());
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Climate {
    pub temperature: f64,
    pub rainfall: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TreeSpecies {
    pub name: String,
    pub count: f64,
    pub share: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TreeEstimate {
    pub total: f64,
    pub per_hectare: f64,
    pub species: Vec<TreeSpecies>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Probe {
    pub point: Point,
    pub biome: String,
    pub ocean: bool,
    /// Visible surface fluid, independent of the biome's ocean classification.
    #[serde(default)]
    pub surface_water: bool,
    /// Modeled top block of exposed volcanic lava; distinct from terrain-noise height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_lava: Option<i32>,
    /// Top of an above-sea-level caldera or a saved water column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_water_y: Option<i32>,
    /// Exposed solid magma, never classified as a fluid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_magma: Option<i32>,
    /// Terrain substrate, independent of overlying foliage and decorative plants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_block: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_y: Option<i32>,
    #[serde(default)]
    pub ground_saved: bool,
    pub climate: Climate,
    pub height: Option<f64>,
    pub forest: u8,
    pub rocks: [String; 3],
    pub hotspot: f64,
    pub hotspot_age: f64,
}
impl Probe {
    pub fn is_water(&self) -> bool {
        self.surface_lava.is_none()
            && self.surface_magma.is_none()
            && (self.ocean || self.surface_water)
    }
    pub fn is_fluid(&self) -> bool {
        self.surface_lava.is_some() || self.is_water()
    }
    pub fn is_bare_surface(&self) -> bool {
        self.is_fluid() || self.surface_magma.is_some()
    }
    pub fn volcanic_surface(&self) -> Option<crate::volcanic_lava::Surface> {
        use crate::volcanic_lava::Surface;
        self.surface_lava
            .map(Surface::Lava)
            .or_else(|| self.surface_magma.map(Surface::Magma))
            .or_else(|| self.surface_water_y.map(Surface::Water))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldInfo {
    pub id: String,
    pub name: String,
    pub path: std::path::PathBuf,
    pub seed: i64,
    pub center: Point,
    pub settings: Settings,
    pub compatible: bool,
    pub warning: Option<String>,
    #[serde(default)]
    pub dimensions: Vec<crate::dimension::DimensionInfo>,
    #[serde(default)]
    pub map_locations: Vec<WorldLocation>,
}
