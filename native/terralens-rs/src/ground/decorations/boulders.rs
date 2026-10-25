//! TFC's source-driven boulder and small-boulder feature projection.
//!
//! Shape/configuration data is extracted from the installed TFG 0.9.25 TFC
//! datapack (see `assets/boulders.json`). The surface height is TerraLens'
//! terrain estimate, so this predicts placement and shape without loading
//! chunks or reading saved world data.

use super::BoulderColumn;
use crate::{
    dimension::Dimension,
    random::Xoroshiro,
    vegetation::{self, FeatureRandom},
    world::World,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, OnceLock},
};

const DATA: &str = include_str!("assets/boulders.json");
const DECORATION_STEP: i32 = 8;
const ORIGIN_Y: i32 = -64;

#[derive(Clone)]
struct Feature {
    id: String,
    chance: i32,
    kind: Kind,
    states: HashMap<String, Vec<String>>,
}

#[derive(Clone, Copy)]
enum Kind {
    Large,
    Patch,
}

fn features() -> &'static [Feature] {
    static FEATURES: OnceLock<Vec<Feature>> = OnceLock::new();
    FEATURES.get_or_init(|| {
        let data: Value = serde_json::from_str(DATA).expect("bundled boulder data is valid");
        data["features"]
            .as_array()
            .expect("boulder features array")
            .iter()
            .map(|f| {
                let states = f["states"]
                    .as_object()
                    .expect("boulder state map")
                    .iter()
                    .map(|(rock, blocks)| {
                        (
                            rock.clone(),
                            blocks
                                .as_array()
                                .expect("state array")
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect(),
                        )
                    })
                    .collect();
                let id = f["id"].as_str().expect("feature id").to_owned();
                let style = f["style"].as_str().expect("feature style");
                let kind = if style.ends_with("_small") {
                    Kind::Patch
                } else {
                    Kind::Large
                };
                Feature {
                    id,
                    chance: f["chance"].as_i64().unwrap() as i32,
                    kind,
                    states,
                }
            })
            .collect()
    })
}

/// Predict source-chunk boulders. A returned column is the highest voxel of one
/// placed feature at that X/Z; duplicate columns retain the block from the
/// highest voxel. `index` is the source FeatureSorter index for later ordering.
pub(super) fn generate(
    world: &World,
    cx: i32,
    cz: i32,
    cancel: &impl Fn() -> bool,
) -> Option<Vec<BoulderColumn>> {
    if world.dimension != Dimension::Overworld {
        return Some(Vec::new());
    }
    let mut output = Vec::new();
    let mut ordered = features().iter().collect::<Vec<_>>();
    ordered.sort_by_key(|f| {
        vegetation::feature_index_in_step(&f.id, DECORATION_STEP).unwrap_or(u32::MAX)
    });
    for feature in ordered {
        if cancel() {
            return None;
        }
        let Some(index) = vegetation::feature_index_in_step(&feature.id, DECORATION_STEP) else {
            continue;
        };
        if !vegetation::feature_biome_allowed(world, cx, cz, &feature.id) {
            continue;
        }
        let mut random = FeatureRandom::new(vegetation::feature_seed(
            cx,
            cz,
            ORIGIN_Y,
            DECORATION_STEP,
            index,
        ));
        if random.next_float() >= 1.0_f32 / feature.chance as f32 {
            continue;
        }
        // The placed-feature in-square modifier runs before configured-feature RNG.
        let (ox, oz) = in_square_origin(&mut random, cx, cz);
        match feature.kind {
            Kind::Large => {
                let Some(sample) = crate::ground::base_sample(world, ox, oz) else {
                    continue;
                };
                let origin_y = sample.y.saturating_add(1); // OCEAN_FLOOR_WG is first air.
                let Some(y) = flat_origin(world, ox, oz, origin_y, 0.4) else {
                    continue;
                };
                let rock = world.rock_at(ox, y, oz);
                let Some(states) = rock.and_then(|r| {
                    feature
                        .states
                        .get(r.rsplit('/').next().unwrap_or(r.as_str()))
                        .cloned()
                }) else {
                    continue;
                };
                emit_large(&mut output, index, ox, y, oz, states, &mut random, cancel)?;
            }
            Kind::Patch => {
                // Minecraft's RandomPatchFeature tries six triangularly-spread
                // offsets. The nested small-boulder placed feature consumes dy,
                // then its OCEAN_FLOOR_WG modifier resets Y and FlatEnough lowers it.
                for _ in 0..6 {
                    if cancel() {
                        return None;
                    }
                    let dx = spread(&mut random, 5);
                    let _dy = spread(&mut random, 1);
                    let dz = spread(&mut random, 5);
                    let x = ox.wrapping_add(dx);
                    let z = oz.wrapping_add(dz);
                    let Some(sample) = crate::ground::base_sample(world, x, z) else {
                        continue;
                    };
                    let origin_y = sample.y.saturating_add(1);
                    let Some(y) = flat_origin(world, x, z, origin_y, 0.2) else {
                        continue;
                    };
                    let rock = world.rock_at(x, y, z);
                    let Some(states) = rock.and_then(|r| {
                        feature
                            .states
                            .get(r.rsplit('/').next().unwrap_or(r.as_str()))
                            .cloned()
                    }) else {
                        continue;
                    };
                    emit_baby(&mut output, index, x, y, z, states, &mut random)?;
                }
            }
        }
    }
    Some(output)
}

