//! Optional surface litter overlay for close-scale maps.
//!
//! Saved columns are authoritative. The overlay intentionally stores clutter apart
//! from the substrate so turning this layer off restores the bare ground color.
//! Forest debris, sticks, salt, dry grass and loose rocks replay installed feature
//! streams. Nested placed features resolve their own heightmaps; source terrain
//! and earlier vegetation can still differ from a complete generated chunk.

use crate::{
    dimension::Dimension,
    ground::{self, GroundKind, GroundSample},
    ground_palette,
    prism::{SurfaceChunk, SurfaceColumn},
    vegetation::{self, FeatureRandom},
    world::World,
};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};

const STEP: i32 = 9;
const ORIGIN_Y: i32 = -64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct CacheKey {
    seed: i64,
    settings: [u64; 6],
    cx: i32,
    cz: i32,
    source: u64,
}

impl CacheKey {
    fn new(world: &World, cx: i32, cz: i32) -> Self {
        let s = &world.region.settings;
        Self {
            seed: world.seed,
            settings: [
                s.temperature_scale.to_bits(),
                s.temperature_constant.to_bits(),
                s.rainfall_scale.to_bits(),
                s.rainfall_constant.to_bits(),
                s.continentalness.to_bits(),
                s.grass_density.to_bits(),
            ],
            cx,
            cz,
            source: world.surface_source_id(),
        }
    }
}

thread_local! {
    static SOURCE_CANDIDATES: RefCell<LruCache<CacheKey, Arc<Vec<LitterCandidate>>>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(2048).unwrap())
    );
    static TARGET_CHUNKS: RefCell<LruCache<CacheKey, Arc<LitterChunk>>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(512).unwrap())
    );
}

#[derive(Clone)]
struct LitterCandidate {
    x: i32,
    z: i32,
    block: LitterBlock,
    stage: i32,
    feature_index: u32,
    source_x: i32,
    source_z: i32,
    ordinal: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LitterKind {
    LooseRock,
    Stick,
    FallenLeaves,
    DryGrass,
    Salt,
    Groundcover,
}

/// One actual block at the saved surface, shown as a separate overlay pixel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LitterBlock {
    pub y: i32,
    pub block: String,
    pub kind: LitterKind,
    /// Texture-average color using Xaero's alpha cutoff and equal-RGB average.
    pub color: [u8; 3],
    /// Alpha coverage from the matching texture. A missing profile means opaque.
    pub opacity: u8,
}

/// Z-major chunk overlay. Empty entries mean the saved surface is ordinary terrain.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LitterChunk {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub columns: Vec<Option<LitterBlock>>,
}

impl LitterChunk {
    /// Extract only exposed, recognized clutter from a saved surface projection.
    /// No substrate fields are changed, so this overlay can be independently toggled.
    pub fn from_surface(chunk: &SurfaceChunk) -> Self {
        Self {
            chunk_x: chunk.chunk_x,
            chunk_z: chunk.chunk_z,
            columns: chunk.columns.iter().map(from_surface_column).collect(),
        }
    }

    fn empty(chunk_x: i32, chunk_z: i32) -> Self {
        Self {
            chunk_x,
            chunk_z,
            columns: vec![None; 256],
        }
    }

    fn put(&mut self, x: i32, z: i32, block: LitterBlock) -> bool {
        if x.div_euclid(16) != self.chunk_x || z.div_euclid(16) != self.chunk_z {
            return false;
        }
        self.columns[z.rem_euclid(16) as usize * 16 + x.rem_euclid(16) as usize] = Some(block);
        true
    }

    #[inline]
    pub fn get(&self, local_x: usize, local_z: usize) -> Option<&LitterBlock> {
        if local_x >= 16 || local_z >= 16 {
            return None;
        }
        self.columns.get(local_z * 16 + local_x)?.as_ref()
    }
}

#[derive(Clone, Copy)]
struct PatchRule {
    id: &'static str,
    chance: i32,
    tries: i32,
    xz_spread: i32,
    y_spread: i32,
    block: &'static str,
    kind: LitterKind,
    climate: Option<ClimateRule>,
    allow_water: bool,
    candidate_biome_filter: bool,
}

