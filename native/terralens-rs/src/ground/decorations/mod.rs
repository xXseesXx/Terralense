//! Sparse, cached surface changes from TFG's placed decoration features.
use super::{GroundSample, base_sample, kind_of};
use crate::{dimension::Dimension, world::World};
use lru::LruCache;
use std::{cell::RefCell, collections::HashMap, num::NonZeroUsize, sync::Arc};

mod boulders;
#[cfg(test)]
mod clay;
mod soils;

#[derive(Clone, Debug)]
pub(super) struct BoulderColumn {
    pub index: u32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: Arc<str>,
}

#[derive(Clone, Debug)]
pub(super) struct SoilPatch {
    pub index: u32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub radius: i32,
    pub height: i32,
    pub states: Arc<HashMap<String, Arc<str>>>,
}

#[derive(Clone, Debug)]
#[cfg(test)]
pub(super) struct ClayPatch {
    pub index: u32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub radius: i32,
    pub height: i32,
}

struct Source {
    boulders: Arc<Vec<BoulderColumn>>,
    soils: Vec<SoilPatch>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct Key {
    seed: i64,
    settings: [u64; 6],
    x: i32,
    z: i32,
}

impl Key {
    fn new(world: &World, x: i32, z: i32) -> Self {
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
            x,
            z,
        }
    }
}

type Changes = Arc<Vec<Option<GroundSample>>>;
thread_local! {
    static BOULDERS: RefCell<LruCache<Key, Arc<Vec<BoulderColumn>>>> = RefCell::new(LruCache::new(NonZeroUsize::new(512).unwrap()));
    static SOURCES: RefCell<LruCache<Key, Arc<Source>>> = RefCell::new(LruCache::new(NonZeroUsize::new(512).unwrap()));
    static TARGETS: RefCell<LruCache<Key, Changes>> = RefCell::new(LruCache::new(NonZeroUsize::new(128).unwrap()));
    static FOREST_TARGETS: RefCell<LruCache<Key, Changes>> = RefCell::new(LruCache::new(NonZeroUsize::new(128).unwrap()));
}

pub(super) fn sample(world: &World, x: i32, z: i32) -> Option<GroundSample> {
    sample_cancelable(world, x, z, &|| false).flatten()
}

pub(super) fn pre_forest(world: &World, x: i32, z: i32) -> Option<GroundSample> {
    sample_at_stage(world, x, z, true, &|| false).flatten()
}

#[cfg(feature = "diagnostics")]
pub(super) fn diagnostic_candidates(world: &World, x: i32, z: i32) -> serde_json::Value {
    let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
    let mut boulders = Vec::new();
    let mut soils = Vec::new();
    for sz in cz - 1..=cz + 1 {
        for sx in cx - 1..=cx + 1 {
            if let Some(source) = source(world, sx, sz, &|| false) {
                for b in source.boulders.iter().filter(|b| b.x == x && b.z == z) {
                    boulders.push(serde_json::json!({"source_chunk":[sx,sz],"index":b.index,"y":b.y,"block":b.block.as_ref()}));
                }
                for p in &source.soils {
                    let (dx, dz) = (x - p.x, z - p.z);
                    if dx * dx + dz * dz <= p.radius * p.radius {
                        soils.push(serde_json::json!({"source_chunk":[sx,sz],"index":p.index,
                        "origin":[p.x,p.y,p.z],"radius":p.radius,"height":p.height}));
                    }
                }
            }
        }
    }
    serde_json::json!({"boulders":boulders,"soils":soils})
}

fn sample_cancelable(
    world: &World,
    x: i32,
    z: i32,
    cancel: &impl Fn() -> bool,
) -> Option<Option<GroundSample>> {
    sample_at_stage(world, x, z, false, cancel)
}