fn in_square_origin(random: &mut FeatureRandom, cx: i32, cz: i32) -> (i32, i32) {
    (
        cx.wrapping_mul(16).wrapping_add(random.next_int_bound(16)),
        cz.wrapping_mul(16).wrapping_add(random.next_int_bound(16)),
    )
}

fn spread(random: &mut FeatureRandom, spread: i32) -> i32 {
    random.next_int_bound(spread + 1) - random.next_int_bound(spread + 1)
}

/// FlatEnoughPlacement checks the 5x5 footprint at the first valid height,
/// scanning downward no more than four blocks. With the column sampler, a
/// sampled surface at or above the tested Y represents a solid, dry voxel.
fn flat_origin(world: &World, x: i32, z: i32, y: i32, flatness: f32) -> Option<i32> {
    for down in 0..4 {
        let test_y = y - down;
        let mut count = 0;
        for dz in -2..=2 {
            for dx in -2..=2 {
                if let Some(sample) = crate::ground::base_sample(world, x + dx, z + dz)
                    && sample.y >= test_y
                {
                    count += 1;
                }
            }
        }
        if count as f32 / 25.0 > flatness {
            return Some(test_y);
        }
    }
    None
}

fn emit_large(
    out: &mut Vec<BoulderColumn>,
    index: u32,
    x: i32,
    y: i32,
    z: i32,
    states: Vec<String>,
    random: &mut FeatureRandom,
    cancel: &impl Fn() -> bool,
) -> Option<()> {
    let size = 6 + random.next_int_bound(4);
    let lo = -0.12_f32 * size as f32;
    let hi = 0.3_f32 * size as f32;
    let radius = hi;
    // Helpers.fork consumes two WorldgenRandom.nextLong() values, then the
    // resulting XoroshiroRandomSource uses source (Lemire/double) methods.
    let fork = Xoroshiro::from_state(random.next_long() as u64, random.next_long() as u64);
    let mut noise = Metaballs::new(fork, lo as f64, hi as f64, radius as f64);
    let mut columns: BTreeMap<(i32, i32), BoulderColumn> = BTreeMap::new();
    for dx in -size..=size {
        for dy in -size..=size {
            for dz in -size..=size {
                if (dx & 3 == 0) && (dy & 3 == 0) && (dz & 3 == 0) && cancel() {
                    return None;
                }
                if noise.inside(dx as f64, dy as f64, dz as f64) {
                    let state = select_state(&states, random);
                    let (bx, by, bz) = (x + dx, y + dy, z + dz);
                    let column = BoulderColumn {
                        index,
                        x: bx,
                        y: by,
                        z: bz,
                        block: Arc::from(state),
                    };
                    columns
                        .entry((bx, bz))
                        .and_modify(|old| {
                            if by > old.y {
                                *old = column.clone();
                            }
                        })
                        .or_insert(column);
                }
            }
        }
    }
    out.extend(columns.into_values());
    Some(())
}

