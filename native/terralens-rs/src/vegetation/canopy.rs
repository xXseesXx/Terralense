//! Fast block-column tree canopy queries, independent of terrain detail tiles.
//!
//! A request composes projected forest and palm-tree templates from nearby source
//! decoration chunks into one cached 16×16 target chunk. Saved surface columns
//! take precedence when available, including columns with no tree.

use super::{PlantCacheKey, SurfacePlant, SurfacePlantKind, plant_cache_key, trees};
use crate::{dimension::Dimension, prism::SurfaceChunk, world::World};
use lru::LruCache;
use std::{
    cell::RefCell,
    mem::size_of,
    num::NonZeroUsize,
    sync::{Arc, Mutex, OnceLock},
};

const CANOPY_CHUNK_CACHE_CAPACITY: usize = 512;
const TREE_SOURCE_CACHE_BYTE_BUDGET: usize = 32 * 1024 * 1024;

/// Highest predicted tree coverage in a block column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Canopy {
    pub y: i32,
    pub species: Arc<str>,
    pub kind: CanopyKind,
    /// The actual highest saved block wins over estimated fluid surfaces.
    pub saved: bool,
}

/// The visible tree block at the top of one map column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanopyKind {
    Leaves,
    Wood,
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct TargetKey {
    source_id: u64,
    plant: PlantCacheKey,
}

thread_local! {
    static CANOPY_CHUNK_CACHE: RefCell<LruCache<TargetKey, Arc<Vec<Option<Canopy>>>>> =
        RefCell::new(LruCache::new(NonZeroUsize::new(CANOPY_CHUNK_CACHE_CAPACITY).unwrap()));
}

struct SharedTreeCache {
    entries: LruCache<PlantCacheKey, (Arc<Vec<SurfacePlant>>, usize)>,
    bytes: usize,
}

static TREE_SOURCE_CACHE: OnceLock<Mutex<SharedTreeCache>> = OnceLock::new();

fn shared_tree_cache() -> &'static Mutex<SharedTreeCache> {
    TREE_SOURCE_CACHE.get_or_init(|| {
        Mutex::new(SharedTreeCache {
            entries: LruCache::new(NonZeroUsize::new(4096).unwrap()),
            bytes: 0,
        })
    })
}

/// Return the topmost predicted tree cell at a world block position.
///
/// Queries are cached by target chunk. At most the surrounding 5×5 source chunks
/// are considered: forest templates extend one chunk, while TFG palm patches add
/// two in-square/random-offset spreads before their templates.
pub fn canopy_at(world: &World, x: i32, z: i32, cancel: &impl Fn() -> bool) -> Option<Canopy> {
    if cancel() || world.dimension != Dimension::Overworld {
        return None;
    }
    let chunk_x = x.div_euclid(16);
    let chunk_z = z.div_euclid(16);
    let key = TargetKey {
        source_id: world.surface_source_id(),
        plant: plant_cache_key(world, chunk_x, chunk_z),
    };
    let chunk = if let Some(cached) =
        CANOPY_CHUNK_CACHE.with(|cache| cache.borrow_mut().get(&key).cloned())
    {
        cached
    } else {
        let Some(chunk) = build_chunk(world, chunk_x, chunk_z, cancel) else {
            return None;
        };
        let chunk = Arc::new(chunk);
        if !cancel() {
            CANOPY_CHUNK_CACHE.with(|cache| cache.borrow_mut().put(key, Arc::clone(&chunk)));
        }
        chunk
    };
    if cancel() {
        return None;
    }
    let local_x = x.rem_euclid(16) as usize;
    let local_z = z.rem_euclid(16) as usize;
    chunk[local_z * 16 + local_x].clone()
}