fn sample_at_stage(
    world: &World,
    x: i32,
    z: i32,
    before_forest: bool,
    cancel: &impl Fn() -> bool,
) -> Option<Option<GroundSample>> {
    if cancel() {
        return None;
    }
    if world.dimension != Dimension::Overworld {
        return Some(None);
    }
    let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
    let key = Key::new(world, cx, cz);
    let cache = if before_forest {
        &FOREST_TARGETS
    } else {
        &TARGETS
    };
    let cached = cache.with(|cache| cache.borrow_mut().get(&key).cloned());
    let changes = match cached {
        Some(changes) => changes,
        None => {
            let changes = Arc::new(build(world, cx, cz, before_forest, cancel)?);
            if cancel() {
                return None;
            }
            cache.with(|cache| cache.borrow_mut().put(key, changes.clone()));
            changes
        }
    };
    Some(changes[(z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize].clone())
}

fn boulder_source(
    world: &World,
    cx: i32,
    cz: i32,
    cancel: &impl Fn() -> bool,
) -> Option<Arc<Vec<BoulderColumn>>> {
    let key = Key::new(world, cx, cz);
    if let Some(source) = BOULDERS.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        return Some(source);
    }
    let source = Arc::new(boulders::generate(world, cx, cz, cancel)?);
    if cancel() {
        return None;
    }
    BOULDERS.with(|cache| cache.borrow_mut().put(key, source.clone()));
    Some(source)
}

fn source(world: &World, cx: i32, cz: i32, cancel: &impl Fn() -> bool) -> Option<Arc<Source>> {
    let key = Key::new(world, cx, cz);
    if let Some(source) = SOURCES.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        return Some(source);
    }
    let source = Arc::new(Source {
        boulders: boulder_source(world, cx, cz, cancel)?,
        soils: soils::generate(world, cx, cz, cancel)?,
    });
    if cancel() {
        return None;
    }
    SOURCES.with(|cache| cache.borrow_mut().put(key, source.clone()));
    Some(source)
}

fn build(
    world: &World,
    cx: i32,
    cz: i32,
    before_forest: bool,
    cancel: &impl Fn() -> bool,
) -> Option<Vec<Option<GroundSample>>> {
    let mut sources = Vec::with_capacity(9);
    let mut boulder_chunks = Vec::with_capacity(9);
    // Max soil radius is 13, large-boulder radius 9, and small-patch reach 6.
    // The surrounding 3x3 source chunks include every possible intersection.
    for sz in cz.saturating_sub(1)..=cz.saturating_add(1) {
        for sx in cx.saturating_sub(1)..=cx.saturating_add(1) {
            if cancel() {
                return None;
            }
            if before_forest {
                boulder_chunks.push(boulder_source(world, sx, sz, cancel)?);
            } else {
                let source = source(world, sx, sz, cancel)?;
                boulder_chunks.push(source.boulders.clone());
                sources.push(source);
            }
        }
    }
    let (min_x, min_z) = (cx.saturating_mul(16), cz.saturating_mul(16));
    let mut base = vec![None; 256];
    let mut result = vec![None; 256];
    let mut boulders: Vec<_> = boulder_chunks.iter().flat_map(|s| s.iter()).collect();
    boulders.sort_by_key(|b| b.index);
    for b in boulders {
        if cancel() {
            return None;
        }
        let (dx, dz) = (
            i64::from(b.x) - i64::from(min_x),
            i64::from(b.z) - i64::from(min_z),
        );
        if !(0..16).contains(&dx) || !(0..16).contains(&dz) {
            continue;
        }
        let index = (dz * 16 + dx) as usize;
        if let Some(ground) = column(world, b.x, b.z, index, &mut base, &mut result)
            && b.y >= ground.y
        {
            *ground = GroundSample {
                y: b.y,
                block: b.block.clone(),
                kind: kind_of(&b.block),
            };
        }
    }
    let mut soils: Vec<_> = sources.iter().flat_map(|s| &s.soils).collect();
    soils.sort_by_key(|s| s.index);
    for patch in soils {
        if cancel() {
            return None;
        }
        for z in patch.z.saturating_sub(patch.radius).max(min_z)
            ..=patch
                .z
                .saturating_add(patch.radius)
                .min(min_z.saturating_add(15))
        {
            for x in patch.x.saturating_sub(patch.radius).max(min_x)
                ..=patch
                    .x
                    .saturating_add(patch.radius)
                    .min(min_x.saturating_add(15))
            {
                let (dx, dz) = (x - patch.x, z - patch.z);
                if dx * dx + dz * dz > patch.radius * patch.radius {
                    continue;
                }
                let index = ((z - min_z) * 16 + x - min_x) as usize;
                if let Some(ground) = column(world, x, z, index, &mut base, &mut result)
                    && (ground.y - patch.y).abs() <= patch.height
                    && let Some(block) = patch.states.get(ground.block.as_ref())
                {
                    ground.block = block.clone();
                    ground.kind = kind_of(block);
                }
            }
        }
    }
    for i in 0..256 {
        if result[i] == base[i] {
            result[i] = None;
        }
    }
    Some(result)
}

