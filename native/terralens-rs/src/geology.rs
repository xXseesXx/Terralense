//! TFC/TFG rock-layer settings. The layer tree and choice order come from the installed
//! TerraFirmaGreg rock settings; `point_rock` is the packed Region.Point.rock value.
use lru::LruCache;
use serde::Deserialize;
use std::sync::OnceLock;
use std::{collections::HashMap, num::NonZeroUsize};

#[path = "geology/height_noise.rs"]
mod height_noise;
#[path = "geology/river_height.rs"]
pub(crate) mod river_height;
#[path = "geology/shore_height.rs"]
mod shore_height;
#[path = "geology/surface_context.rs"]
mod surface_context;
#[path = "geology/volcanic_height.rs"]
mod volcanic_height;

const TFG_ROCK_LAYERS: &str = include_str!("../assets/data/tfg/rock-layers.json");
const TFG_BIOME_HEIGHTS: &str = include_str!("../assets/data/tfg/biome-heights.json");
const TFG_BIOME_BLEND_TYPES: &str = include_str!("../assets/data/tfg/biome-blend-types.json");

#[derive(Deserialize)]
struct RockSettings {
    bottom: Vec<String>,
    land: Vec<String>,
    #[serde(rename = "oceanFloor")]
    ocean_floor: Vec<String>,
    volcanic: Vec<String>,
    uplift: Vec<String>,
    layers: Vec<LayerDef>,
}

#[derive(Deserialize)]
struct LayerDef {
    id: String,
    entries: Vec<(String, String)>,
}

#[derive(Debug)]
struct LayerEntry {
    rock: String,
    next_layer: usize,
}

struct RockLayerSampler {
    // Entry vectors are flattened top-level layers. The four indices are ocean, volcanic, land,
    // uplift, matching ChooseRocks' TYPE_MASK values.
    roots: [Vec<LayerEntry>; 4],
    named: Vec<Vec<LayerEntry>>,
}

static SAMPLER: OnceLock<RockLayerSampler> = OnceLock::new();

fn sampler() -> &'static RockLayerSampler {
    SAMPLER.get_or_init(|| {
        let data: RockSettings = serde_json::from_str(TFG_ROCK_LAYERS)
            .expect("bundled TFG rock layer settings must be valid");
        let mut layers: Vec<(String, Vec<(String, String)>)> =
            Vec::with_capacity(data.layers.len() + 1);
        layers.push((
            "bottom".into(),
            data.bottom
                .iter()
                .map(|rock| (rock.clone(), "bottom".into()))
                .collect(),
        ));
        layers.extend(
            data.layers
                .into_iter()
                .map(|layer| (layer.id, layer.entries)),
        );

        let indices: std::collections::HashMap<String, usize> = layers
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (id.clone(), i))
            .collect();
        let baked: Vec<Vec<LayerEntry>> = layers
            .iter()
            .map(|(id, entries)| {
                entries
                    .iter()
                    .map(|(rock, parent)| LayerEntry {
                        rock: rock.clone(),
                        next_layer: *indices.get(parent).unwrap_or_else(|| {
                            panic!("rock layer {id} references missing parent {parent}")
                        }),
                    })
                    .collect()
            })
            .collect();
        let get_root = |ids: &[String]| -> Vec<LayerEntry> {
            let mut root = Vec::new();
            for id in ids {
                let index = *indices
                    .get(id)
                    .unwrap_or_else(|| panic!("missing TFG rock layer {id}"));
                root.extend(baked[index].iter().map(|entry| LayerEntry {
                    rock: entry.rock.clone(),
                    next_layer: entry.next_layer,
                }));
            }
            root
        };
        RockLayerSampler {
            roots: [
                get_root(&data.ocean_floor),
                get_root(&data.volcanic),
                get_root(&data.land),
                get_root(&data.uplift),
            ],
            named: baked,
        }
    })
}

/// Sample a named rock at `layer_n` using TFC's `RockLayerSettings.sampleAtLayer` algorithm.
/// The packed value's category is its low two bits; its upper bits seed Minecraft's Xoroshiro.
pub fn sample_rock_layer(point_rock: i32, layer_n: usize) -> String {
    let settings = sampler();
    let category = (point_rock & 3) as usize;
    let root = &settings.roots[category];
    if root.is_empty() {
        return String::from("unknown");
    }
    let seed = point_rock >> 2;
    let mut rng = crate::random::Xoroshiro::new(seed as i64);
    let mut entry = &root[rng.next_int_bound(root.len() as i32) as usize];
    for _ in 0..layer_n {
        let next = &settings.named[entry.next_layer];
        if next.is_empty() {
            break;
        }
        entry = &next[rng.next_int_bound(next.len() as i32) as usize];
    }
    entry.rock.clone()
}

/// Return the expected rock stack at a point (top, middle, deepest) for readouts and vein hosts.
pub fn rock_stack(point_rock: i32) -> [String; 3] {
    [
        sample_rock_layer_id(point_rock, 0),
        sample_rock_layer_id(point_rock, 1),
        sample_rock_layer_id(point_rock, bottom_depth_for(point_rock)),
    ]
}

