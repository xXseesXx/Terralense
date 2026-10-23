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
    pub climate: Climate,
    pub height: Option<f64>,
    pub forest: u8,
    pub rocks: [String; 3],
    pub hotspot: f64,
    pub hotspot_age: f64,
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