#[derive(Clone, Copy)]
struct ClimateRule {
    min_temp: f64,
    max_temp: f64,
    min_rain: f64,
    max_rain: f64,
    min_forest: u8,
    max_forest: u8,
    elevation_adjusted: bool,
}

const FOREST: ClimateRule = ClimateRule {
    min_temp: -20.0,
    max_temp: 50.0,
    min_rain: 70.0,
    max_rain: 500.0,
    min_forest: 3,
    max_forest: 4,
    elevation_adjusted: false,
};
const RULES: &[PatchRule] = &[
    PatchRule {
        id: "tfc:sticks_shore_patch",
        chance: 2,
        tries: 18,
        xz_spread: 15,
        y_spread: 1,
        block: "tfc:groundcover/stick",
        kind: LitterKind::Stick,
        climate: Some(ClimateRule {
            min_temp: -50.0,
            max_temp: 50.0,
            min_rain: 50.0,
            max_rain: 500.0,
            min_forest: 0,
            max_forest: 4,
            elevation_adjusted: false,
        }),
        allow_water: true,
        candidate_biome_filter: true,
    },
    PatchRule {
        id: "tfc:sticks_forest_patch",
        chance: 3,
        tries: 20,
        xz_spread: 15,
        y_spread: 1,
        block: "tfc:groundcover/stick",
        kind: LitterKind::Stick,
        climate: Some(FOREST),
        allow_water: false,
        candidate_biome_filter: false,
    },
    PatchRule {
        id: "tfc:salt_lick_patch",
        chance: 110,
        tries: 100,
        xz_spread: 5,
        y_spread: 1,
        block: "tfc:groundcover/salt_lick",
        kind: LitterKind::Salt,
        climate: Some(ClimateRule {
            min_temp: 5.0,
            max_temp: 33.0,
            min_rain: 100.0,
            max_rain: 500.0,
            min_forest: 3,
            max_forest: 4,
            elevation_adjusted: false,
        }),
        allow_water: false,
        candidate_biome_filter: false,
    },
    PatchRule {
        id: "tfg:earth/plant/dry_grass_patch",
        chance: 5,
        tries: 10,
        xz_spread: 15,
        y_spread: 1,
        block: "tfg:plant/dry_grass",
        kind: LitterKind::DryGrass,
        climate: Some(ClimateRule {
            min_temp: -13.0,
            max_temp: 40.0,
            min_rain: 15.0,
            max_rain: 95.0,
            min_forest: 2,
            max_forest: 4,
            elevation_adjusted: true,
        }),
        allow_water: false,
        candidate_biome_filter: false,
    },
    PatchRule {
        id: "tfg:earth/salt_lick_patch",
        chance: 2,
        tries: 32,
        xz_spread: 8,
        y_spread: 1,
        block: "tfc:groundcover/salt_lick",
        kind: LitterKind::Salt,
        climate: None,
        allow_water: false,
        candidate_biome_filter: true,
    },
];

/// Predict litter in a target chunk by combining source candidates from the
/// surrounding 3×3 source-chunk neighborhood. FeatureSorter indices,
/// feature seeds, rarity, in-square placement and RandomPatch offset order are taken
/// from the installed TFG 0.9.25 datapack. Heightmap resolution and simple-block
/// survival use the same terrain/base block sampler as the ground layer. The sampler
/// cannot replay prior stage-9 feature mutations, so it can miss/reposition clutter
/// where a previously generated tree changes WORLD_SURFACE_WG or occupies the target.
/// Source candidates are cached separately, then clipped by absolute coordinates;
/// cross-chunk feature patches therefore do not wrap into the source chunk. It is a
/// bounded close-scale overlay; fast/basic map profiles never request this pass.
pub fn predict_chunk(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Option<Arc<LitterChunk>> {
    if world.dimension != Dimension::Overworld {
        return Some(Arc::new(LitterChunk::empty(chunk_x, chunk_z)));
    }
    let key = CacheKey::new(world, chunk_x, chunk_z);
    if let Some(cached) = TARGET_CHUNKS.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        return Some(cached);
    }
    if cancel() {
        return None;
    }
    let mut chunk = LitterChunk::empty(chunk_x, chunk_z);
    let mut candidates = Vec::new();
    for source_z in chunk_z.saturating_sub(1)..=chunk_z.saturating_add(1) {
        for source_x in chunk_x.saturating_sub(1)..=chunk_x.saturating_add(1) {
            if cancel() {
                return None;
            }
            let Some(source) = source_candidates(world, source_x, source_z, cancel) else {
                return None;
            };
            candidates.extend(
                source
                    .iter()
                    .filter(|candidate| {
                        candidate.x.div_euclid(16) == chunk_x
                            && candidate.z.div_euclid(16) == chunk_z
                    })
                    .cloned(),
            );
        }
    }
    candidates.sort_by_key(|candidate| {
        (
            candidate.stage,
            candidate.feature_index,
            candidate.source_z,
            candidate.source_x,
            candidate.ordinal,
        )
    });
    for candidate in candidates {
        if cancel() {
            return None;
        }
        chunk.put(candidate.x, candidate.z, candidate.block);
    }
    if cancel() {
        return None;
    }
    let chunk = Arc::new(chunk);
    TARGET_CHUNKS.with(|cache| cache.borrow_mut().put(key, Arc::clone(&chunk)));
    Some(chunk)
}