/// Number of transitions required to reach the bottom rock list for this category.
pub fn bottom_depth_for(point_rock: i32) -> usize {
    match point_rock & 3 {
        0 => 2,
        1 => 3,
        2 => 3,
        _ => 4,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TreeEntry {
    species: String,
    min_temperature: f64,
    max_temperature: f64,
    min_rainfall: f64,
    max_rainfall: f64,
}
#[derive(Deserialize)]
struct TreeEcologyData {
    entries: Vec<TreeEntry>,
}
static TREE_ENTRIES: OnceLock<Vec<TreeEntry>> = OnceLock::new();

/// Expected tree feature attempts for a sampled biome area. Climate eligibility comes from the
/// installed TFC/AFC forests; midpoint forest density and the source's 60% continuation rule are
/// modeled. Terrain placement failures remain outside this estimate.
pub fn estimate_trees(probe: &crate::model::Probe, area_blocks: f64) -> crate::model::TreeEstimate {
    let entries = TREE_ENTRIES.get_or_init(|| {
        serde_json::from_str::<TreeEcologyData>(include_str!(
            "../assets/data/tfg/tree-ecology.json"
        ))
        .expect("bundled TFG tree ecology must be valid")
        .entries
    });
    let area = if area_blocks.is_finite() {
        area_blocks.max(0.0)
    } else {
        0.0
    };
    let ty = (probe.forest as usize).min(4);
    let base_attempts = [0.0, 2.0, 2.0, 5.0, 7.0][ty];
    let sparse_chance = if ty == 1 {
        0.08
    } else if ty == 0 {
        0.0
    } else {
        1.0
    };
    let total = area / 256.0 * base_attempts * 1.05 * sparse_chance;
    let mut eligible: Vec<&TreeEntry> = entries
        .iter()
        .filter(|e| {
            probe.climate.temperature >= e.min_temperature
                && probe.climate.temperature <= e.max_temperature
                && probe.climate.rainfall >= e.min_rainfall
                && probe.climate.rainfall <= e.max_rainfall
        })
        .collect();
    eligible.sort_by(|a, b| {
        let da = ((probe.climate.temperature - (a.min_temperature + a.max_temperature) / 2.0)
            + (probe.climate.rainfall - (a.min_rainfall + a.max_rainfall) / 2.0))
            .abs();
        let db = ((probe.climate.temperature - (b.min_temperature + b.max_temperature) / 2.0)
            + (probe.climate.rainfall - (b.min_rainfall + b.max_rainfall) / 2.0))
            .abs();
        da.total_cmp(&db).then_with(|| a.species.cmp(&b.species))
    });
    let raw: Vec<f64> = (0..eligible.len())
        .map(|i| {
            if i + 1 == eligible.len() {
                0.6f64.powi(i as i32)
            } else {
                0.4 * 0.6f64.powi(i as i32)
            }
        })
        .collect();
    let weight_sum: f64 = raw.iter().sum();
    let mut grouped = std::collections::BTreeMap::<String, f64>::new();
    for (entry, weight) in eligible.iter().zip(raw) {
        *grouped.entry(entry.species.clone()).or_default() += if weight_sum > 0.0 {
            weight / weight_sum
        } else {
            0.0
        };
    }
    let mut species: Vec<crate::model::TreeSpecies> = grouped
        .into_iter()
        .map(|(name, share)| crate::model::TreeSpecies {
            name,
            count: (total * share * 10.0).round() / 10.0,
            share,
        })
        .filter(|entry| entry.share > 0.005)
        .collect();
    species.sort_by(|a, b| {
        b.share
            .total_cmp(&a.share)
            .then_with(|| a.name.cmp(&b.name))
    });
    crate::model::TreeEstimate {
        total: (total * 10.0).round() / 10.0,
        per_hectare: if area > 0.0 {
            (total * 10_000.0 / area * 10.0).round() / 10.0
        } else {
            0.0
        },
        species,
    }
}

fn sample_rock_layer_id(point_rock: i32, layer_n: usize) -> String {
    sample_rock_layer(point_rock, layer_n)
}

/// TFG's 7x7 biome-height blend at a block coordinate. Returns `None` when the blended
/// biome set contains a biome without a surface-height factory (currently `river`); callers can
/// report that height as unavailable instead of displaying an invented sea-level value.
pub fn surface_height(
    region: &crate::region::Generator,
    seed: i64,
    block_x: i32,
    block_z: i32,
) -> Option<f64> {
    HEIGHT_SAMPLER.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|sampler| sampler.seed != seed || sampler.settings != region.settings)
        {
            *cache = Some(HeightSampler::new(seed, region.settings.clone()));
        }
        cache
            .as_mut()
            .expect("height sampler initialized")
            .sample(region, block_x, block_z)
    })
}

/// Biome and weight selected by TFG's block-resolution height filler for the
/// surface builder. This is deliberately different from the raw quart biome
/// shown by the atlas: TFG selects the dominant biome within the weighted
/// normal/shore/ocean partitions, then may replace it with the river biome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfaceBiome {
    /// Biome whose surface builder is invoked (may be the river overlay).
    pub biome: u16,
    /// Pre-river biome selected by the height filler.
    pub original_biome: u16,
    /// Weight passed to `SurfaceBuilderContext` for the selected biome.
    pub weight: f64,
}

/// Reproduce the biome choice passed to the TFG surface builder at one block
/// column. Supplying an existing height avoids evaluating the full terrain
/// height a second time on a cold pixel. Raw map biomes are not changed.
pub(crate) fn surface_biome_with_height(
    region: &crate::region::Generator,
    seed: i64,
    block_x: i32,
    block_z: i32,
    sampled_height: Option<f64>,
) -> Option<SurfaceBiome> {
    HEIGHT_SAMPLER.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|sampler| sampler.seed != seed || sampler.settings != region.settings)
        {
            *cache = Some(HeightSampler::new(seed, region.settings.clone()));
        }
        cache
            .as_mut()
            .expect("height sampler initialized")
            .surface_biome(region, block_x, block_z, sampled_height)
    })
}

/// TFG's bilinearly interpolated local surface slope used by surface builders.
/// Its six-by-six quart grid is cached per worldgen chunk; the 49 source heights
/// come from the same seeded, blended sampler used by `surface_height`.
pub(crate) fn surface_slope(
    region: &crate::region::Generator,
    seed: i64,
    block_x: i32,
    block_z: i32,
) -> Option<f64> {
    surface_context::slope(region, seed, block_x, block_z)
}

