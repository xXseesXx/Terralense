//! TerraLens native core, derived from OutCrop's validated TFG port. EUPL-1.2.
//!
//! Native conversion and viewer additions: 22–23 October 2026.
//! See the repository's NOTICE.md for the upstream revisions and asset attribution.
#[cfg(feature = "gui")]
pub mod app;
pub mod atlas;
pub mod beneath;
pub mod biome;
pub mod caves;
pub mod contours;
pub mod features;
pub mod geology;
pub mod ground;
pub mod ground_palette;
mod ground_view;
pub mod hillshade;
pub(crate) mod hot_springs;
pub mod map_colors;
pub mod model;
pub mod noise;
pub mod ore_icons;
pub mod prism;
pub mod random;
pub mod region;
pub mod world;

pub mod rivers;

pub mod search;
pub mod structure_icons;
pub mod structure_loot;
pub mod structure_preview;
pub mod structure_support;
pub mod surface_litter;
pub mod surface_water;

pub mod analysis;
pub mod dimension;
pub mod display;
pub mod exports;
pub mod planetary;
pub mod player_heads;
pub mod vegetation;
pub mod volcanic_lava;
pub mod volcano;
pub mod world_files;