fn source_candidates(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Option<Arc<Vec<LitterCandidate>>> {
    if cancel() {
        return None;
    }
    let key = CacheKey::new(world, chunk_x, chunk_z);
    if let Some(cached) = SOURCE_CANDIDATES.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        return Some(cached);
    }
    let mut output = Vec::new();
    let mut ordinal = 0u32;
    let forest_index = vegetation::feature_index_in_step("tfg:earth/forest", 8).unwrap_or(0);
    // ForestFeature's stage-8 twig/leaf debris uses the same RNG continuation as
    // tree template placement.
    for block in vegetation::forest_litter_source_chunk(world, chunk_x, chunk_z, cancel) {
        if cancel() {
            return None;
        }
        if let Some(kind) = classify(&block.block) {
            output.push(candidate(
                block.x,
                block.z,
                block.y,
                &block.block,
                kind,
                8,
                forest_index,
                chunk_x,
                chunk_z,
                ordinal,
            ));
            ordinal = ordinal.saturating_add(1);
        }
    }
    let mut rules = RULES.iter().collect::<Vec<_>>();
    rules.sort_by_key(|rule| vegetation::feature_index_in_step(rule.id, STEP).unwrap_or(u32::MAX));
    for rule in rules {
        if cancel() {
            return None;
        }
        let Some(index) = vegetation::feature_index_in_step(rule.id, STEP) else {
            continue;
        };
        let mut random = FeatureRandom::new(vegetation::feature_seed(
            chunk_x, chunk_z, ORIGIN_Y, STEP, index,
        ));
        // Minecraft RarityFilter uses nextFloat, then InSquare draws X then Z.
        if random.next_float() >= 1.0 / rule.chance as f32 {
            continue;
        }
        let patch_x = chunk_x
            .wrapping_mul(16)
            .wrapping_add(random.next_int_bound(16));
        let patch_z = chunk_z
            .wrapping_mul(16)
            .wrapping_add(random.next_int_bound(16));
        if rule.climate.is_some_and(|climate| {
            !climate_matches(world, patch_x, patch_z, climate, rule.allow_water)
        }) {
            continue;
        }
        if !vegetation::feature_biome_allowed(world, chunk_x, chunk_z, rule.id)
            || (rule.candidate_biome_filter
                && !vegetation::feature_biome_at_position_allowed(world, patch_x, patch_z, rule.id))
        {
            continue;
        }
        let x_bound = rule.xz_spread + 1;
        let y_bound = rule.y_spread + 1;
        for _ in 0..rule.tries {
            if cancel() {
                return None;
            }
            // Minecraft 1.20.1 RandomPatchFeature draws x−x, y−y, z−z.
            let dx = random.next_int_bound(x_bound) - random.next_int_bound(x_bound);
            let _dy = random.next_int_bound(y_bound) - random.next_int_bound(y_bound);
            let dz = random.next_int_bound(x_bound) - random.next_int_bound(x_bound);
            let x = patch_x.wrapping_add(dx);
            let z = patch_z.wrapping_add(dz);
            let biome = world.biome_id(x, z);
            if !rule.allow_water && (crate::biome::is_ocean(biome) || world.is_surface_water(x, z))
            {
                continue;
            }
            let Some(base) = ground::base_sample(world, x, z) else {
                continue;
            };
            // The child placed features require a replaceable first-air target,
            // then their block's canSurvive check against the actual support type.
            if rule.kind == LitterKind::DryGrass && !above_high_tide(world, x, base.y + 1, z) {
                continue;
            }
            if !survives(rule.kind, &base) {
                continue;
            }
            output.push(candidate(
                x,
                z,
                base.y.saturating_add(1),
                rule.block,
                rule.kind,
                STEP,
                index,
                chunk_x,
                chunk_z,
                ordinal,
            ));
            ordinal = ordinal.saturating_add(1);
        }
    }
    append_loose_rocks(world, chunk_x, chunk_z, cancel, &mut output)?;
    if cancel() {
        return None;
    }
    let output = Arc::new(output);
    SOURCE_CANDIDATES.with(|cache| cache.borrow_mut().put(key, Arc::clone(&output)));
    Some(output)
}