/// Per-column inputs retained before terrain density and river cave carving.
#[derive(Clone)]
pub(crate) struct ColumnDensityContext {
    pub biomes: Vec<(u16, f64)>,
    pub pre_river_height: f64,
    /// River builder selection tests the full height before ForgeCaps truncation.
    pub height_unrounded: f64,
    pub river_weights: [f64; MAX_RIVER_TYPES],
    pub initial_cave_weight: f64,
    pub river: Option<crate::rivers::RiverInfo>,
}
pub(crate) fn column_density_context(
    region: &crate::region::Generator,
    seed: i64,
    x: i32,
    z: i32,
) -> Option<ColumnDensityContext> {
    let cached = HEIGHT_SAMPLER.with(|cache| {
        let mut cache = cache.borrow_mut();
        let sampler = cache.as_mut()?;
        (sampler.seed == seed && sampler.settings == region.settings)
            .then(|| sampler.density_contexts.get(&(x, z)).cloned())
            .flatten()
    });
    if cached.is_some() {
        return cached;
    }
    // Populate outside the RefCell borrow; callbacks may otherwise re-enter it.
    surface_height(region, seed, x, z)?;
    HEIGHT_SAMPLER.with(|cache| {
        cache
            .borrow_mut()
            .as_mut()?
            .density_contexts
            .get(&(x, z))
            .cloned()
    })
}

/// The categorical river annotation only needs this scalar, not an allocated
/// copy of every blended biome. Populate a missing context outside its borrow.
pub(crate) fn river_cave_weight(
    region: &crate::region::Generator,
    seed: i64,
    x: i32,
    z: i32,
) -> Option<f64> {
    let read = || {
        HEIGHT_SAMPLER.with(|cache| {
            let mut cache = cache.borrow_mut();
            let sampler = cache.as_mut()?;
            if sampler.seed != seed || sampler.settings != region.settings {
                return None;
            }
            let context = sampler.density_contexts.get(&(x, z))?;
            Some(context.river.as_ref().map(|_| context.initial_cave_weight))
        })
    };
    if let Some(weight) = read() {
        return weight;
    }
    surface_height(region, seed, x, z)?;
    read().flatten()
}
pub(crate) fn aquifer_height(
    region: &crate::region::Generator,
    seed: i64,
    x: i32,
    z: i32,
) -> Option<f64> {
    #[derive(Deserialize)]
    struct Profile {
        aquifer_offset: Option<f64>,
        aquifer_fixed: Option<f64>,
    }
    static PROFILES: OnceLock<HashMap<String, Profile>> = OnceLock::new();
    let profiles = PROFILES
        .get_or_init(|| serde_json::from_str(include_str!("caves/assets/biomes.json")).unwrap());
    let id = region.biome_at_quart_no_river(x >> 2, z >> 2);
    let name = crate::biome::biome_name(id).rsplit('/').next()?;
    let p = profiles.get(name)?;
    if let Some(h) = p.aquifer_fixed {
        return Some(h);
    }
    surface_height(region, seed, x, z)?;
    HEIGHT_SAMPLER.with(|cache| {
        let mut cache = cache.borrow_mut();
        let sampler = cache.as_mut()?;
        let factory = sampler
            .factories
            .entry(id)
            .or_insert_with(|| height_noise::build_height(seed, name).ok().flatten())
            .as_ref()?;
        Some(factory(x as f64, z as f64) + p.aquifer_offset.unwrap_or(0.))
    })
}

thread_local! { static HEIGHT_SAMPLER: std::cell::RefCell<Option<HeightSampler>> = const { std::cell::RefCell::new(None) }; }

#[derive(Deserialize)]
struct HeightTable {
    biomes: HashMap<String, Option<HeightEntry>>,
}
#[derive(Deserialize)]
struct HeightEntry {
    expression: String,
}
static HEIGHT_EXPRESSIONS: OnceLock<HashMap<String, Option<String>>> = OnceLock::new();
fn height_expressions() -> &'static HashMap<String, Option<String>> {
    HEIGHT_EXPRESSIONS.get_or_init(|| {
        let data: HeightTable = serde_json::from_str(TFG_BIOME_HEIGHTS)
            .expect("bundled TFG biome height expressions must be valid");
        data.biomes
            .into_iter()
            .map(|(name, entry)| (name, entry.map(|e| e.expression)))
            .collect()
    })
}

struct HeightSampler {
    seed: i64,
    settings: crate::model::Settings,
    // A biome factory is made once when that biome first appears in a requested 7x7 blend.
    factories: HashMap<u16, Option<crate::noise::Noise2D>>,
    chunk_weights: LruCache<(i32, i32), ChunkBiomeWeights>,
    surface_biomes: LruCache<(i32, i32), SurfaceBiome>,
    river_samplers: river_height::RiverHeightSamplers,
    shore_samplers: shore_height::Samplers,
    volcanic_samplers: volcanic_height::Samplers,
    river_edges: HashMap<(i32, i32), Vec<crate::region::RiverEdge>>,
    density_contexts: LruCache<(i32, i32), ColumnDensityContext>,
}

impl HeightSampler {
    fn new(seed: i64, settings: crate::model::Settings) -> Self {
        Self {
            seed,
            settings,
            factories: HashMap::new(),
            chunk_weights: LruCache::new(NonZeroUsize::new(256).unwrap()),
            surface_biomes: LruCache::new(NonZeroUsize::new(16_384).unwrap()),
            river_samplers: river_height::RiverHeightSamplers::new(seed),
            shore_samplers: shore_height::Samplers::new(seed),
            volcanic_samplers: volcanic_height::Samplers::new(seed),
            river_edges: HashMap::new(),
            density_contexts: LruCache::new(NonZeroUsize::new(8192).unwrap()),
        }
    }

