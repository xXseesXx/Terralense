//! TFC/TFG rock-layer settings. The layer tree and choice order come from the installed
//! TerraFirmaGreg rock settings; `point_rock` is the packed Region.Point.rock value.
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[path = "geology/height_noise.rs"]
mod height_noise;
#[path = "geology/river_height.rs"]
mod river_height;

const TFG_ROCK_LAYERS: &str = include_str!("../assets/data/tfg/rock-layers.json");
const TFG_BIOME_HEIGHTS: &str = include_str!("../assets/data/tfg/biome-heights.json");

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
        if cache.as_ref().is_none_or(|sampler| sampler.seed != seed) {
            *cache = Some(HeightSampler::new(seed));
        }
        cache
            .as_mut()
            .expect("height sampler initialized")
            .sample(region, block_x, block_z)
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
    // A biome factory is made once when that biome first appears in a requested 7x7 blend.
    factories: HashMap<u16, Option<crate::noise::Noise2D>>,
    chunk_weights: HashMap<(i32, i32), Vec<[f64; 109]>>,
    river_samplers: river_height::RiverHeightSamplers,
    river_edges: HashMap<(i32, i32), Vec<crate::region::RiverEdge>>,
}

impl HeightSampler {
    fn new(seed: i64) -> Self {
        Self {
            seed,
            factories: HashMap::new(),
            chunk_weights: HashMap::new(),
            river_samplers: river_height::RiverHeightSamplers::new(seed),
            river_edges: HashMap::new(),
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
        let weights = sample_column(
            self.chunk_weights(region, chunk_x, chunk_z),
            local_x,
            local_z,
        );
        let mut height = 0.0;
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
            height += weight * noise(x as f64, z as f64);
        }
        let mut river_weights = vec![0.0; self.river_samplers.count()];
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
            for value in &mut river_weights {
                *value = *value * (1.0 - adjusted) / (1.0 - initial_cave_weight);
            }
            river_weights[cave_index] = adjusted;
        }
        let key = (chunk_x, chunk_z);
        if !self.river_edges.contains_key(&key) {
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
            self.river_edges.insert(key, edges);
            if self.river_edges.len() > 2048 {
                if let Some(oldest) = self
                    .river_edges
                    .keys()
                    .find(|&&entry| entry != key)
                    .copied()
                {
                    self.river_edges.remove(&oldest);
                }
            }
        }
        let edges = self.river_edges.get(&key).expect("river partition cached");
        let Some(info) = crate::rivers::sample_river_edge(edges, x as f64, z as f64) else {
            return Some(height.trunc());
        };
        let mut carved = 0.0;
        for (kind, weight) in river_weights.into_iter().enumerate() {
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
        Some(carved.trunc())
    }

    fn chunk_weights(
        &mut self,
        region: &crate::region::Generator,
        chunk_x: i32,
        chunk_z: i32,
    ) -> &[[f64; 109]] {
        let key = (chunk_x, chunk_z);
        if !self.chunk_weights.contains_key(&key) {
            let weights = sample_chunk_biomes(region, chunk_x, chunk_z);
            self.chunk_weights.insert(key, weights);
            if self.chunk_weights.len() > 256 {
                if let Some(oldest) = self
                    .chunk_weights
                    .keys()
                    .find(|&&entry| entry != key)
                    .copied()
                {
                    self.chunk_weights.remove(&oldest);
                }
            }
        }
        self.chunk_weights
            .get(&key)
            .expect("chunk weights inserted")
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

fn sample_chunk_biomes(
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
// The original shared TFC blender uses its own numeric registration groups even for
// TFG's expanded palette. Preserve those boundaries to keep conversion parity.
fn blend_group(id: usize) -> usize {
    if id <= 3 || id == 21 {
        1
    } else if id >= 22 && id != 23 {
        2
    } else {
        0
    }
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