fn append_loose_rocks(
    world: &World,
    cx: i32,
    cz: i32,
    cancel: &impl Fn() -> bool,
    output: &mut Vec<LitterCandidate>,
) -> Option<()> {
    const ID: &str = "tfc:surface_loose_rocks";
    let Some(index) = vegetation::feature_index_in_step(ID, 10) else {
        return Some(());
    };
    if !vegetation::feature_biome_allowed(world, cx, cz, ID) {
        return Some(());
    }
    let mut random = FeatureRandom::new(vegetation::feature_seed(cx, cz, ORIGIN_Y, 10, index));
    // The outer feature has only InSquare. The nested tfc:loose_rock placed
    // feature resets Y to OCEAN_FLOOR_WG for every random-patch attempt.
    let ox = cx.wrapping_mul(16).wrapping_add(random.next_int_bound(16));
    let oz = cz.wrapping_mul(16).wrapping_add(random.next_int_bound(16));
    for ordinal in 0..8 {
        if cancel() {
            return None;
        }
        let x = ox + random.next_int_bound(8) - random.next_int_bound(8);
        let _dy = random.next_int_bound(2) - random.next_int_bound(2);
        let z = oz + random.next_int_bound(8) - random.next_int_bound(8);
        let Some(support) = ground::base_sample(world, x, z) else {
            continue;
        };
        let y = support.y.saturating_add(1);
        let Some(rock) = world.rock_at(x, y, z) else {
            continue;
        };
        let rock = rock.rsplit('/').next().unwrap_or(&rock);
        let wet = world.surface_water_y(x, z).is_some_and(|wy| wy >= y);
        // Source short-circuit order draws the moss boolean before the fluid
        // check. Counts are drawn only after the support/survival tests pass.
        let mossy = world.climate(x, z).rainfall > 250. && random.next(1) != 0 && !wet;
        if !survives(LitterKind::LooseRock, &support) {
            continue;
        }
        let _count = 1 + random.next_int_bound(2);
        let block = format!(
            "tfc:rock/{}/{rock}",
            if mossy { "mossy_loose" } else { "loose" }
        );
        output.push(candidate(
            x,
            z,
            y,
            &block,
            LitterKind::LooseRock,
            10,
            index,
            cx,
            cz,
            ordinal,
        ));
    }
    Some(())
}

fn candidate(
    x: i32,
    z: i32,
    y: i32,
    block: &str,
    kind: LitterKind,
    stage: i32,
    feature_index: u32,
    source_x: i32,
    source_z: i32,
    ordinal: u32,
) -> LitterCandidate {
    let (color, opacity) = palette(block).unwrap_or(([128, 128, 128], 255));
    LitterCandidate {
        x,
        z,
        block: LitterBlock {
            y,
            block: block.to_owned(),
            kind,
            color,
            opacity,
        },
        stage,
        feature_index,
        source_x,
        source_z,
        ordinal,
    }
}