    fn sample(
        &mut self,
        region: &crate::region::Generator,
        block_x: i32,
        block_z: i32,
    ) -> Option<f64> {
        let x = block_x;
        let z = block_z;
        let chunk_x = x.div_euclid(16);
        let chunk_z = z.div_euclid(16);
        let local_x = x - chunk_x * 16;
        let local_z = z - chunk_z * 16;
        let weights = self.chunk_weights(region, chunk_x, chunk_z, local_x, local_z);
        let mut weighted_heights = [0.0; 109];
        for (biome, weight) in weights.iter().copied().enumerate() {
            if weight <= 0.0 {
                continue;
            }
            let factory = self.factories.entry(biome as u16).or_insert_with(|| {
                let name = crate::biome::ids::BIOME_IDS[biome]
                    .rsplit('/')
                    .next()
                    .unwrap_or("");
                match height_expressions().get(name) {
                    Some(Some(_expression)) => Some(
                        height_noise::build_height(self.seed, name)
                            .expect("every non-null TFG height expression needs an exact factory")
                            .expect("non-null TFG height expression produced no sampler"),
                    ),
                    _ => None,
                }
            });
            let noise = factory.as_ref()?;
            let biome_height = weight * noise(x as f64, z as f64);
            weighted_heights[biome] = biome_height;
        }
        // TFGChunkHeightFiller runs its shore blending and tide clamp before centered features,
        // then river carving. Preserve the upstream stage order.
        let mut height = self
            .shore_samplers
            .apply(region, x, z, &weights, &weighted_heights);
        height = self.volcanic_samplers.apply(region, x, z, height);
        let river_count = self.river_samplers.count();
        debug_assert!(river_count <= MAX_RIVER_TYPES);
        let mut river_weights = [0.0; MAX_RIVER_TYPES];
        for (biome, weight) in weights.iter().copied().enumerate() {
            if weight <= 0.0 {
                continue;
            }
            let name = crate::biome::ids::BIOME_IDS[biome]
                .rsplit('/')
                .next()
                .unwrap_or("");
            river_weights[self.river_samplers.type_for(name)] += weight;
        }
        let cave_index = self.river_samplers.cave_index();
        let none_index = self.river_samplers.none_index();
        let initial_cave_weight = river_weights[cave_index];
        if initial_cave_weight > 0.0 {
            let total = 1.0 - river_weights[none_index];
            let adjusted = if initial_cave_weight < 0.25 {
                map_range(initial_cave_weight, 0.0, 0.25, 0.0, 0.1 * total)
            } else {
                total
            };
            for value in &mut river_weights[..river_count] {
                *value = *value * (1.0 - adjusted) / (1.0 - initial_cave_weight);
            }
            river_weights[cave_index] = adjusted;
        }
        let key = (chunk_x, chunk_z);
        if let std::collections::hash_map::Entry::Vacant(e) = self.river_edges.entry(key) {
            let center_x = (chunk_x * 16 + 8) as f64 / 128.0;
            let center_z = (chunk_z * 16 + 8) as f64 / 128.0;
            let reach =
                (2500.0f64 / (128.0 * 128.0)).sqrt() + 8.0 * std::f64::consts::SQRT_2 / 128.0;
            let edges = region
                .river_edges_at(x, z)
                .into_iter()
                .filter(|edge| {
                    crate::rivers::river_maybe_intersects(edge, center_x, center_z, reach * reach)
                })
                .collect();
            e.insert(edges);
            if self.river_edges.len() > 2048
                && let Some(oldest) = self
                    .river_edges
                    .keys()
                    .find(|&&entry| entry != key)
                    .copied()
            {
                self.river_edges.remove(&oldest);
            }
        }
        let edges = self.river_edges.get(&key).expect("river partition cached");
        let Some(info) = crate::rivers::sample_river_edge(edges, x as f64, z as f64) else {
            self.density_contexts.put(
                (x, z),
                ColumnDensityContext {
                    biomes: weights
                        .iter()
                        .enumerate()
                        .filter(|(_, w)| **w > 0.)
                        .map(|(i, w)| (i as u16, *w))
                        .collect(),
                    pre_river_height: height,
                    height_unrounded: height,
                    river_weights: [0.; MAX_RIVER_TYPES],
                    initial_cave_weight: 0.,
                    river: None,
                },
            );
            return Some(height.trunc());
        };
        self.density_contexts.put(
            (x, z),
            ColumnDensityContext {
                biomes: weights
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| **w > 0.)
                    .map(|(i, w)| (i as u16, *w))
                    .collect(),
                pre_river_height: height,
                height_unrounded: height,
                river_weights,
                initial_cave_weight,
                river: Some(info.clone()),
            },
        );
        let mut carved = 0.0;
        for (kind, weight) in river_weights[..river_count].iter().copied().enumerate() {
            if weight > 0.0 {
                carved += weight
                    * self.river_samplers.sample(
                        kind,
                        &info,
                        x as f64,
                        z as f64,
                        height,
                        initial_cave_weight,
                        weight,
                    );
            }
        }
        if let Some(context) = self.density_contexts.get_mut(&(x, z)) {
            context.height_unrounded = carved;
        }
        Some(carved.trunc())
    }

    fn surface_biome(
        &mut self,
        region: &crate::region::Generator,
        block_x: i32,
        block_z: i32,
        sampled_height: Option<f64>,
    ) -> Option<SurfaceBiome> {
        let key = (block_x, block_z);
        if let Some(cached) = self.surface_biomes.get(&key).copied() {
            return Some(cached);
        }
        let chunk_x = block_x.div_euclid(16);
        let chunk_z = block_z.div_euclid(16);
        let local_x = block_x.rem_euclid(16);
        let local_z = block_z.rem_euclid(16);
        let weights = self.chunk_weights(region, chunk_x, chunk_z, local_x, local_z);
        let selection = surface_context::select_biome(&weights);

        let mut biome = selection.biome;
        // Match TFGChunkNoiseFiller.updateLocalCaches: the river display biome
        // is used only after its height and normalized-distance gates pass.
        let height = sampled_height.or_else(|| self.sample(region, block_x, block_z))?;
        // The stored height is truncated, but TFG checks the unrounded height
        // here. A column at 64.2 must retain its land builder, unlike one at 64.
        let selection_height = if height == 64. {
            if self.density_contexts.peek(&(block_x, block_z)).is_none() {
                self.sample(region, block_x, block_z)?;
            }
            self.density_contexts
                .peek(&(block_x, block_z))?
                .height_unrounded
        } else {
            height
        };
        if selection_height <= 64.0
            && selection.has_rivers
            && self
                .river_info(region, chunk_x, chunk_z, block_x, block_z)
                .is_some_and(|info| info.dist_sq_blocks / info.width_sq < 1.1)
        {
            biome = crate::biome::ids::id::RIVER;
        }
        let context = SurfaceBiome {
            biome,
            original_biome: selection.biome,
            weight: if biome == crate::biome::ids::id::RIVER {
                0.5
            } else {
                weights[biome as usize]
            },
        };
        self.surface_biomes.put(key, context);
        Some(context)
    }

    fn river_info(
        &mut self,
        region: &crate::region::Generator,
        chunk_x: i32,
        chunk_z: i32,
        block_x: i32,
        block_z: i32,
    ) -> Option<crate::rivers::RiverInfo> {
        let key = (chunk_x, chunk_z);
        if let std::collections::hash_map::Entry::Vacant(e) = self.river_edges.entry(key) {
            let center_x = (chunk_x * 16 + 8) as f64 / 128.0;
            let center_z = (chunk_z * 16 + 8) as f64 / 128.0;
            let reach =
                (2500.0f64 / (128.0 * 128.0)).sqrt() + 8.0 * std::f64::consts::SQRT_2 / 128.0;
            let edges = region
                .river_edges_at(block_x, block_z)
                .into_iter()
                .filter(|edge| {
                    crate::rivers::river_maybe_intersects(edge, center_x, center_z, reach * reach)
                })
                .collect();
            e.insert(edges);
            if self.river_edges.len() > 2048
                && let Some(oldest) = self
                    .river_edges
                    .keys()
                    .find(|&&entry| entry != key)
                    .copied()
            {
                self.river_edges.remove(&oldest);
            }
        }
        crate::rivers::sample_river_edge(
            self.river_edges.get(&key).expect("river partition cached"),
            block_x as f64,
            block_z as f64,
        )
    }

    fn chunk_weights(
        &mut self,
        region: &crate::region::Generator,
        chunk_x: i32,
        chunk_z: i32,
        local_x: i32,
        local_z: i32,
    ) -> [f64; 109] {
        let key = (chunk_x, chunk_z);
        if self.chunk_weights.get(&key).is_none() {
            self.chunk_weights
                .put(key, ChunkBiomeWeights::new(chunk_x, chunk_z));
        }
        self.chunk_weights
            .get_mut(&key)
            .expect("chunk weights inserted")
            .sample_column(region, local_x, local_z)
    }
}

