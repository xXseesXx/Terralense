//! Supported dimension identities and storage-independent map selection.
use crate::model::Point;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dimension {
    #[default]
    #[serde(rename = "minecraft:overworld")]
    Overworld,
    #[serde(rename = "minecraft:the_nether")]
    Beneath,
}

impl Dimension {
    pub const ALL: [Self; 2] = [Self::Overworld, Self::Beneath];
    pub fn id(self) -> &'static str {
        match self {
            Self::Overworld => "minecraft:overworld",
            Self::Beneath => "minecraft:the_nether",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Overworld => "Overworld",
            Self::Beneath => "Beneath",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "minecraft:overworld" | "overworld" => Some(Self::Overworld),
            "minecraft:the_nether" | "nether" | "beneath" => Some(Self::Beneath),
            _ => None,
        }
    }
    pub fn storage(self, world: &Path) -> PathBuf {
        match self {
            Self::Overworld => world.to_path_buf(),
            Self::Beneath => world.join("DIM-1"),
        }
    }
    pub fn default_y(self) -> i32 {
        match self {
            Self::Overworld => 64,
            Self::Beneath => 96,
        }
    }
    pub fn clamp_y(self, y: i32) -> i32 {
        match self {
            Self::Overworld => y.clamp(-64, 319),
            Self::Beneath => y.clamp(0, 255),
        }
    }
    /// Inclusive cutaway layer, aligned to a Minecraft section.
    pub fn band(self, y: i32) -> (i32, i32) {
        let base = self.clamp_y(y).div_euclid(16) * 16;
        (base, base + 15)
    }
}

/// Discovered dimension metadata, including unsupported dimensions for future ports.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DimensionInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub generated: bool,
    pub center: Point,
    pub min_y: i32,
    pub height: i32,
    pub supported: bool,
    pub warning: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn beneath_layers_cover_the_dimension_without_gaps_or_overlap() {
        for y in 0..256 {
            let (min, max) = Dimension::Beneath.band(y);
            assert_eq!(min % 16, 0);
            assert_eq!(max - min, 15);
            assert!(min <= y && y <= max);
        }
        assert_eq!(Dimension::Beneath.band(-10), (0, 15));
        assert_eq!(Dimension::Beneath.band(1000), (240, 255));
    }
}