/// Sample predicted litter at one world column. Repeated pixel requests share the
/// bounded chunk cache, so a tile renderer need not retain chunk internals.
pub fn sample_predicted(
    world: &World,
    x: i32,
    z: i32,
    cancel: &impl Fn() -> bool,
) -> Option<LitterBlock> {
    let chunk = predict_chunk(world, x.div_euclid(16), z.div_euclid(16), cancel)?;
    chunk
        .get(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize)
        .cloned()
}

fn above_high_tide(world: &World, x: i32, y: i32, z: i32) -> bool {
    thread_local! {
        static NOISE:RefCell<Option<(i64,crate::noise::OpenSimplex2D,crate::noise::OpenSimplex2D)>>=const {RefCell::new(None)};
    }
    NOISE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|(seed, _, _)| *seed != world.seed)
        {
            let mut tide = crate::noise::OpenSimplex2D::new(world.seed);
            tide.octaves(3)
                .spread(f64::from(0.005_f32))
                .scaled(57., 69.);
            let mut detail = crate::noise::OpenSimplex2D::new(world.seed);
            detail.spread(0.03);
            *cache = Some((world.seed, tide, detail));
        }
        let (_, tide, detail) = cache.as_ref().unwrap();
        let high =
            tide.sample(x as f64, z as f64).clamp(63., 67.) + detail.sample(x as f64, z as f64);
        let difference = (y as f64 - high) as i32;
        (1..=512).contains(&difference)
    })
}

fn climate_matches(world: &World, x: i32, z: i32, rule: ClimateRule, allow_water: bool) -> bool {
    let biome = world.biome_id(x, z);
    if !allow_water && crate::biome::is_ocean(biome) {
        return false;
    }
    let climate = world.climate(x, z);
    let forest = world.region.forest_type(x >> 4, z >> 4);
    // Only dry-grass outer placement resolves WORLD_SURFACE_WG before
    // its climate test; forest/shore/salt patches test at the -64 origin.
    let temperature = if rule.elevation_adjusted {
        let y = world.height(x, z).unwrap_or(63.) as f32 + 1.;
        climate.temperature - f64::from(((y - 63.) * 0.16225).clamp(0., 17.822))
    } else {
        climate.temperature
    };
    temperature >= rule.min_temp
        && temperature <= rule.max_temp
        && climate.rainfall >= rule.min_rain
        && climate.rainfall <= rule.max_rain
        && forest >= rule.min_forest
        && forest <= rule.max_forest
}

fn survives(kind: LitterKind, support: &GroundSample) -> bool {
    match kind {
        // TFC GroundcoverBlock has no custom survival predicate; its vanilla
        // default is true. The placed feature separately checks that the
        // heightmap target is replaceable, which our first-air lookup provides.
        LitterKind::Stick | LitterKind::Salt | LitterKind::Groundcover => true,
        LitterKind::DryGrass => support.kind == GroundKind::Sand || dry_plantable(&support.block),
        LitterKind::FallenLeaves => false,
        // LooseRockFeature has a separate `canSurvive` + ice exclusion and is
        // projected independently from the stage-9 random-patch rules.
        LitterKind::LooseRock => !matches!(support.kind, GroundKind::Ice | GroundKind::Other),
    }
}

fn dry_plantable(block: &str) -> bool {
    block.starts_with("tfc:grass/")
        || block.starts_with("tfg:grass/")
        || block == "tfc:farmland"
        || block == "minecraft:farmland"
        || matches!(
            block,
            "minecraft:dirt" | "minecraft:coarse_dirt" | "minecraft:rooted_dirt"
        )
        || block.starts_with("tfc:dirt/")
        || block.starts_with("tfg:dirt/")
        || block.starts_with("tfc:clay_grass/")
        || block.starts_with("tfg:clay_grass/")
        || block.starts_with("tfc:rooted_dirt/")
        || block.starts_with("tfg:coarse_dirt/")
        || block.starts_with("tfc:farmland/")
        || block.starts_with("tfg:farmland/")
        || block.starts_with("tfc:soil/dirt/")
        || block.starts_with("tfg:soil/dirt/")
        || block.starts_with("tfc:sand/")
        || block.starts_with("tfc:rock/sand/")
}