const MAX_RIVER_TYPES: usize = 16;

/// Biome blending is sampled on demand per terrain chunk. A distant map zoom often asks
/// for one height from a chunk; eagerly filling the original 45×45 quart sample and all
/// 65 corner kernels did work for roughly 2000 biome cells that result could not use.
struct ChunkBiomeWeights {
    chunk_x: i32,
    chunk_z: i32,
    sampled: Box<[Option<u16>; 45 * 45]>,
    broad: Box<[Option<[f64; 109]>; 16]>,
    quart: Box<[Option<[f64; 109]>; 49]>,
}

impl ChunkBiomeWeights {
    fn new(chunk_x: i32, chunk_z: i32) -> Self {
        Self {
            chunk_x,
            chunk_z,
            sampled: Box::new([None; 45 * 45]),
            broad: Box::new([None; 16]),
            quart: Box::new([None; 49]),
        }
    }

    fn sample_column(
        &mut self,
        region: &crate::region::Generator,
        local_x: i32,
        local_z: i32,
    ) -> [f64; 109] {
        let ix = local_x.div_euclid(4) + 1;
        let iz = local_z.div_euclid(4) + 1;
        let lx = (local_x - local_x.div_euclid(4) * 4) as f64 / 4.0;
        let lz = (local_z - local_z.div_euclid(4) * 4) as f64 / 4.0;
        // Keep the source code's four additions in order. Most distant samples
        // land on a quart corner, so skip kernels with a zero interpolation weight.
        let scales = [
            (1.0 - lx) * (1.0 - lz),
            lx * (1.0 - lz),
            (1.0 - lx) * lz,
            lx * lz,
        ];
        let corners = [
            (scales[0] > 0.0).then(|| self.corner(region, ix, iz)),
            (scales[1] > 0.0).then(|| self.corner(region, ix + 1, iz)),
            (scales[2] > 0.0).then(|| self.corner(region, ix, iz + 1)),
            (scales[3] > 0.0).then(|| self.corner(region, ix + 1, iz + 1)),
        ];
        let mut out = [0.0; 109];
        for index in 0..4 {
            if let Some(corner) = &corners[index] {
                add_scaled(&mut out, corner, scales[index]);
            }
        }
        out
    }