fn emit_baby(
    out: &mut Vec<BoulderColumn>,
    index: u32,
    x: i32,
    y: i32,
    z: i32,
    states: Vec<String>,
    random: &mut FeatureRandom,
) -> Option<()> {
    let mut columns: BTreeMap<(i32, i32), BoulderColumn> = BTreeMap::new();
    if random.next_float() < 0.5 {
        emit_baby_block(&mut columns, index, x, y, z, &states, random);
        // Direction::values(): DOWN, UP, NORTH, SOUTH, WEST, EAST.
        for (dx, dy, dz) in [
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
            (-1, 0, 0),
            (1, 0, 0),
        ] {
            if random.next_float() < 0.8 {
                emit_baby_block(&mut columns, index, x + dx, y + dy, z + dz, &states, random);
            }
        }
    } else if random.next_float() < 0.5 {
        let height = 4 + random.next_int_bound(3);
        for i in 0..height {
            emit_baby_block(&mut columns, index, x, y + i - 2, z, &states, random);
        }
        // Plane.HORIZONTAL order is NORTH, EAST, SOUTH, WEST.
        let (dx, dz) = match random.next_int_bound(4) {
            0 => (0, -1),
            1 => (1, 0),
            2 => (0, 1),
            _ => (-1, 0),
        };
        for i in 0..height - 2 {
            emit_baby_block(
                &mut columns,
                index,
                x + dx,
                y + i - 2,
                z + dz,
                &states,
                random,
            );
        }
    } else {
        emit_baby_block(&mut columns, index, x, y, z, &states, random);
        emit_baby_block(&mut columns, index, x, y - 1, z, &states, random);
    }
    out.extend(columns.into_values());
    Some(())
}

fn emit_baby_block(
    columns: &mut BTreeMap<(i32, i32), BoulderColumn>,
    index: u32,
    x: i32,
    y: i32,
    z: i32,
    states: &[String],
    random: &mut FeatureRandom,
) {
    let column = BoulderColumn {
        index,
        x,
        y,
        z,
        block: Arc::from(select_state(states, random)),
    };
    columns
        .entry((x, z))
        .and_modify(|old| {
            if y > old.y {
                *old = column.clone();
            }
        })
        .or_insert(column);
}

fn select_state(states: &[String], random: &mut FeatureRandom) -> String {
    if states.len() <= 1 {
        states.first().cloned().unwrap_or_default()
    } else {
        states[random.next_int_bound(states.len() as i32) as usize].clone()
    }
}

struct Ball {
    x: f64,
    y: f64,
    z: f64,
    weight: f64,
}
struct Metaballs {
    balls: Vec<Ball>,
}
impl Metaballs {
    fn new(mut random: Xoroshiro, min: f64, max: f64, radius: f64) -> Self {
        // Helpers.uniform(RandomSource,int,int) is max-exclusive.
        let count = 6 + random.bounded(2);
        let negative = (count as f64 * (-min / (max - min))) as i32;
        let mut balls = Vec::with_capacity(count as usize);
        for i in 0..count {
            let x = tri(&mut random, radius);
            let y = tri(&mut random, radius);
            let z = tri(&mut random, radius);
            let weight = if i < negative {
                uniform(&mut random, min, 0.0)
            } else {
                uniform(&mut random, 0.0, max)
            };
            balls.push(Ball { x, y, z, weight });
        }
        Self { balls }
    }
    fn inside(&mut self, x: f64, y: f64, z: f64) -> bool {
        let mut field = 0.0;
        for b in &self.balls {
            let dx = x - b.x;
            let dy = y - b.y;
            let dz = z - b.z;
            field += b.weight * b.weight.abs() / (dx * dx + dy * dy + dz * dz);
            if field > 1.0 {
                return true;
            }
        }
        false
    }
}
fn tri(r: &mut Xoroshiro, radius: f64) -> f64 {
    (r.next_double() - r.next_double()) * radius
}
fn uniform(r: &mut Xoroshiro, min: f64, max: f64) -> f64 {
    r.next_double() * (max - min) + min
}