fn from_surface_column(column: &SurfaceColumn) -> Option<LitterBlock> {
    let kind = classify(&column.block)?;
    let (color, opacity) = palette(&column.block).unwrap_or((column.color, 255));
    Some(LitterBlock {
        y: column.y,
        block: column.block.clone(),
        kind,
        color,
        opacity,
    })
}

/// Recognize only blocks that are explicit surface litter/ground effects in TFC/TFG.
/// Exposed foliage, crops and natural ground materials remain owned by their layers.
pub fn classify(block: &str) -> Option<LitterKind> {
    let path = block.rsplit_once(':').map_or(block, |(_, path)| path);
    if path.starts_with("rock/loose/")
        || path.starts_with("rock/mossy_loose/")
        || path.starts_with("loose_rock/")
    {
        Some(LitterKind::LooseRock)
    } else if path == "groundcover/stick" || path.ends_with("/groundcover/stick") {
        Some(LitterKind::Stick)
    } else if path.starts_with("wood/twig/") {
        Some(LitterKind::Stick)
    } else if path.starts_with("wood/fallen_leaves/") {
        Some(LitterKind::FallenLeaves)
    } else if path == "plant/dry_grass" || path.ends_with("/dry_grass") {
        Some(LitterKind::DryGrass)
    } else if path == "groundcover/salt_lick" {
        Some(LitterKind::Salt)
    } else if path.starts_with("groundcover/") {
        Some(LitterKind::Groundcover)
    } else {
        None
    }
}

/// Texture profile for an installed TFG litter block, with an opaque fallback for
/// older palettes. Alpha is kept separate so renderers can composite loose sprites.
pub fn palette(block: &str) -> Option<([u8; 3], u8)> {
    #[derive(Deserialize)]
    struct TextureColor {
        rgb: [u8; 3],
        opacity: u8,
    }
    static COLORS: OnceLock<std::collections::HashMap<String, TextureColor>> = OnceLock::new();
    let colors = COLORS.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/ground-colors/litter_palette.json"))
            .expect("bundled litter texture palette is valid")
    });
    colors
        .get(block)
        .map(|color| (color.rgb, color.opacity))
        .or_else(|| {
            let path = block.split_once(':')?.1;
            let key = if let Some(rock) = path.strip_prefix("rock/loose/") {
                format!("tfc:rock/raw/{rock}")
            } else {
                let rock = path.strip_prefix("rock/mossy_loose/")?;
                format!("tfc:rock/mossy_cobble/{rock}")
            };
            ground_palette::color(&key).map(|rgb| (rgb, 255))
        })
        .or_else(|| {
            let (_, path) = block.split_once(':')?;
            let species = path.strip_prefix("wood/twig/")?;
            ground_palette::tree_color(species, false).map(|rgb| (rgb, 255))
        })
        .or_else(|| {
            block
                .strip_prefix("tfc:wood/fallen_leaves/")
                .and_then(|species| {
                    ground_palette::tree_color(&format!("tfc:tree/{species}"), true)
                })
                .map(|rgb| (rgb, 255))
        })
        .or_else(|| ground_palette::color(block).map(|rgb| (rgb, 255)))
}

/// Composite one clutter texture sample over the existing substrate. The input
/// opacity is the Xaero-style mean texture coverage, independent of RGB averaging.
#[inline]
pub fn composite(base: [u8; 3], litter: &LitterBlock) -> [u8; 3] {
    let alpha = u32::from(litter.opacity);
    std::array::from_fn(|i| {
        ((u32::from(litter.color[i]) * alpha + u32::from(base[i]) * (255 - alpha) + 127) / 255)
            as u8
    })
}

/// Render color using Xaero's texture RGB × registered block tint. Only fallen
/// leaves are tinted in TFC's block color registry; other clutter stays at the
/// source texture color. `season` is the normalized fraction of the calendar year.
pub fn color_at(
    world: &World,
    litter: &LitterBlock,
    x: i32,
    z: i32,
    season: Option<f32>,
) -> [u8; 3] {
    if litter.kind != LitterKind::FallenLeaves {
        return litter.color;
    }
    let Some(species) = litter
        .block
        .split_once(':')
        .and_then(|(_, p)| p.strip_prefix("wood/fallen_leaves/"))
    else {
        return litter.color;
    };
    let climate = world.climate(x, z);
    let tint = crate::map_colors::fallen_leaves_at(
        climate.temperature,
        climate.rainfall,
        x,
        litter.y,
        z,
        species,
        world.region.settings.temperature_scale,
        season,
    );
    ground_palette::multiply_tint(litter.color, tint)
}