    fn corner(&mut self, region: &crate::region::Generator, qx: i32, qz: i32) -> [f64; 109] {
        let index = (qx + 7 * qz) as usize;
        if let Some(cached) = self.quart[index] {
            return cached;
        }
        let origin_x = self.chunk_x * 16;
        let origin_z = self.chunk_z * 16;
        let mut local = [0.0; 109];
        self.kernel(region, &mut local, 2, qx - 1, qz - 1);

        let x1 = origin_x + (qx - 1) * 4;
        let z1 = origin_z + (qz - 1) * 4;
        let cx = x1.div_euclid(16);
        let cz = z1.div_euclid(16);
        let lx = (x1 - cx * 16) as f64 / 16.0;
        let lz = (z1 - cz * 16) as f64 / 16.0;
        let ix = (x1 - origin_x).div_euclid(16) + 1;
        let iz = (z1 - origin_z).div_euclid(16) + 1;
        let scales = [
            (1.0 - lx) * (1.0 - lz),
            lx * (1.0 - lz),
            (1.0 - lx) * lz,
            lx * lz,
        ];
        let broad_corners = [
            (scales[0] > 0.0).then(|| self.broad_corner(region, ix, iz)),
            (scales[1] > 0.0).then(|| self.broad_corner(region, ix + 1, iz)),
            (scales[2] > 0.0).then(|| self.broad_corner(region, ix, iz + 1)),
            (scales[3] > 0.0).then(|| self.broad_corner(region, ix + 1, iz + 1)),
        ];
        let mut broad_blend = [0.0; 109];
        for index in 0..4 {
            if let Some(corner) = &broad_corners[index] {
                add_scaled(&mut broad_blend, corner, scales[index]);
            }
        }
        compose(&mut local, &broad_blend);
        self.quart[index] = Some(local);
        local
    }

    fn broad_corner(&mut self, region: &crate::region::Generator, bx: i32, bz: i32) -> [f64; 109] {
        let index = (bx | (bz << 2)) as usize;
        if let Some(cached) = self.broad[index] {
            return cached;
        }
        let mut out = [0.0; 109];
        self.kernel(region, &mut out, 4, bx - 1, bz - 1);
        self.broad[index] = Some(out);
        out
    }

    fn kernel(
        &mut self,
        region: &crate::region::Generator,
        out: &mut [f64; 109],
        bits: u32,
        x_offset: i32,
        z_offset: i32,
    ) {
        let scale = 1 << bits;
        let origin_x = self.chunk_x * 16;
        let origin_z = self.chunk_z * 16;
        for dx in -KERNEL_RADIUS..=KERNEL_RADIUS {
            for dz in -KERNEL_RADIUS..=KERNEL_RADIUS {
                let x = origin_x + (x_offset + dx) * scale;
                let z = origin_z + (z_offset + dz) * scale;
                let sx = ((x - origin_x + 80).div_euclid(4)) as usize;
                let sz = ((z - origin_z + 80).div_euclid(4)) as usize;
                let sample_index = sx * 45 + sz;
                let biome = *self.sampled[sample_index].get_or_insert_with(|| {
                    region.biome_at_quart_no_river(x.div_euclid(4), z.div_euclid(4))
                }) as usize;
                if biome < 109 {
                    out[biome] += KERNEL[((dx + 4) + (dz + 4) * 9) as usize];
                }
            }
        }
    }
}

const KERNEL_RADIUS: i32 = 4;
const KERNEL: [f64; 81] = {
    let mut values = [0.0; 81];
    let mut i = 0;
    while i < 81 {
        let x = (i % 9) as i32 - 4;
        let z = (i / 9) as i32 - 4;
        values[i] = 0.0211640211641 * (1.0 - 0.03125 * (z * z + x * x) as f64);
        i += 1;
    }
    values
};

#[cfg(test)]
fn sample_chunk_biomes_reference(
    region: &crate::region::Generator,
    chunk_x: i32,
    chunk_z: i32,
) -> Vec<[f64; 109]> {
    let origin_x = chunk_x * 16;
    let origin_z = chunk_z * 16;
    // Unique quart samples from [-80, 96] blocks around the chunk origin.
    let mut sampled = [0u16; 45 * 45];
    for sx in 0..45 {
        for sz in 0..45 {
            sampled[sx * 45 + sz] = region.biome_at_quart_no_river(
                origin_x.div_euclid(4) - 20 + sx as i32,
                origin_z.div_euclid(4) - 20 + sz as i32,
            );
        }
    }
    let mut broad = vec![[0.0; 109]; 16];
    for bx in 0..4 {
        for bz in 0..4 {
            let mut w = [0.0; 109];
            sample_kernel(
                &sampled,
                &mut w,
                origin_x,
                origin_z,
                4,
                bx as i32 - 1,
                bz as i32 - 1,
            );
            broad[bx | (bz << 2)] = w;
        }
    }
    let mut quart = vec![[0.0; 109]; 49];
    for qx in 0..7 {
        for qz in 0..7 {
            let mut local = [0.0; 109];
            sample_kernel(
                &sampled,
                &mut local,
                origin_x,
                origin_z,
                2,
                qx as i32 - 1,
                qz as i32 - 1,
            );
            let x1 = origin_x + (qx as i32 - 1) * 4;
            let z1 = origin_z + (qz as i32 - 1) * 4;
            let cx = x1.div_euclid(16);
            let cz = z1.div_euclid(16);
            let lx = (x1 - cx * 16) as f64 / 16.0;
            let lz = (z1 - cz * 16) as f64 / 16.0;
            let ix = (x1 - origin_x).div_euclid(16) + 1;
            let iz = (z1 - origin_z).div_euclid(16) + 1;
            let mut broad_blend = [0.0; 109];
            add_scaled(
                &mut broad_blend,
                &broad[(ix | (iz << 2)) as usize],
                (1.0 - lx) * (1.0 - lz),
            );
            add_scaled(
                &mut broad_blend,
                &broad[((ix + 1) | (iz << 2)) as usize],
                lx * (1.0 - lz),
            );
            add_scaled(
                &mut broad_blend,
                &broad[(ix | ((iz + 1) << 2)) as usize],
                (1.0 - lx) * lz,
            );
            add_scaled(
                &mut broad_blend,
                &broad[((ix + 1) | ((iz + 1) << 2)) as usize],
                lx * lz,
            );
            compose(&mut local, &broad_blend);
            quart[qx + 7 * qz] = local;
        }
    }
    quart
}