#[cfg(test)]
mod reference_tests {
    use super::*;

    /// Golden values independently produced by Java XoroshiroRandomSource and
    /// the installed TFC Metaballs3D source (seed 123456789).
    #[test]
    fn metaballs_matches_java_fork_and_shape_fixture() {
        let mut feature = FeatureRandom::new(123_456_789);
        let size = 6 + feature.next_int_bound(4);
        let lo = feature.next_long();
        let hi = feature.next_long();
        assert_eq!(
            (size, lo, hi),
            (6, 8_777_179_533_922_683_073, -8_486_945_823_809_138_579)
        );
        let mut balls = Metaballs::new(
            Xoroshiro::from_state(lo as u64, hi as u64),
            (-0.12_f32 * size as f32) as f64,
            (0.3_f32 * size as f32) as f64,
            (0.3_f32 * size as f32) as f64,
        );
        let mut count = 0_i64;
        let mut hash = 0xcbf29ce484222325_u64;
        for x in -size..=size {
            for y in -size..=size {
                for z in -size..=size {
                    if balls.inside(x as f64, y as f64, z as f64) {
                        count += 1;
                        hash ^=
                            (((x + 32) as u64) << 32) ^ (((y + 32) as u64) << 16) ^ (z + 32) as u64;
                        hash = hash.wrapping_mul(0x100000001b3);
                    }
                }
            }
        }
        assert_eq!((count, hash), (76, 0x73eabe2f93e6f324));
    }

    /// Independent Java trace through Helpers.seedLargeFeatures' seed formula,
    /// mapped RarityFilter (nextFloat), then in_square (two bounded ints).
    #[test]
    fn stage_eight_candidate_origin_matches_java_trace() {
        let seed = vegetation::feature_seed(-70, -100, ORIGIN_Y, DECORATION_STEP, 3);
        assert_eq!(seed, -7_554_132_420_855_999_682);
        let mut random = FeatureRandom::new(seed);
        assert!(random.next_float() < 1.0_f32 / 22.0);
        let origin = in_square_origin(&mut random, -70, -100);
        assert_eq!(origin, (-1117, -1592));
        assert_eq!(random.next(24), 10_936_981);
    }

    /// Java traces for BabyBoulderFeature with a two-state state supplier.
    /// These catch state-selection calls interleaved with shape RNG draws.
    #[test]
    fn baby_sphere_and_column_match_java_rng_interleaving() {
        let states = vec!["A".to_owned(), "B".to_owned()];

        let sphere = baby_trace(0, &states);
        assert_eq!(
            sphere.0,
            vec![
                (99, 64, -20, "A"),
                (100, 64, -21, "B"),
                (100, 65, -20, "B"),
                (100, 64, -19, "B"),
                (101, 64, -20, "A"),
            ]
        );
        assert_eq!(sphere.1, 16_256_859);

        let column = baby_trace(1, &states);
        assert_eq!(column.0, vec![(100, 64, -21, "B"), (100, 66, -20, "A")]);
        assert_eq!(column.1, 15_994_460);
    }

    fn baby_trace(seed: i64, states: &[String]) -> (Vec<(i32, i32, i32, &str)>, i32) {
        let mut random = FeatureRandom::new(seed);
        let mut output = Vec::new();
        emit_baby(&mut output, 3, 100, 64, -20, states.to_vec(), &mut random).unwrap();
        let columns = output
            .into_iter()
            .map(|c| {
                (
                    c.x,
                    c.y,
                    c.z,
                    if c.block.as_ref() == "A" { "A" } else { "B" },
                )
            })
            .collect();
        (columns, random.next(24))
    }
}