fn column<'a>(
    world: &World,
    x: i32,
    z: i32,
    index: usize,
    base: &mut [Option<GroundSample>],
    result: &'a mut [Option<GroundSample>],
) -> Option<&'a mut GroundSample> {
    if result[index].is_none() {
        let ground = base_sample(world, x, z);
        base[index] = ground.clone();
        result[index] = ground;
    }
    result[index].as_mut()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ground::GroundKind, model::Settings};

    #[test]
    fn source_boulders_cross_chunk_edges_and_keep_their_surface_height() {
        let world = World::new(0, Settings::default());
        // Installed feature gates and stage-8 RNG produce a mossy basalt boulder
        // whose west edge crosses into the neighboring chunk at this dry site.
        let (cx, cz) = (-4096, -4096);
        let (x, z) = (-65538, -65533);
        let b = boulders::generate(&world, cx, cz, &|| false)
            .unwrap()
            .into_iter()
            .find(|b| (b.x, b.z, b.index) == (x, z, 5))
            .expect("source-driven mossy boulder witness");
        assert_ne!((x.div_euclid(16), z.div_euclid(16)), (cx, cz));
        assert_eq!(b.block.as_ref(), "tfc:rock/mossy_cobble/basalt");
        assert_eq!(b.y, 73);
        assert!(!world.probe(x, z).is_fluid());
        let terrain = world.height(x, z);
        let base = base_sample(&world, x, z).unwrap();
        assert!(b.y > base.y);
        let actual = sample(&world, x, z).expect("visible boulder projection");
        assert!(actual.y >= b.y);
        assert_eq!(actual.kind, GroundKind::Rock);
        assert_eq!(actual, super::super::sample(&world, x, z).unwrap());
        assert_eq!(
            super::super::sample_with_detail(&world, x, z, false).unwrap(),
            base,
            "coarse views skip decoration work"
        );
        assert_eq!(sample(&world, x, z), Some(actual.clone()));
        let mut probe = world.probe(x, z);
        crate::ground_view::populate(&world, x, z, &mut probe, false);
        assert_eq!(probe.ground_block.as_deref(), Some(actual.block.as_ref()));
        assert_eq!(probe.ground_y, Some(actual.y));
        assert_eq!(
            crate::ground_view::relief_height(&world, x, z, terrain),
            Some(f64::from(actual.y))
        );
        assert_eq!(
            world.height(x, z),
            terrain,
            "relief must not alter generator height"
        );
    }

    #[test]
    fn cancelled_surface_projection_never_publishes_partial_target_data() {
        let world = World::new(0, Settings::default());
        let (x, z) = (-1264_i32, 4689_i32);
        let key = Key::new(&world, x.div_euclid(16), z.div_euclid(16));
        TARGETS.with(|cache| cache.borrow_mut().pop(&key));
        let calls = std::cell::Cell::new(0);
        let cancelled = sample_cancelable(&world, x, z, &|| {
            let n = calls.get() + 1;
            calls.set(n);
            n >= 5
        });
        assert!(cancelled.is_none());
        assert!(!TARGETS.with(|cache| cache.borrow().contains(&key)));
        assert!(sample_cancelable(&world, x, z, &|| false).is_some());
        assert!(TARGETS.with(|cache| cache.borrow().contains(&key)));
    }
}