/// Alpha-composite one litter texture over terrain, applying any registered
/// foliage tint before compositing. The substrate always remains the base pixel.
#[inline]
pub fn composite_at(
    base: [u8; 3],
    world: &World,
    litter: &LitterBlock,
    x: i32,
    z: i32,
    season: Option<f32>,
) -> [u8; 3] {
    let color = color_at(world, litter, x, z, season);
    let alpha = u32::from(litter.opacity);
    std::array::from_fn(|i| {
        ((u32::from(color[i]) * alpha + u32::from(base[i]) * (255 - alpha) + 127) / 255) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::{
        LitterBlock, LitterChunk, LitterKind, classify, composite, dry_plantable, palette,
    };
    use crate::prism::{SurfaceChunk, SurfaceColumn};
    use crate::vegetation;

    #[test]
    fn patch_rules_match_independently_extracted_installed_configs() {
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/ground/litter-rules.json"))
                .unwrap();
        for raw in reference["features"].as_array().unwrap() {
            if raw["id"] == "tfc:surface_loose_rocks" {
                assert_eq!(raw["configured"]["config"]["tries"], 8);
                assert_eq!(raw["configured"]["config"]["xz_spread"], 7);
                assert_eq!(
                    raw["child_placed"]["placement"][0]["heightmap"],
                    "OCEAN_FLOOR_WG"
                );
                continue;
            }
            let rule = super::RULES
                .iter()
                .find(|r| r.id == raw["id"].as_str().unwrap())
                .unwrap();
            let config = &raw["configured"]["config"];
            assert_eq!(
                Some(i64::from(rule.tries)),
                config["tries"].as_i64(),
                "{} tries",
                rule.id
            );
            assert_eq!(
                Some(i64::from(rule.xz_spread)),
                config["xz_spread"].as_i64(),
                "{} XZ spread",
                rule.id
            );
            assert_eq!(Some(i64::from(rule.y_spread)), config["y_spread"].as_i64());
            let placed = raw["placed"]["placement"].as_array().unwrap();
            assert_eq!(Some(i64::from(rule.chance)), placed[0]["chance"].as_i64());
            assert_eq!(placed[1]["type"], "minecraft:in_square");
            assert!(
                raw["child_placed"]["placement"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["type"] == "minecraft:heightmap"),
                "{} child must reset the random-patch Y offset",
                rule.id
            );
        }
    }

    #[test]
    fn litter_clips_absolute_coordinates_including_negative_chunk_edges() {
        let block = LitterBlock {
            y: 70,
            block: "tfc:groundcover/stick".into(),
            kind: LitterKind::Stick,
            color: [100; 3],
            opacity: 95,
        };
        let mut left = LitterChunk::empty(-1, 0);
        assert!(left.put(-1, 0, block.clone()));
        assert!(!left.put(0, 0, block.clone()));
        assert!(!left.put(-17, 0, block.clone()));
        assert!(left.get(15, 0).is_some());
        assert!(left.get(0, 0).is_none());
        let mut right = LitterChunk::empty(0, 0);
        assert!(right.put(0, 0, block));
        assert!(right.get(0, 0).is_some());
    }

    #[test]
    fn loose_mossy_rocks_and_species_twigs_use_model_texture_colors() {
        assert_eq!(
            classify("tfc:rock/mossy_loose/andesite"),
            Some(LitterKind::LooseRock)
        );
        assert_eq!(
            palette("tfc:rock/mossy_loose/andesite").unwrap().0,
            crate::ground_palette::color("tfc:rock/mossy_cobble/andesite").unwrap()
        );
        assert_eq!(
            palette("afc:wood/twig/eucalyptus").unwrap().0,
            crate::ground_palette::tree_color("eucalyptus", false).unwrap()
        );
    }

    #[test]
    fn classification_is_limited_to_explicit_surface_effects() {
        assert_eq!(classify("tfc:groundcover/stick"), Some(LitterKind::Stick));
        assert_eq!(
            classify("tfc:wood/fallen_leaves/maple"),
            Some(LitterKind::FallenLeaves)
        );
        assert_eq!(classify("tfg:plant/dry_grass"), Some(LitterKind::DryGrass));
        assert_eq!(
            classify("tfc:groundcover/salt_lick"),
            Some(LitterKind::Salt)
        );
        assert_eq!(
            classify("tfc:rock/loose/granite"),
            Some(LitterKind::LooseRock)
        );
        assert_eq!(classify("tfc:rock/raw/granite"), None);
        assert_eq!(classify("tfc:grass/loam"), None);
    }

    #[test]
    fn installed_stage_nine_features_keep_their_feature_sorter_positions() {
        assert_eq!(
            vegetation::feature_index_in_step("tfc:sticks_forest_patch", 9),
            Some(80)
        );
        assert_eq!(
            vegetation::feature_index_in_step("tfc:sticks_shore_patch", 9),
            Some(30)
        );
        assert_eq!(
            vegetation::feature_index_in_step("tfg:earth/salt_lick_patch", 9),
            Some(0)
        );
        assert_eq!(
            vegetation::feature_index_in_step("tfc:salt_lick_patch", 9),
            Some(82)
        );
        assert_eq!(
            vegetation::feature_index_in_step("tfg:earth/plant/dry_grass_patch", 9),
            Some(189)
        );
        assert_eq!(
            vegetation::feature_index_in_step("tfc:surface_loose_rocks", 10),
            Some(0)
        );
    }

    #[test]
    fn dry_grass_survival_matches_pinned_dry_plantable_supports() {
        assert!(dry_plantable("tfc:grass/loam"));
        assert!(dry_plantable("tfc:sand/red"));
        assert!(dry_plantable("tfc:dirt/loam"));
        assert!(dry_plantable("tfg:grass/alfisol"));
        assert!(!dry_plantable("tfc:rock/raw/granite"));
    }

    #[test]
    fn saved_litter_palette_matches_shipped_texture_sample() {
        assert_eq!(palette("tfc:groundcover/stick"), Some(([61, 55, 23], 95)));
        assert_eq!(
            palette("tfc:groundcover/salt_lick"),
            Some(([231, 231, 231], 255))
        );
        assert!(palette("tfg:plant/dry_grass").is_some());
    }

    #[test]
    fn saved_litter_stays_separate_from_bare_substrate() {
        let mut columns = vec![
            SurfaceColumn {
                y: 71,
                block: "tfc:grass/loam".into(),
                tree: None,
                crop: None,
                color: [80, 120, 70]
            };
            256
        ];
        columns[3 * 16 + 5] = SurfaceColumn {
            y: 72,
            block: "tfc:groundcover/stick".into(),
            tree: None,
            crop: None,
            color: [1, 2, 3],
        };
        let chunk = SurfaceChunk {
            chunk_x: -1,
            chunk_z: 2,
            columns,
        };
        let litter = LitterChunk::from_surface(&chunk);
        assert_eq!(litter.get(5, 3).unwrap().y, 72);
        assert_eq!(litter.get(5, 3).unwrap().kind, LitterKind::Stick);
        assert!(litter.get(4, 3).is_none());
        assert_eq!(chunk.columns[3 * 16 + 4].block, "tfc:grass/loam");
    }

    #[test]
    fn translucent_clutter_composites_without_replacing_the_substrate() {
        let litter = LitterBlock {
            y: 1,
            block: "tfc:groundcover/stick".into(),
            kind: LitterKind::Stick,
            color: [200, 100, 0],
            opacity: 128,
        };
        assert_eq!(composite([0, 100, 200], &litter), [100, 100, 100]);
        assert_eq!(
            composite(
                [1, 2, 3],
                &LitterBlock {
                    opacity: 0,
                    ..litter.clone()
                }
            ),
            [1, 2, 3]
        );
        assert_eq!(
            composite(
                [1, 2, 3],
                &LitterBlock {
                    opacity: 255,
                    ..litter
                }
            ),
            [200, 100, 0]
        );
    }
}
