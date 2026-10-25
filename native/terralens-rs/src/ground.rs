//! Procedural TFG surface-material sampler.
//!
//! This models the exposed substrate selected by TFG's surface builders. Saved chunk
//! substrate data, when available, is authoritative and should replace this estimate.
use crate::{biome::ids, noise::OpenSimplex2D, world::World};
use std::sync::{Arc, OnceLock};

mod builders;
mod centered;
mod decorations;
mod depth;
mod lenses;
mod sandstone;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundKind {
    Grass,
    Soil,
    Sand,
    Gravel,
    Rock,
    Snow,
    Ice,
    Mud,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundSample {
    pub y: i32,
    /// Namespaced Minecraft block identifier, e.g. `tfc:grass/loam`.
    pub block: Arc<str>,
    pub kind: GroundKind,
}

/// Estimate the block at the terrain surface using TFG's biome surface-state rules.
///
/// The result includes modeled spring surfaces, but not saved structures, trees
/// or thin snow. Callers prefer saved substrate and apply other fluid surfaces.
pub fn sample(world: &World, x: i32, z: i32) -> Option<GroundSample> {
    sample_with_options(world, x, z, &crate::display::TerrainDetailOptions::full())
}

/// Render-only sampler whose expensive stages are selected per request.
pub(crate) fn sample_with_options(
    world: &World,
    x: i32,
    z: i32,
    options: &crate::display::TerrainDetailOptions,
) -> Option<GroundSample> {
    let physical = options.physical_surface;
    if physical
        && options.springs
        && let Some(spring) = crate::hot_springs::sample(world, x, z)
    {
        return Some(GroundSample {
            y: spring.y,
            kind: kind_of(&spring.block),
            block: spring.block,
        });
    }
    if physical
        && options.large_rocks_special_soils
        && let Some(surface) = decorations::sample(world, x, z)
    {
        return Some(surface);
    }
    base_sample_inner(
        world,
        x,
        z,
        physical && options.caves,
        physical && options.exposed_dikes,
    )
}

/// Earlier rock decoration can emerge above the terrain aquifer. Query only
/// the sparse boulder stage, avoiding later soil patches and feature recursion.
pub(crate) fn rock_above_fluid(
    world: &World,
    x: i32,
    z: i32,
    fluid_y: i32,
) -> Option<GroundSample> {
    decorations::pre_forest(world, x, z).filter(|ground| ground.y >= fluid_y)
}

/// Explain the independent seed-only sampler by generation stage.
#[cfg(feature = "diagnostics")]
pub fn diagnostic_column(world: &World, x: i32, z: i32) -> serde_json::Value {
    use serde_json::json;
    let describe = |s: Option<GroundSample>| s.map(|s| json!({"y":s.y,"block":s.block.as_ref()}));
    let raw = world.height(x, z);
    let context = crate::geology::surface_biome_with_height(&world.region, world.seed, x, z, raw);
    let physical = crate::caves::column(world, x, z);
    let climate = world.climate(x, z);
    let y = physical.map_or(raw.unwrap_or(63.) as i32, |c| c.surface_builder_y);
    let biome = context.map_or_else(|| world.biome_id(x, z), |c| c.biome);
    let simple = soil_top(
        world,
        biome,
        x,
        y,
        z,
        climate.rainfall,
        climate.temperature,
        None,
    );
    json!({"x":x,"z":z,"raw_height":raw,
        "source_biome":world.biome(x,z),
        "base_biome":crate::biome::biome_name(world.region.biome_at_quart_no_river(x.div_euclid(4),z.div_euclid(4))),
        "builder_biome":crate::biome::biome_name(biome),
        "original_builder_biome":context.map(|c|crate::biome::biome_name(c.original_biome)),
        "builder_weight":context.map(|c|c.weight),
        "height_before_truncation":crate::geology::column_density_context(&world.region,world.seed,x,z).map(|c|c.height_unrounded),
        "rainfall":climate.rainfall,"temperature":climate.temperature,
        "slope":crate::geology::surface_slope(&world.region,world.seed,x,z),
        "physical":physical.map(|c|json!({"solid_y":c.solid_y,"fluid":c.fluid,"surface_builder_y":c.surface_builder_y})),
        "rock_at_surface":world.rock_at(x,y,z),
        "centered_builder":centered::sample(world,x,y,z,climate.rainfall,climate.temperature),
        "specialist_builder":builders::sample(world,biome,x,y,z,climate.rainfall,climate.temperature),
        "normal_exposure":depth::normal_exposure(world,biome,x,y,z,climate.rainfall,climate.temperature),
        "generic_soil":simple,
        "base":describe(base_sample(world,x,z)),
        "before_forest":describe(pre_forest_sample(world,x,z)),
        "final_ground":describe(sample(world,x,z)),
        "surface_water":world.is_surface_water(x,z),
        "decoration_candidates":decorations::diagnostic_candidates(world,x,z)})
}

/// Surface at forest decoration time: earlier springs and boulders are present,
/// while later dry springs and soil discs cannot alter root eligibility.
pub(crate) fn pre_forest_sample(world: &World, x: i32, z: i32) -> Option<GroundSample> {
    let boulder = decorations::pre_forest(world, x, z);
    if let Some(spring) = crate::hot_springs::sample_before_forest(world, x, z) {
        let (y, block) = if matches!(
            crate::volcanic_lava::Surface::from_block(&spring.block, spring.y),
            Some(crate::volcanic_lava::Surface::Water(_) | crate::volcanic_lava::Surface::Lava(_))
        ) {
            spring
                .floor
                .map(|floor| (floor.y, floor.block))
                .unwrap_or((spring.y.saturating_sub(1), spring.block))
        } else {
            (spring.y, spring.block)
        };
        if boulder.as_ref().is_none_or(|boulder| boulder.y < y) {
            return Some(GroundSample {
                y,
                kind: kind_of(&block),
                block,
            });
        }
    }
    boulder.or_else(|| base_sample(world, x, z))
}

#[cfg(test)]
pub(crate) fn sample_with_detail(
    world: &World,
    x: i32,
    z: i32,
    detail: bool,
) -> Option<GroundSample> {
    if detail && let Some(spring) = crate::hot_springs::sample(world, x, z) {
        return Some(GroundSample {
            y: spring.y,
            kind: kind_of(&spring.block),
            block: spring.block,
        });
    }
    if detail && let Some(surface) = decorations::sample(world, x, z) {
        return Some(surface);
    }
    base_sample_inner(world, x, z, detail, true)
}

/// Surface-builder terrain before placed features. Decoration generators must
/// use this entry point so projecting a feature cannot recursively generate itself.
pub(crate) fn base_sample(world: &World, x: i32, z: i32) -> Option<GroundSample> {
    base_sample_inner(world, x, z, true, true)
}

/// Surface-builder result immediately before the underground ore/dike feature
/// stage. Used only to replay dike RNG provider calls without recursing into the
/// dike overlay currently being evaluated.
pub(super) fn source_sample(world: &World, x: i32, z: i32) -> Option<GroundSample> {
    base_sample_inner(world, x, z, true, false)
}

/// Builder-stage voxel without later decorations or spring recursion.
pub(crate) fn source_block_at(world: &World, x: i32, y: i32, z: i32) -> Option<Arc<str>> {
    let surface = source_sample(world, x, z)?;
    if y > surface.y || crate::caves::state_at(world, x, y, z)? != crate::caves::Cell::Solid {
        return None;
    }
    Some(if y == surface.y {
        surface.block
    } else {
        subsurface_before_forest(world, x, y, z, surface.y).into()
    })
}

/// Whether the source block at an arbitrary Y is one of the rock states the
/// intrusive dike configs can replace. The three configs share the same 60
/// raw/gravel/hardened rock keys, so internal replay needs only this predicate.
pub(super) fn source_is_dike_replaceable_from_sample(
    world: &World,
    x: i32,
    y: i32,
    z: i32,
    surface: &GroundSample,
    stratified: bool,
) -> bool {
    if y > surface.y {
        return false;
    }
    if y == surface.y {
        return is_dike_replaceable_rock(&surface.block);
    }
    if crate::caves::state_at(world, x, y, z) != Some(crate::caves::Cell::Solid) {
        return false;
    }
    // Dike replay examines many thousands of underground positions. Outside
    // badlands, the surface-builder soil profile is at most five blocks deep;
    // below it the state is raw host rock. Avoid rebuilding climate, biome,
    // slope and soil strings for every solid voxel in the pipe bounding box.
    // Badlands keep the full path because their sandstone strata can be deep.
    if !stratified && y < surface.y - 5 {
        return true;
    }
    is_dike_replaceable_rock(&subsurface_before_forest(world, x, y, z, surface.y))
}

pub(super) fn dike_source_stratified(world: &World, x: i32, z: i32) -> bool {
    let source_biome = world
        .region
        .biome_at_quart_no_river(x.div_euclid(4), z.div_euclid(4));
    matches!(
        source_biome,
        crate::biome::ids::id::BADLANDS
            | crate::biome::ids::id::MESAS
            | crate::biome::ids::id::BUTTES
            | crate::biome::ids::id::STAIR_STEP_CANYONS
            | crate::biome::ids::id::HOODOOS
            | crate::biome::ids::id::WHORLED_CANYONS
    )
}

fn is_dike_replaceable_rock(block: &str) -> bool {
    ["tfc:rock/raw/", "tfc:rock/gravel/", "tfc:rock/hardened/"]
        .iter()
        .any(|prefix| block.starts_with(prefix))
}

fn base_sample_inner(
    world: &World,
    x: i32,
    z: i32,
    detailed: bool,
    apply_lenses: bool,
) -> Option<GroundSample> {
    if world.dimension != crate::dimension::Dimension::Overworld {
        return None;
    }
    // Below sea level the fill loop runs up to the sea boundary. Its upper
    // density slide can retain one more solid block than the raw noise height.
    // Above sea level maxFilledY clips that extra block. Local views then resolve
    // the cave density/carver pass; distant views retain the cheap height profile.
    let height = world.height(x, z)?;
    let ordinary = crate::surface_water::ordinary_solid_top(height)?;
    let column = detailed
        .then(|| crate::caves::column(world, x, z))
        .flatten();
    let y = column.map_or(ordinary, |c| c.solid_y);
    let profile_y = column.map_or(ordinary, |c| c.surface_builder_y);
    let biome =
        crate::geology::surface_biome_with_height(&world.region, world.seed, x, z, Some(height))?
            .biome;
    let climate = world.climate(x, z);
    let block: Arc<str> = if y < profile_y {
        subsurface_before_forest(world, x, y, z, profile_y)
    } else {
        surface_block(world, biome, x, y, z, climate.rainfall, climate.temperature)
    }
    .into();
    let block = if apply_lenses {
        lenses::replace_surface_block(world, x, y, z, &block).unwrap_or(block)
    } else {
        block
    };
    Some(GroundSample {
        y,
        kind: kind_of(&block),
        block,
    })
}

/// Soil/rock beneath the pre-carver surface, used by root-state matching.
/// Root candidates use the natural builder stage, never later soil discs.
pub(crate) fn block_before_forest_at(world: &World, x: i32, y: i32, z: i32) -> Option<Arc<str>> {
    let surface = pre_forest_sample(world, x, z)?;
    if y > surface.y {
        return None;
    }
    if y == surface.y {
        return Some(surface.block);
    }
    if crate::caves::state_at(world, x, y, z)? != crate::caves::Cell::Solid {
        return None;
    }
    Some(subsurface_before_forest(world, x, y, z, surface.y).into())
}
fn subsurface_before_forest(world: &World, x: i32, y: i32, z: i32, cap: i32) -> String {
    // Resolve the source's top/mid soil families before falling through to rock.
    // Normal soil layers have altitude/slope depth <=5. Profile-specific deeper
    // sandstone is selected separately, while caves remain non-solid above.
    let climate = world.climate(x, z);
    let raw = world.height(x, z);
    let biome = crate::geology::surface_biome_with_height(&world.region, world.seed, x, z, raw)
        .map_or_else(|| world.biome_id(x, z), |c| c.biome);
    if let Some(block) = sandstone::at_depth(
        world,
        biome,
        x,
        y,
        z,
        cap,
        climate.rainfall,
        climate.temperature,
    ) {
        return block;
    }
    let top = surface_block(
        world,
        biome,
        x,
        cap,
        z,
        climate.rainfall,
        climate.temperature,
    );
    let slope = crate::geology::surface_slope(&world.region, world.seed, x, z).unwrap_or(0.);
    let depth = depth::surface_depth(cap, slope, 5, -1).max(0);
    if cap - y <= depth {
        for (from, to) in [(":clay_grass/", ":clay/"), (":grass/", ":dirt/")] {
            if top.contains(from) {
                return top.replacen(from, to, 1);
            }
        }
        if top.starts_with("tfc:sand/")
            || top.starts_with("tfc:mud/")
            || top.starts_with("tfg:mud/")
        {
            return top;
        }
    }
    builders::raw_rock(world, x, y, z)
}

fn surface_block(
    world: &World,
    biome: u16,
    x: i32,
    y: i32,
    z: i32,
    rainfall: f64,
    temperature: f64,
) -> String {
    if let Some(block) = centered::sample(world, x, y, z, rainfall, temperature) {
        return block;
    }
    if let Some(block) = sandstone::sample(world, biome, x, y, z, rainfall, temperature) {
        return block;
    }
    if let Some(block) = builders::sample(world, biome, x, y, z, rainfall, temperature) {
        return block;
    }
    if let Some(block) = depth::normal_exposure(world, biome, x, y, z, rainfall, temperature) {
        return block;
    }
    soil_top(world, biome, x, y, z, rainfall, temperature, None)
}

fn soil_top(
    world: &World,
    biome: u16,
    x: i32,
    y: i32,
    z: i32,
    rainfall: f64,
    temperature: f64,
    dry_override: Option<&str>,
) -> String {
    let dry = || {
        dry_override
            .map(str::to_owned)
            .unwrap_or_else(|| dry_surface(world, x, y, z, rainfall))
    };
    // The default TFG builder uses TOP_GRASS_TO_SANDY_GRAVEL with its 18-region
    // rain/temperature lookup. TFGSoilSurfaceState clamps both indices and takes min.
    let rain_index = map_index(rainfall, 35., 450., 3., 17.99, 18);
    let temp_index = map_index(
        elevation_temperature(temperature, y),
        -19.,
        -4.,
        0.,
        17.99,
        18,
    );
    let i = rain_index.min(temp_index);
    match i {
        0 => "minecraft:snow_block".into(),
        1 => "minecraft:snow_block".into(),
        2 => {
            if position_hash(x, y, z, 729_375_982) & 127 > 63 {
                "minecraft:snow_block".into()
            } else {
                dry()
            }
        }
        3 => dry(),
        4 => {
            if position_hash(x, y, z, 729_375_982) & 127 > 63 {
                dry()
            } else {
                "tfg:coarse_dirt/sandy_loam".into()
            }
        }
        5 => "tfg:coarse_dirt/sandy_loam".into(),
        6 => if position_hash(x, y, z, 729_375_982) & 127 > 63 {
            "tfg:coarse_dirt/sandy_loam"
        } else {
            "tfc:grass/sandy_loam"
        }
        .to_owned(),
        7 => "tfc:grass/sandy_loam".into(),
        8 => if patch_noise(x, z) > 0. {
            "tfc:grass/sandy_loam"
        } else {
            "tfc:grass/loam"
        }
        .into(),
        9 | 10 => "tfc:grass/loam".into(),
        11 => {
            if patch_noise(x, z) > 0. {
                "tfc:grass/loam".into()
            } else {
                transitional_soil(
                    biome,
                    x,
                    z,
                    temperature,
                    "tfc:grass/silty_loam",
                    "tfg:grass/oxisol",
                )
            }
        }
        12 | 13 => transitional_soil(
            biome,
            x,
            z,
            temperature,
            "tfc:grass/silty_loam",
            "tfg:grass/oxisol",
        ),
        14 => {
            if patch_noise(x, z) > 0. {
                "tfc:grass/silty_loam".into()
            } else {
                transitional_soil(
                    biome,
                    x,
                    z,
                    temperature,
                    "tfc:grass/silt",
                    "tfg:grass/oxisol",
                )
            }
        }
        _ => transitional_soil(
            biome,
            x,
            z,
            temperature,
            "tfc:grass/silt",
            "tfg:grass/oxisol",
        ),
    }
}

fn elevation_temperature(average: f64, y: i32) -> f64 {
    f64::from(average as f32 - ((y - 63) as f32 * 0.16225_f32).clamp(0., 17.822_f32))
}

fn map_index(value: f64, from: f64, to: f64, out_from: f64, out_to: f64, n: usize) -> usize {
    // SurfaceBuilderContext exposes rainfall/temperature as floats and Minecraft's
    // clampedMap operates in float precision. Preserve that for band boundaries.
    let value = value as f32;
    let from = from as f32;
    let to = to as f32;
    let out_from = out_from as f32;
    let out_to = out_to as f32;
    let t = ((value - from) / (to - from)).clamp(0., 1.);
    (out_from + t * (out_to - out_from)).clamp(0., n as f32 - 0.01_f32) as usize
}

fn patch_noise(x: i32, z: i32) -> f64 {
    static NOISE: OnceLock<OpenSimplex2D> = OnceLock::new();
    NOISE
        .get_or_init(|| {
            let mut n = OpenSimplex2D::new(18_273_952_837_592);
            n.octaves(2).spread(f64::from(0.04_f32));
            n
        })
        .sample(x as f64, z as f64)
}

fn position_hash(x: i32, y: i32, z: i32, salt: i64) -> u32 {
    // Exact TFC Helpers.hash(salt, BlockPos): signed Java long overflow, then int cast.
    let hash = (salt as u64)
        ^ (x as i64 as u64).wrapping_mul(501_125_321)
        ^ (y as i64 as u64).wrapping_mul(1_136_930_381)
        ^ (z as i64 as u64);
    hash.wrapping_mul(0x27d4_eb2d) as u32
}

fn transitional_soil(biome: u16, x: i32, z: i32, temp: f64, cold: &str, hot: &str) -> String {
    // Only the climate-transition states force flooding-river silt; arid soil
    // bands keep their own variants. This comparison uses annual temperature,
    // unlike the elevation-adjusted temperature used to select the soil band.
    if biome == ids::id::RIVER {
        "tfc:grass/silt".into()
    } else if (temp as f32) < 16.0 {
        cold.into()
    } else if (temp as f32) > 16.7_f32 {
        hot.into()
    } else if patch_noise(x, z) > 0. {
        hot.into()
    } else {
        cold.into()
    }
}

fn dry_surface(world: &World, x: i32, y: i32, z: i32, rainfall: f64) -> String {
    // TFG delegates this to TFC's nine-region SAND_OR_GRAVEL state: low rainfall
    // selects rock-specific sand; the next band patches sand and gravel; later
    // bands use gravel.
    let region = map_index(rainfall, 20., 500., 0., 8.99, 9);
    if region == 0 || (region == 1 && patch_noise(x, z) > 0.) {
        sand(world, x, y, z)
    } else {
        gravel_rock(world, x, y, z)
    }
}
fn gravel_rock(world: &World, x: i32, y: i32, z: i32) -> String {
    let rock = world.rock_at(x, y, z).unwrap_or_else(|| "granite".into());
    format!("tfc:rock/gravel/{rock}")
}
fn sand(world: &World, x: i32, y: i32, z: i32) -> String {
    let name = world.rock_at(x, y, z).unwrap_or_else(|| "granite".into());
    let color = sand_variant(&name);
    format!("tfc:sand/{color}")
}

fn sand_variant(rock: &str) -> &'static str {
    match rock {
        "granite" | "claystone" | "slate" | "phyllite" => "brown",
        "diorite" | "limestone" | "chalk" | "marble" => "white",
        "gabbro" | "shale" | "dolomite" => "black",
        "conglomerate" | "schist" | "gneiss" => "green",
        "chert" | "quartzite" => "yellow",
        "rhyolite" | "basalt" | "andesite" | "dacite" => "red",
        _ => "brown",
    }
}
fn kind_of(id: &str) -> GroundKind {
    let path = id.split_once(':').map_or(id, |(_, path)| path);
    let (family, variant) = path.split_once('/').unwrap_or((path, ""));
    match family {
        "grass" | "clay_grass" | "grass_block" => GroundKind::Grass,
        "sand" | "red_sand" => GroundKind::Sand,
        "gravel" | "tuff_gravel" => GroundKind::Gravel,
        "rock" if variant.starts_with("gravel/") => GroundKind::Gravel,
        "rock" | "raw_sandstone" | "stone" | "tuff" => GroundKind::Rock,
        "snow" | "snow_block" => GroundKind::Snow,
        "ice" | "sea_ice" | "packed_ice" | "blue_ice" => GroundKind::Ice,
        "mud" => GroundKind::Mud,
        "dirt" | "coarse_dirt" | "rooted_dirt" | "farmland" | "grass_path" | "dirt_path"
        | "clay" | "duff" | "hardened_clay" => GroundKind::Soil,
        _ => GroundKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposed_dike_toggle_selects_feature_rock_or_host_surface() {
        let world = World::new(0, crate::model::Settings::default());
        let x = -112_512;
        let z = -15_549;
        let mut detailed = crate::display::TerrainDetailOptions::full();
        detailed.caves = false;
        detailed.large_rocks_special_soils = false;
        let with_dike = sample_with_options(&world, x, z, &detailed).unwrap();
        detailed.exposed_dikes = false;
        let without_dike = sample_with_options(&world, x, z, &detailed).unwrap();
        assert_ne!(with_dike.block, without_dike.block);
        assert!(with_dike.block.starts_with("tfc:rock/raw/diorite"));
    }

    #[test]
    fn spring_toggle_only_adds_the_spring_surface_when_enabled() {
        let world = World::new(0, crate::model::Settings::default());
        let mut found = None;
        for z in (16_090..16_125).step_by(1) {
            for x in (-319_270..-319_230).step_by(1) {
                if crate::hot_springs::sample(&world, x, z).is_some() {
                    found = Some((x, z));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        let (x, z) = found.expect("reference area contains a generated spring surface");
        let mut options = crate::display::TerrainDetailOptions::full();
        options.caves = false;
        options.large_rocks_special_soils = false;
        options.springs = false;
        let ground_only = sample_with_options(&world, x, z, &options).unwrap();
        options.springs = true;
        let spring = sample_with_options(&world, x, z, &options).unwrap();
        assert_ne!(ground_only.block, spring.block);
    }

    #[test]
    fn surface_material_uses_the_solid_cap_below_heightmap_first_air() {
        let world = World::new(0, crate::model::Settings::default());
        for (x, z) in [(8833, 11107), (-65538, -65533), (14840, 14327)] {
            let cap =
                crate::surface_water::ordinary_solid_top(world.height(x, z).unwrap()).unwrap();
            assert_eq!(base_sample(&world, x, z).unwrap().y, cap);
        }
    }

    #[test]
    fn java_clamped_map_boundaries_match_tfg_surface_regions() {
        assert_eq!(map_index(35., 35., 450., 3., 17.99, 18), 3);
        // Mth.clampedMap clamps the interpolation fraction, not the final
        // index against zero. Arid terrain cannot enter the polar snow bands.
        assert_eq!(map_index(0., 35., 450., 3., 17.99, 18), 3);
        assert_eq!(map_index(-100., 35., 450., 3., 17.99, 18), 3);
        assert_eq!(map_index(450., 35., 450., 3., 17.99, 18), 17);
        assert_eq!(map_index(-100., -19., -4., 0., 17.99, 18), 0);
        assert_eq!(map_index(-4., -19., -4., 0., 17.99, 18), 17);
    }

    #[test]
    fn tfc_position_hash_matches_java_long_overflow_and_int_cast() {
        // Independently evaluated from Helpers.hash(salt, x, y, z) in the pinned source.
        assert_eq!(position_hash(1234, 70, -456, 729_375_982), 3_369_584_434);
    }

    #[test]
    fn rock_block_order_and_sand_palette_follow_tfc_registry() {
        assert_eq!(sand_variant("granite"), "brown");
        assert_eq!(sand_variant("gabbro"), "black");
        assert_eq!(sand_variant("basalt"), "red");
        assert_eq!(sand_variant("conglomerate"), "green");
        assert_eq!(sand_variant("chert"), "yellow");
        assert_eq!(kind_of("tfc:rock/gravel/granite"), GroundKind::Gravel);
        assert_eq!(kind_of("tfc:sand/black"), GroundKind::Sand);
        assert_eq!(kind_of("tfg:coarse_dirt/sandy_loam"), GroundKind::Soil);
        assert_eq!(kind_of("tfc:grass_path/sandy_loam"), GroundKind::Soil);
        assert_eq!(kind_of("tfc:raw_sandstone/red"), GroundKind::Rock);
    }

    #[test]
    fn soil_temperature_transition_uses_mean_climate_and_river_override() {
        assert_eq!(
            transitional_soil(
                ids::id::PLAINS,
                0,
                0,
                17.,
                "tfc:grass/silt",
                "tfg:grass/oxisol"
            ),
            "tfg:grass/oxisol"
        );
        assert_eq!(
            transitional_soil(
                ids::id::PLAINS,
                0,
                0,
                15.,
                "tfc:grass/silt",
                "tfg:grass/oxisol"
            ),
            "tfc:grass/silt"
        );
        assert_eq!(
            transitional_soil(
                ids::id::RIVER,
                0,
                0,
                25.,
                "tfc:grass/silty_loam",
                "tfg:grass/oxisol"
            ),
            "tfc:grass/silt"
        );
        assert!(
            elevation_temperature(17., 170) < 0.,
            "elevation cooling selects the soil band without changing the warm-soil transition input"
        );
    }
}