fn build_chunk(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Option<Vec<Option<Canopy>>> {
    if cancel() {
        return None;
    }

    // A generated save chunk is authoritative for all 256 columns. In particular,
    // a saved column without a tree suppresses any seed-based forecast.
    if world.surface_source_id() != 0
        && let Some(saved) = world.saved_surface(chunk_x, chunk_z)
    {
        return Some(from_saved(&saved));
    }

    let mut result: Vec<Option<Canopy>> = (0..256).map(|_| None).collect();
    let min_x = chunk_x.wrapping_mul(16);
    let min_z = chunk_z.wrapping_mul(16);
    let max_x = min_x.saturating_add(15);
    let max_z = min_z.saturating_add(15);
    for source_z in chunk_z.saturating_sub(2)..=chunk_z.saturating_add(2) {
        for source_x in chunk_x.saturating_sub(2)..=chunk_x.saturating_add(2) {
            if cancel() {
                return None;
            }
            let source = source_trees(world, source_x, source_z, cancel);
            if cancel() {
                return None;
            }
            for plant in source.iter() {
                // Keep the actual top block type in each column. Exposed branches
                // belong to the canopy silhouette, including all-wood dead trees.
                if !matches!(
                    plant.kind,
                    SurfacePlantKind::Tree | SurfacePlantKind::TreeWood
                ) || plant.x < min_x
                    || plant.x > max_x
                    || plant.z < min_z
                    || plant.z > max_z
                {
                    continue;
                }
                compose_plant(&mut result, min_x, min_z, plant);
            }
        }
    }
    (!cancel()).then_some(result)
}

fn compose_plant(result: &mut [Option<Canopy>], min_x: i32, min_z: i32, plant: &SurfacePlant) {
    let kind = match plant.kind {
        SurfacePlantKind::Tree => CanopyKind::Leaves,
        SurfacePlantKind::TreeWood => CanopyKind::Wood,
        SurfacePlantKind::Bush | SurfacePlantKind::Crop => return,
    };
    let index = (plant.z.wrapping_sub(min_z) as usize) * 16 + plant.x.wrapping_sub(min_x) as usize;
    let replace = match &result[index] {
        None => true,
        Some(current) => {
            plant.y > current.y
                || (plant.y == current.y
                    && (kind == CanopyKind::Wood && current.kind == CanopyKind::Leaves
                        || (kind == current.kind
                            && plant.variant.as_str() < current.species.as_ref())))
        }
    };
    if replace {
        result[index] = Some(Canopy {
            y: plant.y,
            species: Arc::from(plant.variant.as_str()),
            kind,
            saved: false,
        });
    }
}

fn from_saved(saved: &SurfaceChunk) -> Vec<Option<Canopy>> {
    (0..256)
        .map(|index| {
            let column = saved.columns.get(index)?;
            let (species, kind) = if let Some(species) = column.tree.as_deref() {
                (species.to_owned(), CanopyKind::Leaves)
            } else {
                let species = saved_wood_species(&column.block)?;
                (species, CanopyKind::Wood)
            };
            Some(Canopy {
                y: column.y,
                species: Arc::from(species),
                kind,
                saved: true,
            })
        })
        .collect()
}

pub(super) fn source_trees(
    world: &World,
    chunk_x: i32,
    chunk_z: i32,
    cancel: &impl Fn() -> bool,
) -> Arc<Vec<super::SurfacePlant>> {
    if cancel() {
        return Arc::new(Vec::new());
    }
    let key = plant_cache_key(world, chunk_x, chunk_z);
    if let Some(cached) = shared_tree_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entries
        .get(&key)
        .map(|(plants, _)| Arc::clone(plants))
    {
        return cached;
    }
    let mut plants = trees::generate(world, chunk_x, chunk_z, cancel);
    if !cancel() {
        plants.extend(crate::vegetation::crops::generate_tree_patches(
            world, chunk_x, chunk_z, cancel,
        ));
    }
    let plants = plants
        .into_iter()
        .filter(|plant| {
            matches!(
                plant.kind,
                SurfacePlantKind::Tree | SurfacePlantKind::TreeWood
            )
        })
        .collect::<Vec<_>>();
    let plants = Arc::new(plants);
    if !cancel() {
        let bytes = estimated_tree_bytes(&plants, plants.capacity());
        if bytes <= TREE_SOURCE_CACHE_BYTE_BUDGET {
            let mut cache = shared_tree_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let SharedTreeCache {
                entries,
                bytes: used_bytes,
            } = &mut *cache;
            insert_tree_entry(
                entries,
                used_bytes,
                key,
                Arc::clone(&plants),
                bytes,
                TREE_SOURCE_CACHE_BYTE_BUDGET,
            );
        }
    }
    plants
}

fn saved_wood_species(block: &str) -> Option<String> {
    let path = block.rsplit_once(':').map_or(block, |(_, path)| path);
    if path == "palm_tree/trunk" || path.starts_with("palm_tree/trunk[") {
        return Some("tfg:tree/palm".to_owned());
    }
    let wood = path.strip_prefix("wood/")?;
    let species = [
        "log/",
        "branch/",
        "wood/",
        "stripped_log/",
        "stripped_branch/",
        "stripped_wood/",
        "log_stripped/",
        "branch_stripped/",
        "wood_stripped/",
    ]
    .iter()
    .find_map(|prefix| wood.strip_prefix(prefix))?;
    (!species.is_empty()).then(|| format!("tfc:tree/{species}"))
}

fn insert_tree_entry(
    entries: &mut LruCache<PlantCacheKey, (Arc<Vec<SurfacePlant>>, usize)>,
    used_bytes: &mut usize,
    key: PlantCacheKey,
    plants: Arc<Vec<SurfacePlant>>,
    bytes: usize,
    budget: usize,
) {
    if bytes > budget {
        return;
    }
    if let Some((_, old_bytes)) = entries.pop(&key) {
        *used_bytes = used_bytes.saturating_sub(old_bytes);
    }
    while used_bytes.saturating_add(bytes) > budget {
        let Some((_, (_, evicted_bytes))) = entries.pop_lru() else {
            break;
        };
        *used_bytes = used_bytes.saturating_sub(evicted_bytes);
    }
    // `push` can also evict at the hard entry-count limit. Account for that
    // eviction here, including zero-byte empty source chunks.
    if let Some((_, (_, evicted_bytes))) = entries.push(key, (plants, bytes)) {
        *used_bytes = used_bytes.saturating_sub(evicted_bytes);
    }
    *used_bytes = used_bytes.saturating_add(bytes);
}

fn estimated_tree_bytes(plants: &[SurfacePlant], vector_capacity: usize) -> usize {
    size_of::<Vec<SurfacePlant>>()
        .saturating_add(vector_capacity.saturating_mul(size_of::<SurfacePlant>()))
        .saturating_add(
            plants
                .iter()
                .map(|plant| plant.variant.capacity())
                .sum::<usize>(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vegetation::templates::TemplateBlockKind;

    #[test]
    fn saved_columns_are_authoritative_and_keep_species_and_height() {
        let saved = SurfaceChunk {
            chunk_x: 0,
            chunk_z: 0,
            columns: (0..256)
                .map(|i| crate::prism::SurfaceColumn {
                    y: 64 + i,
                    block: "tfc:grass".into(),
                    tree: (i == 17).then(|| "tfc:tree/oak".into()),
                    crop: None,
                    color: [0, 0, 0],
                })
                .collect(),
        };
        let canopy = from_saved(&saved);
        assert!(canopy[0].is_none());
        assert_eq!(canopy[17].as_ref().map(|c| c.y), Some(81));
        assert_eq!(
            canopy[17].as_ref().map(|c| c.species.as_ref()),
            Some("tfc:tree/oak")
        );
        assert_eq!(canopy[17].as_ref().unwrap().kind, CanopyKind::Leaves);
    }

    #[test]
    fn saved_top_wood_is_included_and_empty_columns_stay_empty() {
        let mut columns = vec![
            crate::prism::SurfaceColumn {
                y: 64,
                block: "minecraft:grass_block".into(),
                tree: None,
                crop: None,
                color: [0; 3],
            };
            256
        ];
        columns[17].y = 81;
        columns[17].block = "tfc:wood/branch/acacia".into();
        columns[18].y = 82;
        columns[18].block = "tfc:wood/log_stripped/oak".into();
        columns[19].y = 83;
        columns[19].block = "tfg:palm_tree/trunk".into();
        let canopy = from_saved(&SurfaceChunk {
            chunk_x: 0,
            chunk_z: 0,
            columns,
        });

        assert_eq!(canopy[0], None, "saved terrain with no tree stays bare");
        for (index, species) in [
            (17, "tfc:tree/acacia"),
            (18, "tfc:tree/oak"),
            (19, "tfg:tree/palm"),
        ] {
            let cell = canopy[index].as_ref().unwrap();
            assert_eq!(cell.y, 64 + index as i32);
            assert_eq!(cell.species.as_ref(), species);
            assert_eq!(cell.kind, CanopyKind::Wood);
        }
    }

    #[test]
    fn equal_height_species_tiebreak_is_lexical() {
        let mut result: Vec<Option<Canopy>> = (0..256).map(|_| None).collect();
        let make = |y, variant: &str| SurfacePlant {
            x: 2,
            y,
            z: 3,
            kind: SurfacePlantKind::Tree,
            variant: variant.into(),
        };
        compose_plant(&mut result, 0, 0, &make(10, "tfc:tree/zebrawood"));
        compose_plant(&mut result, 0, 0, &make(10, "tfc:tree/ash"));
        assert_eq!(
            result[50].as_ref().unwrap().species.as_ref(),
            "tfc:tree/ash"
        );
        compose_plant(&mut result, 0, 0, &make(9, "tfc:tree/acacia"));
        assert_eq!(
            result[50].as_ref().unwrap().species.as_ref(),
            "tfc:tree/ash"
        );
    }

    #[test]
    fn authored_acacia_live_branch_and_dead_template_keep_their_silhouettes() {
        let live_top = (0..256)
            .filter_map(|seed| {
                let template = crate::vegetation::templates::select("tfc:tree/acacia", seed)?;
                let top = template_top_cells(&template.cells);
                top.values()
                    .any(|cell| cell.kind == TemplateBlockKind::Wood)
                    .then_some(top)
            })
            .next()
            .expect("an installed live acacia variant exposes top-down wood");

        let dead = crate::vegetation::templates::select("tfc:tree/acacia_dead", 7)
            .expect("installed dead acacia template");
        let dead_top = template_top_cells(&dead.cells);
        assert!(!dead_top.is_empty());
        assert!(
            dead_top
                .values()
                .all(|cell| cell.kind == TemplateBlockKind::Wood)
        );

        for (feature, cells) in [
            ("tfc:tree/acacia", live_top),
            ("tfc:tree/acacia_dead", dead_top),
        ] {
            let mut result: Vec<Option<Canopy>> = (0..256).map(|_| None).collect();
            for (&(x, z), cell) in &cells {
                let plant = SurfacePlant {
                    x: i32::from(x) + 8,
                    y: i32::from(cell.y),
                    z: i32::from(z) + 8,
                    kind: match cell.kind {
                        TemplateBlockKind::Leaves => SurfacePlantKind::Tree,
                        TemplateBlockKind::Wood => SurfacePlantKind::TreeWood,
                        TemplateBlockKind::Plant => continue,
                    },
                    variant: feature.into(),
                };
                compose_plant(&mut result, 0, 0, &plant);
            }
            assert_eq!(
                result.iter().flatten().count(),
                cells.len(),
                "{feature} outline"
            );
            for ((x, z), expected) in cells {
                let actual = result[((z + 8) * 16 + x + 8) as usize].as_ref().unwrap();
                assert_eq!(actual.y, i32::from(expected.y));
                assert_eq!(
                    actual.kind,
                    if expected.kind == TemplateBlockKind::Wood {
                        CanopyKind::Wood
                    } else {
                        CanopyKind::Leaves
                    }
                );
                assert_eq!(actual.species.as_ref(), feature);
            }
        }
    }

    fn template_top_cells(
        cells: &[crate::vegetation::templates::TemplateCell],
    ) -> std::collections::BTreeMap<(i16, i16), crate::vegetation::templates::TemplateCell> {
        let mut top = std::collections::BTreeMap::new();
        for &cell in cells {
            let slot = top.entry((cell.x, cell.z)).or_insert(cell);
            if cell.y > slot.y || (cell.y == slot.y && cell.kind == TemplateBlockKind::Wood) {
                *slot = cell;
            }
        }
        top
    }

    #[test]
    fn wood_wins_same_height_projection_tie_and_taller_leaf_replaces_it() {
        let mut result: Vec<Option<Canopy>> = (0..256).map(|_| None).collect();
        let make = |y, kind, species: &str| SurfacePlant {
            x: 2,
            y,
            z: 3,
            kind,
            variant: species.into(),
        };
        compose_plant(
            &mut result,
            0,
            0,
            &make(10, SurfacePlantKind::Tree, "tfc:tree/oak"),
        );
        compose_plant(
            &mut result,
            0,
            0,
            &make(10, SurfacePlantKind::TreeWood, "tfc:tree/oak"),
        );
        assert_eq!(result[50].as_ref().unwrap().kind, CanopyKind::Wood);
        compose_plant(
            &mut result,
            0,
            0,
            &make(11, SurfacePlantKind::Tree, "tfc:tree/oak"),
        );
        assert_eq!(result[50].as_ref().unwrap().kind, CanopyKind::Leaves);
    }

    #[test]
    fn cancelled_source_and_target_are_not_cached_and_non_overworld_is_ignored() {
        use crate::{dimension::Dimension, model::Settings, world::World};
        let world = World::new(i64::MIN + 173, Settings::default());
        let key = plant_cache_key(&world, 1_234_567, -1_234_567);
        let cancelled = || true;
        let source = source_trees(&world, key.chunk_x, key.chunk_z, &cancelled);
        assert!(source.is_empty());
        assert!(
            shared_tree_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entries
                .peek(&key)
                .is_none()
        );
        assert!(canopy_at(&world, key.chunk_x * 16, key.chunk_z * 16, &cancelled).is_none());
        let target_key = TargetKey {
            source_id: 0,
            plant: key,
        };
        assert!(CANOPY_CHUNK_CACHE.with(|cache| cache.borrow_mut().peek(&target_key).is_none()));

        let moon = World::for_dimension(i64::MIN + 174, Settings::default(), Dimension::Moon, 64);
        assert!(canopy_at(&moon, 0, 0, &|| false).is_none());
    }

    #[test]
    fn saved_source_identity_is_part_of_target_cache_key() {
        let plant = PlantCacheKey {
            seed: 1,
            settings: [0; 6],
            chunk_x: 0,
            chunk_z: 0,
        };
        assert_ne!(
            TargetKey {
                source_id: 7,
                plant
            },
            TargetKey {
                source_id: 8,
                plant
            }
        );
    }

    #[test]
    fn entry_limit_eviction_updates_shared_byte_accounting() {
        let mut entries = LruCache::new(NonZeroUsize::new(1).unwrap());
        let mut used_bytes = 0;
        let make_key = |chunk_x| PlantCacheKey {
            seed: 3,
            settings: [0; 6],
            chunk_x,
            chunk_z: 0,
        };
        let empty = Arc::new(Vec::new());
        insert_tree_entry(
            &mut entries,
            &mut used_bytes,
            make_key(0),
            Arc::clone(&empty),
            0,
            16,
        );
        insert_tree_entry(
            &mut entries,
            &mut used_bytes,
            make_key(1),
            Arc::clone(&empty),
            0,
            16,
        );
        assert_eq!(used_bytes, 0);

        insert_tree_entry(
            &mut entries,
            &mut used_bytes,
            make_key(2),
            Arc::clone(&empty),
            12,
            16,
        );
        assert_eq!(used_bytes, 12);
        insert_tree_entry(
            &mut entries,
            &mut used_bytes,
            make_key(3),
            Arc::clone(&empty),
            12,
            16,
        );
        assert_eq!(
            used_bytes, 12,
            "the implicit entry-count eviction must release its bytes"
        );
        assert!(entries.peek(&make_key(3)).is_some());
    }
}