#[cfg(test)]
fn sample_kernel(
    sampled: &[u16; 45 * 45],
    out: &mut [f64; 109],
    origin_x: i32,
    origin_z: i32,
    bits: u32,
    x_offset: i32,
    z_offset: i32,
) {
    let scale = 1 << bits;
    for dx in -KERNEL_RADIUS..=KERNEL_RADIUS {
        for dz in -KERNEL_RADIUS..=KERNEL_RADIUS {
            let x = origin_x + (x_offset + dx) * scale;
            let z = origin_z + (z_offset + dz) * scale;
            let sx = ((x - origin_x + 80).div_euclid(4)) as usize;
            let sz = ((z - origin_z + 80).div_euclid(4)) as usize;
            let biome = sampled[sx * 45 + sz] as usize;
            if biome < 109 {
                out[biome] += KERNEL[((dx + 4) + (dz + 4) * 9) as usize];
            }
        }
    }
}
// TFG expands and reorders the biome palette, so resolve each ordinal from the pinned
// explicit BiomeBlendType table instead of relying on the old TFC ordinal boundaries.
fn blend_group(id: usize) -> usize {
    blend_groups().get(id).copied().unwrap_or(0) as usize
}

fn blend_groups() -> &'static [u8; 109] {
    static GROUPS: OnceLock<[u8; 109]> = OnceLock::new();
    GROUPS.get_or_init(|| {
        #[derive(Deserialize)]
        struct Table {
            biomes: HashMap<String, String>,
        }
        let table: Table = serde_json::from_str(TFG_BIOME_BLEND_TYPES)
            .expect("bundled TFG biome blend types must be valid");
        std::array::from_fn(|id| {
            let name = crate::biome::ids::BIOME_IDS[id]
                .rsplit('/')
                .next()
                .unwrap_or("");
            match table.biomes.get(name).map(String::as_str) {
                Some("LAND") => 0,
                Some("OCEAN") => 1,
                Some("LAKE") => 2,
                other => panic!("missing or invalid TFG biome blend type for {name}: {other:?}"),
            }
        })
    })
}
fn add_scaled(target: &mut [f64; 109], source: &[f64; 109], scale: f64) {
    if scale > 0.0 {
        for i in 0..109 {
            target[i] += source[i] * scale;
        }
    }
}
fn compose(local: &mut [f64; 109], broad: &[f64; 109]) {
    let mut bt = [0.0; 3];
    let mut lt = [0.0; 3];
    for i in 0..109 {
        let g = blend_group(i);
        bt[g] += broad[i];
        lt[g] += local[i];
    }
    let old = *broad;
    *local = [0.0; 109];
    for i in 0..109 {
        let g = blend_group(i);
        if lt[g] > 0.0 && bt[g] > 0.0 {
            local[i] = old[i] * lt[g] / bt[g];
        }
    }
}
#[cfg(test)]
fn sample_column(corners: &[[f64; 109]], local_x: i32, local_z: i32) -> [f64; 109] {
    let ix = local_x.div_euclid(4) + 1;
    let iz = local_z.div_euclid(4) + 1;
    let lx = (local_x - local_x.div_euclid(4) * 4) as f64 / 4.0;
    let lz = (local_z - local_z.div_euclid(4) * 4) as f64 / 4.0;
    let mut out = [0.0; 109];
    add_scaled(
        &mut out,
        &corners[(ix + iz * 7) as usize],
        (1.0 - lx) * (1.0 - lz),
    );
    add_scaled(
        &mut out,
        &corners[(ix + 1 + iz * 7) as usize],
        lx * (1.0 - lz),
    );
    add_scaled(
        &mut out,
        &corners[(ix + (iz + 1) * 7) as usize],
        (1.0 - lx) * lz,
    );
    add_scaled(
        &mut out,
        &corners[(ix + 1 + (iz + 1) * 7) as usize],
        lx * lz,
    );
    out
}

