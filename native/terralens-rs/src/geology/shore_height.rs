//! TFG shoreline blending and tidal height adjustment.
//!
//! Biome metadata is bundled from the pinned TFG Core Modern 0.9.25 source. The exact source
//! stage is `TFGChunkHeightFiller.adjustHeightForShoreContributions` followed by its ocean tide
//! clamp. `weighted_heights[id]` must be `weights[id] * biome_height(id)` for this same column
//! (zero for absent biomes), so all contributions and the max shore biome are selected using the
//! very same sampled 7x7 weights as the base height.

use crate::{
    biome::ids::BIOME_IDS,
    noise::{self, Noise2D, OpenSimplex2D, TfgCellular2D},
    region::Generator,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

const SHORE_DATA: &str = include_str!("../../assets/data/tfg/biome-shores.json");
const SEA_LEVEL: f64 = 63.0;

#[derive(Deserialize)]
struct Data {
    ocean: Vec<String>,
    shore: HashMap<String, (String, f64)>,
}
static DATA: OnceLock<Data> = OnceLock::new();
fn data() -> &'static Data {
    DATA.get_or_init(|| serde_json::from_str(SHORE_DATA).expect("bundled TFG shore metadata"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    None,
    Classic,
    Sandy,
    Embayments,
    UpperTerrace,
    LowerTerrace,
    SetbackCliffs,
    Dunes,
    RockyShores,
    SeaStacks,
}
impl Kind {
    fn parse(s: &str) -> Self {
        match s {
            "classic" => Self::Classic,
            "sandy" => Self::Sandy,
            "embayments" => Self::Embayments,
            "upper_terrace" => Self::UpperTerrace,
            "lower_terrace" => Self::LowerTerrace,
            "setback_cliffs" => Self::SetbackCliffs,
            "dunes" => Self::Dunes,
            "rocky_shores" => Self::RockyShores,
            "sea_stacks" => Self::SeaStacks,
            _ => Self::None,
        }
    }
}

pub(super) struct Samplers {
    tide: OpenSimplex2D,
    tide_detail: OpenSimplex2D,
    classic: OpenSimplex2D,
    setback_bank: Noise2D,
    dunes: Noise2D,
    embayment_shelf: OpenSimplex2D,
    embayment_steepness: Noise2D,
    embayment_roughness: OpenSimplex2D,
    embayment_tidepool: Noise2D,
    rocky_shelf: OpenSimplex2D,
    rocky_steepness: Noise2D,
    rocky_roughness: OpenSimplex2D,
    rocky_tidepool: Noise2D,
    sea_stack_cells: TfgCellular2D,
    sea_stack_distribution: OpenSimplex2D,
    upper_terrace: OpenSimplex2D,
    lower_terrace: OpenSimplex2D,
}
impl Samplers {
    pub(super) fn new(seed: i64) -> Self {
        let mut tide = OpenSimplex2D::new(seed);
        tide.octaves(3)
            .spread(0.005_f32 as f64)
            .scaled(SEA_LEVEL - 6.0, SEA_LEVEL + 6.0);
        let mut tide_detail = OpenSimplex2D::new(seed);
        tide_detail.spread(0.03_f32 as f64);
        let mut classic = OpenSimplex2D::new(seed.wrapping_add(8_719_234_132));
        classic
            .octaves(2)
            .spread(0.003_f32 as f64)
            .scaled(-0.1, 1.1);
        let mut setback_bank_base = OpenSimplex2D::new(seed);
        setback_bank_base.spread(0.03);
        let setback_bank = scaled_noise(
            noise::map(setback_bank_base.as_noise(), f64::abs),
            -1.0,
            8.0,
        );
        let mut dune_warp = OpenSimplex2D::new(seed);
        dune_warp.spread(0.05).scaled(-10.0, 10.0);
        let mut dune_base = OpenSimplex2D::new(seed);
        dune_base.spread(0.03);
        let dune_mapped = scaled_noise(noise::map(dune_base.as_noise(), f64::abs), -2.0, 6.5);
        let dunes = noise::warped(dune_mapped, dune_warp);
        let mut embayment_shelf = OpenSimplex2D::new(seed.wrapping_add(5252));
        embayment_shelf.octaves(3).spread(0.03).scaled(-0.8, 1.0);
        let mut embayment_steep_raw = OpenSimplex2D::new(seed.wrapping_add(224));
        embayment_steep_raw.octaves(2).spread(0.11);
        let embayment_steepness =
            clamped_scaled(embayment_steep_raw.as_noise(), -0.7, 0.7, 0.0, 0.3);
        let mut embayment_roughness = OpenSimplex2D::new(seed);
        embayment_roughness
            .octaves(3)
            .spread(0.09)
            .scaled(-3.0, 3.0);
        let mut embayment_tide_raw = OpenSimplex2D::new(seed.wrapping_add(492));
        embayment_tide_raw.octaves(3).spread(0.09);
        let embayment_tidepool =
            clamped_scaled(embayment_tide_raw.as_noise(), -1.0, 0.0, -5.0, 0.0);
        let mut rocky_shelf = OpenSimplex2D::new(seed.wrapping_add(5252));
        rocky_shelf.octaves(3).spread(0.03).scaled(-0.8, 1.0);
        let mut rocky_steep_raw = OpenSimplex2D::new(seed.wrapping_add(224));
        rocky_steep_raw.octaves(2).spread(0.11);
        let rocky_steepness = clamped_scaled(rocky_steep_raw.as_noise(), -0.7, 0.7, 0.0, 0.3);
        let mut rocky_roughness = OpenSimplex2D::new(seed);
        rocky_roughness.octaves(3).spread(0.09).scaled(-3.0, 3.0);
        let mut rocky_tide_raw = OpenSimplex2D::new(seed.wrapping_add(492));
        rocky_tide_raw.octaves(3).spread(0.09);
        let rocky_tidepool = clamped_scaled(rocky_tide_raw.as_noise(), -1.0, 0.0, -5.0, 0.0);
        let mut sea_stack_cells = TfgCellular2D::new(seed.wrapping_add(323));
        sea_stack_cells.spread(0.05);
        let mut sea_stack_distribution = OpenSimplex2D::new(seed.wrapping_add(5424));
        sea_stack_distribution.octaves(2).spread(0.015);
        let mut upper_terrace = OpenSimplex2D::new(seed);
        upper_terrace
            .octaves(4)
            .spread(0.05)
            .scaled(63.0 + 18.0, 63.0 + 30.0);
        let mut lower_terrace = OpenSimplex2D::new(seed);
        lower_terrace
            .octaves(4)
            .spread(0.05)
            .scaled(63.0 + 7.0, 63.0 + 15.0);
        Self {
            tide,
            tide_detail,
            classic,
            setback_bank,
            dunes,
            embayment_shelf,
            embayment_steepness,
            embayment_roughness,
            embayment_tidepool,
            rocky_shelf,
            rocky_steepness,
            rocky_roughness,
            rocky_tidepool,
            sea_stack_cells,
            sea_stack_distribution,
            upper_terrace,
            lower_terrace,
        }
    }

    /// Returns the shore blended, tide adjusted height, before volcano and river stages.
    pub(super) fn apply(
        &self,
        _region: &Generator,
        x: i32,
        z: i32,
        weights: &[f64; 109],
        weighted_heights: &[f64; 109],
    ) -> f64 {
        self.apply_weights(x, z, weights, weighted_heights)
    }

    fn apply_weights(
        &self,
        x: i32,
        z: i32,
        weights: &[f64; 109],
        weighted_heights: &[f64; 109],
    ) -> f64 {
        let metadata = data();
        let mut height: f64 = weighted_heights.iter().sum();
        let mut ocean_weight = 0.0;
        let mut shore_weight = 0.0;
        let mut shore_height = 0.0;
        let mut normal_height = 0.0;
        let mut max_shore_weight = 0.0;
        let mut max_shore_id = None;
        let mut shore_kind_weights = [0.0_f64; 10];
        for (id, (&weight, &weighted_height)) in weights.iter().zip(weighted_heights).enumerate() {
            if weight <= 0.0 {
                continue;
            }
            let name = BIOME_IDS[id].rsplit('/').next().unwrap_or("");
            // Every biome participates in the shore blend. Biomes without a shore type use
            // `ShoreBlendType.NONE`, whose sampler contributes the original height unchanged.
            let blend_kind = metadata
                .shore
                .get(name)
                .map(|(kind_name, _)| Kind::parse(kind_name))
                .unwrap_or(Kind::None);
            shore_kind_weights[blend_kind as usize] += weight;
            if metadata.shore.contains_key(name) {
                shore_weight += weight;
                shore_height += weighted_height;
                // The original uses strict > while iterating the object2double map; ties retain
                // whichever entry was encountered first. Registry ordinal is the stable proxy.
                if weight > max_shore_weight {
                    max_shore_weight = weight;
                    max_shore_id = Some(id);
                }
            } else if metadata.ocean.iter().any(|n| n == name) {
                ocean_weight += weight;
            } else {
                normal_height += weighted_height;
            }
        }
        let land_weight = 1.0 - ocean_weight - shore_weight;
        if shore_weight > 0.0 {
            // ShoreNoise samplers all see the original weighted height. Preserve enum order,
            // including NONE first, and blend the independent results by type weight.
            let mut blended = shore_kind_weights[Kind::None as usize] * height;
            for kind in [
                Kind::Classic,
                Kind::Sandy,
                Kind::Embayments,
                Kind::UpperTerrace,
                Kind::LowerTerrace,
                Kind::SetbackCliffs,
                Kind::Dunes,
                Kind::RockyShores,
                Kind::SeaStacks,
            ] {
                let w = shore_kind_weights[kind as usize];
                if w == 0.0 {
                    continue;
                }
                let (shore_name, shore_base) = max_shore_id
                    .and_then(|id| {
                        metadata
                            .shore
                            .get(BIOME_IDS[id].rsplit('/').next().unwrap_or(""))
                    })
                    .map(|(n, h)| (n.as_str(), *h))
                    .unwrap_or(("", 0.0));
                let selected_kind = Kind::parse(shore_name);
                // The base height is metadata of the highest-weight shore biome, as in the source.
                let sample = self.sample_kind(
                    kind,
                    x,
                    z,
                    height,
                    ocean_weight,
                    land_weight,
                    shore_weight,
                    max_shore_weight,
                    selected_kind,
                    shore_base,
                    shore_height,
                    normal_height,
                );
                blended += w * sample;
            }
            height = blended;
        }
        if ocean_weight >= 0.25 {
            // Source `shoreTideLevelNoise`: the first term is 3-octave OpenSimplex scaled to
            // [57,69], clamped to [63,67], plus a 0.03-spread OpenSimplex term.
            let tide = self
                .tide
                .sample(x as f64, z as f64)
                .clamp(SEA_LEVEL, SEA_LEVEL + 4.0)
                + self.tide_detail.sample(x as f64, z as f64);
            let edge = tide - 4.0;
            let t = ((land_weight - 0.32) / (0.36 - 0.32)).clamp(0.0, 1.0);
            height = height.min(edge) * (1.0 - t) + height * t;
        }
        height
    }

    #[allow(clippy::too_many_arguments)]
    fn sample_kind(
        &self,
        kind: Kind,
        x: i32,
        z: i32,
        height: f64,
        ocean: f64,
        land: f64,
        shore: f64,
        this: f64,
        _selected: Kind,
        base: f64,
        shore_height: f64,
        normal_height: f64,
    ) -> f64 {
        let (x, z) = (x as f64, z as f64);
        let tide = self.tide_level(x, z);
        match kind {
            Kind::None => height,
            Kind::Sandy => simple_beach(tide, height, land, ocean),
            Kind::Classic => {
                let influence = (self.classic.sample(x, z)
                    + map(height, base, base + 20.0, 0.0, 0.6))
                .clamp(0.0, 1.0);
                let adjusted = 1.0 - (1.0 - influence) * (1.0 - influence);
                let x2 = lerp(adjusted, 0.8, 0.515);
                let y2 = 1.15 - 0.3 * x2;
                let adjusted_shore = if shore < x2 {
                    map(shore, 0.5, x2, 0.5, y2)
                } else {
                    map(shore, x2, 1.0, y2, 1.0)
                };
                let normal = 1.0 - shore;
                let adjusted_normal = 1.0 - adjusted_shore;
                // Java Math.max propagates NaN. Rust f64::max instead returns its numeric
                // operand, which changes pure-shore classic columns (normal is 0, yielding
                // 0/0) from the source's unchanged height to the shore base.
                let weighted_height = (adjusted_shore / shore) * shore_height
                    + (adjusted_normal / normal) * normal_height;
                let adjusted_height = java_max(weighted_height, base);
                if adjusted_height < height {
                    adjusted_height
                } else {
                    height
                }
            }
            Kind::SetbackCliffs => {
                let sand = simple_beach_no_land_blend(tide, height, land, ocean);
                let shore_bank = (self.setback_bank)(x, z) + sand;
                if land >= 0.25 {
                    return height;
                }
                let bank = if ocean > 0.0 {
                    clamped_map(ocean, 0.0, 0.25, shore_bank, tide - 4.0)
                } else {
                    shore_bank
                };
                clamped_map(this + land / 2.0, 0.5, 1.0, sand, bank)
            }
            Kind::Dunes => {
                let sand = simple_beach(tide, height, land, ocean);
                let full = (self.dunes)(x, z) + sand;
                let dune = if ocean > 0.0 {
                    clamped_map(ocean, 0.0, 0.25, full, tide - 4.0)
                } else {
                    full
                };
                clamped_map(this, 0.5, 1.0, sand, dune)
            }
            Kind::Embayments => {
                let sand = simple_beach(tide, height, land, ocean);
                let rough = self.embayment_roughness.sample(x, z) + (self.embayment_tidepool)(x, z);
                let top = SEA_LEVEL + rough + 15.0;
                let mid = SEA_LEVEL + rough + 9.0;
                let shelf = shelf_progress(self.embayment_shelf.sample(x, z), ocean, land);
                let steep = (self.embayment_steepness)(x, z);
                if shelf > 0.7 + steep {
                    top
                } else if shelf > 0.7 {
                    map(shelf, 0.7, 0.7 + steep, mid, top)
                } else if shelf > 0.35 + steep {
                    mid
                } else if shelf > 0.35 {
                    map(shelf, 0.35, 0.35 + steep, sand, mid)
                } else {
                    sand
                }
            }
            Kind::RockyShores => {
                let edge = tide - 4.0;
                let rough = self.rocky_roughness.sample(x, z) + (self.rocky_tidepool)(x, z);
                let top = SEA_LEVEL + rough + 15.0;
                let mid = SEA_LEVEL + rough + 9.0;
                let low = SEA_LEVEL + rough + 3.0;
                let shelf = shelf_progress(self.rocky_shelf.sample(x, z), ocean, land);
                let steep = (self.rocky_steepness)(x, z);
                if shelf > 0.7 + steep {
                    top
                } else if shelf > 0.7 {
                    map(shelf, 0.7, 0.7 + steep, mid, top)
                } else if shelf > 0.35 + steep {
                    mid
                } else if shelf > 0.35 {
                    map(shelf, 0.35, 0.35 + steep, low, mid)
                } else if shelf > 0.1 - steep {
                    low
                } else {
                    edge
                }
            }
            Kind::SeaStacks => {
                let cell = self.sea_stack_cells.cell(x, z);
                let sample = self.sea_stack_distribution.sample(x, z);
                let distribution = (1.0 - sample.abs()).max(0.0);
                let min_density = 0.85;
                let center = self.sea_stack_distribution.sample(cell.x, cell.y);
                let stack_density = (1.0 - center.abs()).max(0.0);
                let f1 = if stack_density < min_density {
                    3.0
                } else {
                    cell.f1
                };
                let output_min = clamped_map(sample, 0.2, 0.6, 3.0, 1.0);
                let _stack_noise =
                    f1 * clamped_map((cell.f2 - cell.f1).abs(), 0.0, 0.25, output_min, 1.0);
                let _ = distribution;
                clamped_map(ocean, 0.0, 0.5, height, shore_height / shore)
            }
            Kind::UpperTerrace => self.upper_terrace.sample(x, z),
            Kind::LowerTerrace => self.lower_terrace.sample(x, z),
        }
    }

    fn tide_level(&self, x: f64, z: f64) -> f64 {
        self.tide.sample(x, z).clamp(SEA_LEVEL, SEA_LEVEL + 4.0) + self.tide_detail.sample(x, z)
    }
}

fn map(value: f64, in_min: f64, in_max: f64, out_min: f64, out_max: f64) -> f64 {
    out_min + (value - in_min) / (in_max - in_min) * (out_max - out_min)
}
fn clamped_map(value: f64, in_min: f64, in_max: f64, out_min: f64, out_max: f64) -> f64 {
    let t = ((value - in_min) / (in_max - in_min)).clamp(0.0, 1.0);
    lerp(t, out_min, out_max)
}
fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}
fn simple_beach(tide: f64, height: f64, land: f64, ocean: f64) -> f64 {
    let self_height = tide - 2.0;
    let ocean_height = tide - 4.0;
    let land_height = tide + 2.0;
    if ocean > 0.1 {
        clamped_map(ocean, 0.1, 0.25, self_height, ocean_height)
    } else {
        let derived = if land < 0.3 {
            map(land, 0.3, 0.0, height.min(land_height), self_height)
        } else {
            clamped_map(land, 0.3, 0.5, height.min(land_height), height)
        };
        clamped_map(ocean, 0.0, 0.1, derived, self_height)
    }
}
fn simple_beach_no_land_blend(tide: f64, _height: f64, _land: f64, ocean: f64) -> f64 {
    clamped_map(ocean, 0.0, 0.5, tide - 1.0, tide - 4.0)
}
fn shelf_progress(base: f64, ocean: f64, land: f64) -> f64 {
    if ocean > 0.1 {
        clamped_map(ocean, 0.1, 0.3, base, base - ocean * 3.0)
    } else {
        let land_influence = clamped_map(land, 0.4, 0.0, base + land * 2.0, base);
        clamped_map(ocean, 0.0, 0.1, base + land_influence, base)
    }
}
fn clamped_scaled(noise: Noise2D, old_min: f64, old_max: f64, min: f64, max: f64) -> Noise2D {
    std::rc::Rc::new(move |x, z| {
        let scale = (max - min) / (old_max - old_min);
        (noise(x, z) * scale + (min - old_min * scale)).clamp(min, max)
    })
}
fn scaled_noise(noise: Noise2D, min: f64, max: f64) -> Noise2D {
    std::rc::Rc::new(move |x, z| (noise(x, z) + 1.0) * 0.5 * (max - min) + min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserializer, de::Error as _};

    const FIXTURE: &str = include_str!("../../tests/fixtures/tfg-centered-volcanic-java.json");

    #[derive(Deserialize)]
    struct Fixture {
        samples: Vec<Row>,
    }
    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    #[serde(default)]
    struct Row {
        family: String,
        profile: String,
        seed: i64,
        x: i32,
        z: i32,
        biome: Option<BiomeMetadata>,
        ocean_weight: f64,
        land_weight: f64,
        shore_weight: f64,
        this_weight: f64,
        height_in: f64,
        shore_height: f64,
        normal_height: f64,
        #[serde(deserialize_with = "number_or_special")]
        column_height: f64,
        tide_level: f64,
        #[serde(deserialize_with = "number_or_special")]
        tide_stage_height: f64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BiomeMetadata {
        shore_base_height: Option<f64>,
    }

    fn number_or_special<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(number) = value.as_f64() {
            Ok(number)
        } else if let Some(text) = value.as_str() {
            match text {
                "NaN" => Ok(f64::NAN),
                "Infinity" => Ok(f64::INFINITY),
                "-Infinity" => Ok(f64::NEG_INFINITY),
                _ => Err(D::Error::custom(format!("invalid floating value {text:?}"))),
            }
        } else {
            Err(D::Error::custom(
                "expected a JSON number or Java special-value string",
            ))
        }
    }

    #[test]
    fn source_java_shore_column_height_and_tide_match() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).unwrap();
        let mut compared = 0;
        for row in fixture.samples.iter().filter(|r| r.family == "shore") {
            // The source never calls a type sampler when its shore type has zero weight. Some
            // direct fixture evaluations of those unreachable inputs intentionally divide 0/0.
            if row.shore_weight == 0.0 {
                continue;
            }
            let kind = if row.profile.starts_with("classic") {
                Kind::Classic
            } else {
                match row.profile.as_str() {
                    "sandyBeach" => Kind::Sandy,
                    "setbackCliffs" => Kind::SetbackCliffs,
                    "dunes" => Kind::Dunes,
                    "embayments" => Kind::Embayments,
                    "rockyShores" => Kind::RockyShores,
                    "seaStacks" => Kind::SeaStacks,
                    "upperTerrace" => Kind::UpperTerrace,
                    "lowerTerrace" => Kind::LowerTerrace,
                    other => panic!("unknown source shore sampler {other}"),
                }
            };
            let sampler = Samplers::new(row.seed);
            let actual = sampler.sample_kind(
                kind,
                row.x,
                row.z,
                row.height_in,
                row.ocean_weight,
                row.land_weight,
                row.shore_weight,
                row.this_weight,
                kind,
                row.biome
                    .as_ref()
                    .and_then(|b| b.shore_base_height)
                    .unwrap_or(0.0),
                row.shore_height,
                row.normal_height,
            );
            assert!(
                (actual - row.column_height).abs() < 1.0e-6,
                "{} seed {} at {},{}: Rust {actual}, Java {}",
                row.profile,
                row.seed,
                row.x,
                row.z,
                row.column_height
            );
            assert!(
                (sampler.tide_level(row.x as f64, row.z as f64) - row.tide_level).abs() < 1.0e-6,
                "tide seed {} at {},{}",
                row.seed,
                row.x,
                row.z
            );
            let mut staged = row.column_height;
            if row.ocean_weight >= 0.25 {
                let edge = row.tide_level - 4.0;
                staged = clamped_map(row.land_weight, 0.32, 0.36, staged.min(edge), staged);
            }
            assert!(
                (staged - row.tide_stage_height).abs() < 1.0e-6,
                "tide stage at {},{}: Rust {staged}, Java {}",
                row.x,
                row.z,
                row.tide_stage_height
            );
            compared += 1;
        }
        assert!(
            compared >= 1800,
            "expected the full pinned shoreline oracle and reachable corner set; got {compared}"
        );
    }

    #[test]
    fn mixed_land_ocean_shore_weights_keep_none_contributions() {
        use crate::biome::ids::id::{OCEAN, PLAINS, SHORE};
        let sampler = Samplers::new(0);
        let (x, z) = (13_579, -24_680);
        let mut weights = [0.0; 109];
        let mut weighted_heights = [0.0; 109];
        weights[OCEAN as usize] = 0.25;
        weights[PLAINS as usize] = 0.50;
        weights[SHORE as usize] = 0.25;
        weighted_heights[OCEAN as usize] = 10.0;
        weighted_heights[PLAINS as usize] = 40.0;
        weighted_heights[SHORE as usize] = 15.0;

        let height = 65.0;
        let tide = sampler.tide_level(x as f64, z as f64);
        let shore_type_height = simple_beach(tide, height, 0.5, 0.25);
        let shore_blended = 0.75 * height + 0.25 * shore_type_height;
        let expected = clamped_map(
            0.5,
            0.32,
            0.36,
            shore_blended.min(tide - 4.0),
            shore_blended,
        );
        let actual = sampler.apply_weights(x, z, &weights, &weighted_heights);
        assert!(
            (actual - expected).abs() < 1.0e-9,
            "actual {actual}, expected {expected}"
        );
    }

    #[test]
    fn classic_pure_shore_preserves_java_nan_comparison_behavior() {
        let sampler = Samplers::new(0);
        let height = 84.0;
        let actual = sampler.sample_kind(
            Kind::Classic,
            1234,
            -5678,
            height,
            0.0,
            0.0,
            1.0,
            1.0,
            Kind::Classic,
            -16.0,
            84.0,
            0.0,
        );
        assert_eq!(actual, height);
    }

    #[test]
    fn bundled_shore_base_heights_match_the_biome_extension_getter() {
        let shores = &data().shore;
        // BiomeBuilderMixin adds TFC's sea level when the registered offset is set; the
        // extension getter returns these absolute values (not the registered offsets).
        assert_eq!(shores["shore"].1, 59.0); // SEA_LEVEL_Y - 4
        assert_eq!(shores["sea_stacks"].1, 57.0); // SEA_LEVEL_Y - 6
        assert_eq!(shores["ice_sheet_shore"].1, 51.0); // SEA_LEVEL_Y - 12
        assert_eq!(shores["ice_sheet_oceanic_mountains_edge"].1, 47.0); // SEA_LEVEL_Y - 16
        assert_eq!(shores["meltwater_lake"].1, 47.0);
        assert_eq!(shores["old_shield_volcano_shore"].1, SEA_LEVEL); // tuffShoreType keeps builder default
    }

    #[test]
    fn source_skips_shore_families_when_their_weight_is_zero() {
        use crate::biome::ids::id::{OCEAN, PLAINS};
        let sampler = Samplers::new(12345);
        let (x, z) = (321, -654);

        let mut land_weights = [0.0; 109];
        let mut land_heights = [0.0; 109];
        land_weights[PLAINS as usize] = 1.0;
        land_heights[PLAINS as usize] = 80.0;
        assert_eq!(
            sampler.apply_weights(x, z, &land_weights, &land_heights),
            80.0
        );

        let mut ocean_weights = [0.0; 109];
        let mut ocean_heights = [0.0; 109];
        ocean_weights[OCEAN as usize] = 1.0;
        ocean_heights[OCEAN as usize] = 50.0;
        let edge = sampler.tide_level(x as f64, z as f64) - 4.0;
        assert_eq!(
            sampler.apply_weights(x, z, &ocean_weights, &ocean_heights),
            50.0_f64.min(edge)
        );
    }
}
