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
    #[serde(rename = "ad_astra:moon")]
    Moon,
    #[serde(rename = "ad_astra:mars")]
    Mars,
}

impl Dimension {
    pub const ALL: [Self; 4] = [Self::Overworld, Self::Beneath, Self::Moon, Self::Mars];
    pub fn id(self) -> &'static str {
        match self {
            Self::Overworld => "minecraft:overworld",
            Self::Beneath => "minecraft:the_nether",
            Self::Moon => "ad_astra:moon",
            Self::Mars => "ad_astra:mars",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Overworld => "Overworld",
            Self::Beneath => "Beneath",
            Self::Moon => "Moon",
            Self::Mars => "Mars",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "minecraft:overworld" | "overworld" => Some(Self::Overworld),
            "minecraft:the_nether" | "nether" | "beneath" => Some(Self::Beneath),
            "ad_astra:moon" | "moon" => Some(Self::Moon),
            "ad_astra:mars" | "mars" => Some(Self::Mars),
            _ => None,
        }
    }
    pub fn storage(self, world: &Path) -> PathBuf {
        match self {
            Self::Overworld => world.to_path_buf(),
            Self::Beneath => world.join("DIM-1"),
            Self::Moon => world.join("dimensions/ad_astra/moon"),
            Self::Mars => world.join("dimensions/ad_astra/mars"),
        }
    }
    pub fn default_y(self) -> i32 {
        match self {
            Self::Overworld => 64,
            Self::Beneath => 96,
            Self::Moon => 64,
            Self::Mars => 88,
        }
    }
    pub fn clamp_y(self, y: i32) -> i32 {
        match self {
            Self::Overworld => y.clamp(-64, 319),
            Self::Beneath => y.clamp(0, 255),
            Self::Moon => y.clamp(0, 383),
            // Mars' physical dimension type spans -64..320 in the saved
            // world. Its noise settings begin at -32; that is a generator
            // range, not the selectable/saved dimension range.
            Self::Mars => y.clamp(-64, 319),
        }
    }
    pub fn min_y(self) -> i32 {
        match self {
            Self::Overworld => -64,
            Self::Beneath | Self::Moon => 0,
            Self::Mars => -64,
        }
    }
    pub fn height(self) -> i32 {
        match self {
            Self::Overworld | Self::Moon | Self::Mars => 384,
            Self::Beneath => 256,
        }
    }
    pub fn is_planetary(self) -> bool {
        matches!(self, Self::Moon | Self::Mars)
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

    #[test]
    fn moon_and_mars_have_stable_ids_storage_and_installed_vertical_ranges() {
        let root = Path::new("/save");
        assert_eq!(Dimension::ALL.len(), 4);
        assert_eq!(Dimension::from_id("moon"), Some(Dimension::Moon));
        assert_eq!(Dimension::from_id("mars"), Some(Dimension::Mars));
        assert_eq!(Dimension::Moon.id(), "ad_astra:moon");
        assert_eq!(Dimension::Mars.id(), "ad_astra:mars");
        assert_eq!(
            Dimension::Moon.storage(root),
            root.join("dimensions/ad_astra/moon")
        );
        assert_eq!(
            Dimension::Mars.storage(root),
            root.join("dimensions/ad_astra/mars")
        );
        assert_eq!(
            (
                Dimension::Moon.min_y(),
                Dimension::Moon.height(),
                Dimension::Moon.default_y()
            ),
            (0, 384, 64)
        );
        assert_eq!(
            (
                Dimension::Mars.min_y(),
                Dimension::Mars.height(),
                Dimension::Mars.default_y()
            ),
            (-64, 384, 88)
        );
        assert!(Dimension::Moon.is_planetary() && Dimension::Mars.is_planetary());
        assert_eq!(Dimension::Moon.clamp_y(-10), 0);
        assert_eq!(Dimension::Moon.clamp_y(900), 383);
        assert_eq!(Dimension::Mars.clamp_y(-80), -64);
        assert_eq!(Dimension::Mars.clamp_y(900), 319);
    }
}