#[inline]
fn map_range(value: f64, old_min: f64, old_max: f64, new_min: f64, new_max: f64) -> f64 {
    new_min + ((value - old_min) / (old_max - old_min)) * (new_max - new_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_chunk_biome_weights_match_full_reference_bit_for_bit() {
        for (seed, chunk_x, chunk_z) in [
            (0, 0, 0),
            (0, -1, 0),
            (0, -128, -256),
            (1, 127, -129),
            (-918273645, -2, 3),
        ] {
            let region = crate::region::Generator::new(seed, crate::model::Settings::default());
            let full = sample_chunk_biomes_reference(&region, chunk_x, chunk_z);
            let mut lazy = ChunkBiomeWeights::new(chunk_x, chunk_z);
            for local_z in 0..16 {
                for local_x in 0..16 {
                    let expected = sample_column(&full, local_x, local_z);
                    let actual = lazy.sample_column(&region, local_x, local_z);
                    for (index, (expected, actual)) in
                        expected.iter().zip(actual.iter()).enumerate()
                    {
                        assert_eq!(
                            expected.to_bits(),
                            actual.to_bits(),
                            "seed={seed} chunk={chunk_x},{chunk_z} local={local_x},{local_z} biome={index}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn all_registered_biome_blend_groups_match_the_pinned_tfg_source() {
        let mut counts = [0usize; 3];
        for id in 0..109 {
            counts[blend_group(id)] += 1;
        }
        assert_eq!(counts, [95, 5, 9]);
        for (name, expected) in [
            ("deep_ocean_trench", 1),
            ("tidal_flats", 1),
            ("shore", 0),
            ("sea_stacks", 0),
            ("canyons", 0),
            ("lake", 2),
            ("mountain_lake", 2),
            ("subglacial_lake", 2),
            ("meltwater_lake", 2),
            ("tuyas", 0),
            ("stone_circles", 0),
        ] {
            let id = crate::biome::ids::BIOME_IDS
                .iter()
                .position(|id| id.ends_with(name))
                .unwrap();
            assert_eq!(blend_group(id), expected, "TFGBiomes type for {name}");
        }
    }

    #[test]
    fn surface_height_cache_is_keyed_by_seed_and_all_region_settings() {
        let settings_a = crate::model::Settings::default();
        let mut settings_b = settings_a.clone();
        settings_b.continentalness = 0.88;
        settings_b.temperature_scale = 14_000.0;
        settings_b.rainfall_scale = 17_000.0;
        let region_a = crate::region::Generator::new(0, settings_a.clone());
        let region_b = crate::region::Generator::new(0, settings_b.clone());
        let mut fresh_a = HeightSampler::new(0, settings_a);
        let mut fresh_b = HeightSampler::new(0, settings_b);
        let coords = [
            (-4096, -4096),
            (-3072, -1024),
            (-2048, 2048),
            (-1024, 4096),
            (0, 0),
            (1024, -3072),
            (2048, 1024),
            (3072, 4096),
            (4096, -2048),
        ];
        let mut outputs_differ = false;
        for (x, z) in coords {
            let expected_a = fresh_a.sample(&region_a, x, z);
            let expected_b = fresh_b.sample(&region_b, x, z);
            outputs_differ |= expected_a != expected_b;

            let got_a = surface_height(&region_a, 0, x, z);
            let got_b = surface_height(&region_b, 0, x, z);
            let returned_a = surface_height(&region_a, 0, x, z);
            assert_eq!(got_a, expected_a, "settings A at ({x}, {z})");
            assert_eq!(got_b, expected_b, "settings B at ({x}, {z})");
            assert_eq!(returned_a, expected_a, "A after B at ({x}, {z})");
        }
        assert!(
            outputs_differ,
            "chosen settings and coordinates must exercise terrain changes"
        );
    }

    #[test]
    fn cached_density_context_and_river_annotation_follow_world_identity() {
        let (x, z) = (10880, -16000);
        for seed in [0, 1, 0] {
            let settings = crate::model::Settings::default();
            let mut varied = settings.clone();
            varied.continentalness = 0.88;
            varied.temperature_scale = 14_000.;
            varied.rainfall_scale = 17_000.;
            for settings in [settings, varied] {
                let region = crate::region::Generator::new(seed, settings.clone());
                let mut fresh = HeightSampler::new(seed, settings);
                fresh.sample(&region, x, z).unwrap();
                let expected = fresh.density_contexts.get(&(x, z)).unwrap();
                for _ in 0..2 {
                    let actual = column_density_context(&region, seed, x, z).unwrap();
                    assert_eq!(actual.biomes, expected.biomes);
                    assert_eq!(actual.river_weights, expected.river_weights);
                    assert_eq!(actual.pre_river_height, expected.pre_river_height);
                    assert_eq!(actual.height_unrounded, expected.height_unrounded);
                    assert_eq!(actual.initial_cave_weight, expected.initial_cave_weight);
                    assert_eq!(
                        river_cave_weight(&region, seed, x, z),
                        expected
                            .river
                            .as_ref()
                            .map(|_| expected.initial_cave_weight)
                    );
                }
            }
        }
    }

    #[test]
    fn bundled_rock_settings_are_ordered_and_reach_bottom() {
        for category in 0..4 {
            let packed = 0x1234_5678_i32 & !3 | category;
            let stack = rock_stack(packed);
            assert!(
                stack
                    .iter()
                    .all(|rock| !rock.is_empty() && rock != "unknown")
            );
            assert!(!sample_rock_layer(packed, bottom_depth_for(packed)).is_empty());
        }
        assert_eq!(
            [
                bottom_depth_for(0),
                bottom_depth_for(1),
                bottom_depth_for(2),
                bottom_depth_for(3)
            ],
            [2, 3, 3, 4]
        );
        // Captured from the upstream TFG settings with the same packed point value. The generic
        // TFC fixture uses a different rock table and therefore has different expected lithology.
        assert_eq!(sample_rock_layer(-863_411_540, 0), "basalt");
        assert_eq!(sample_rock_layer(-863_411_540, 1), "gabbro");
        assert_eq!(sample_rock_layer(-863_411_540, 2), "diorite");
        assert_eq!(sample_rock_layer(-863_411_540, 3), "gabbro");
        assert_eq!(sample_rock_layer(-863_411_540, 4), "schist");
    }
    #[test]
    fn trees_use_installed_climate_entries_and_forest_attempts() {
        let mut probe = crate::model::Probe::default();
        probe.climate.temperature = 12.0;
        probe.climate.rainfall = 300.0;
        probe.forest = 3;
        let estimate = estimate_trees(&probe, 25_600.0);
        assert!((estimate.total - 525.0).abs() < 0.1);
        assert!(estimate.per_hectare > 0.0);
        assert!(!estimate.species.is_empty());
        probe.forest = 0;
        assert_eq!(estimate_trees(&probe, 25_600.0).total, 0.0);
    }
}
