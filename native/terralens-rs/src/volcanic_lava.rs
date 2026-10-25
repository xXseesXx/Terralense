//! TFG 0.9.25 volcanic surface water, lava and magma over terrain columns.
//!
//! Tuya calderas and the pack's active cinder cones use TFC's FloodFillLakeFeature. On solid
//! height columns, its six-neighbor fill reduces to connected horizontal basins.
//! This preserves the actual 44-block feature boundary, spill height and minimum
//! volume. Installed cinder rivulets, fissure sources and encased spouts also contribute.
//! Full caves, springs and later fluid ticks remain outside the seed-only model;
//! opened worlds use complete saved surface columns wherever available.

use crate::vegetation::FeatureRandom;
use crate::{
    biome::ids::id,
    noise::{Cell, TfgCellular2D},
    world::World,
};
use lru::LruCache;
use std::{cell::RefCell, cmp::Reverse, collections::BinaryHeap, num::NonZeroUsize, sync::Arc};
mod dead;
mod rivulet;
mod sources;

pub const COLOR: [u8; 3] = [245, 107, 28];
pub const MAGMA_COLOR: [u8; 3] = [190, 71, 24];
// Rivulets can write throughout the owner's three chunks. Caldera flood filling
// keeps its original 44-block box, two columns inside this surface window.
const SPAN: usize = 48;
const GRID: usize = SPAN + 2;
const FILL_GRID: usize = 46;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Water(i32),
    Lava(i32),
    Magma(i32),
}

impl Surface {
    pub fn y(self) -> i32 {
        match self {
            Self::Water(y) | Self::Lava(y) | Self::Magma(y) => y,
        }
    }
    pub fn is_hot(self) -> bool {
        matches!(self, Self::Lava(_) | Self::Magma(_))
    }
    /// Highest saved blocks only: buried magma and waterlogged solids stay terrain.
    pub fn from_block(block: &str, y: i32) -> Option<Self> {
        match block {
            "minecraft:lava" => Some(Self::Lava(y)),
            "minecraft:magma_block" => Some(Self::Magma(y)),
            "minecraft:water"
            | "tfc:fluid/salt_water"
            | "tfc:fluid/spring_water"
            | "tfc:fluid/river_water" => Some(Self::Water(y)),
            name if name.starts_with("tfc:rock/magma/") => Some(Self::Magma(y)),
            _ => None,
        }
    }
}

type Center = (i32, i32);
type BasinCache = LruCache<(Family, i32, i32), Option<Arc<Basin>>>;
type ChunkSurfaceCache = LruCache<(i32, i32), Arc<Vec<Option<Surface>>>>;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Family {
    Cinder,
    Tuff,
    Tuya,
}

pub struct Sampler {
    tuya_cells: TfgCellular2D,
    cinder_cells: TfgCellular2D,
    tuff_cells: TfgCellular2D,
    basins: RefCell<BasinCache>,
    chunk_surfaces: RefCell<ChunkSurfaceCache>,
}

struct Basin {
    min_x: i32,
    min_z: i32,
    surface: Vec<Option<Surface>>,
}

impl Basin {
    fn at(&self, x: i32, z: i32) -> Option<Surface> {
        let dx = x.checked_sub(self.min_x)?;
        let dz = z.checked_sub(self.min_z)?;
        if !(0..SPAN as i32).contains(&dx) || !(0..SPAN as i32).contains(&dz) {
            return None;
        }
        self.surface[(dz as usize + 1) * GRID + dx as usize + 1]
    }
}

impl Sampler {
    pub fn new(seed: i64) -> Self {
        let mut cells = TfgCellular2D::configured(seed, 0.21, 1);
        cells.spread(0.0033_f32 as f64);
        let mut cinder_cells = TfgCellular2D::new(seed);
        cinder_cells.spread(0.009_f32 as f64);
        let mut tuff_cells = TfgCellular2D::configured(seed, 0.2, 1);
        tuff_cells.spread(0.003_f32 as f64);
        Self {
            tuya_cells: cells,
            cinder_cells,
            tuff_cells,
            basins: RefCell::new(LruCache::new(NonZeroUsize::new(128).unwrap())),
            // 512 cached 16×16 chunks bound the output cache to roughly 1.5 MiB.
            // Empty chunks are cached too, avoiding repeat Voronoi/basin lookups while
            // panning or issuing many point queries at far zoom levels.
            chunk_surfaces: RefCell::new(LruCache::new(NonZeroUsize::new(512).unwrap())),
        }
    }

    pub fn surface(&self, world: &World, x: i32, z: i32) -> Option<Surface> {
        let chunk = (x.div_euclid(16), z.div_euclid(16));
        let cached = self.chunk_surfaces.borrow_mut().get(&chunk).cloned();
        if let Some(surface) = cached {
            let lx = x.rem_euclid(16) as usize;
            let lz = z.rem_euclid(16) as usize;
            return surface.get(lz * 16 + lx).copied().flatten();
        }
        let min_x = chunk.0 * 16;
        let min_z = chunk.1 * 16;
        let mut surfaces = Vec::new();
        for family in [Family::Cinder, Family::Tuya, Family::Tuff] {
            if family == Family::Tuff && world.surface_source_id() == 0 {
                continue;
            }
            let (candidates, count) = self.chunk_candidates(min_x, min_z, family);
            self.apply_chunk_candidates(
                world,
                min_x,
                min_z,
                family,
                &candidates[..count],
                &mut surfaces,
            );
        }
        let surfaces = Arc::new(surfaces);
        self.chunk_surfaces
            .borrow_mut()
            .put(chunk, surfaces.clone());
        let lx = x.rem_euclid(16) as usize;
        let lz = z.rem_euclid(16) as usize;
        surfaces.get(lz * 16 + lx).copied().flatten()
    }

    fn chunk_candidates(&self, min_x: i32, min_z: i32, family: Family) -> ([Cell; 16], usize) {
        let (cells, frequency) = self.family_cells(family);
        let min_cx = (f64::from(min_x) * frequency).floor() as i32 - 1;
        let max_cx = (f64::from(min_x + 15) * frequency).floor() as i32 + 1;
        let min_cz = (f64::from(min_z) * frequency).floor() as i32 - 1;
        let max_cz = (f64::from(min_z + 15) * frequency).floor() as i32 + 1;
        // Frequencies are <= 0.009, so 16 blocks cross at most two lattice cells
        // per axis. Including each axis's neighbors yields at most a 4×4 union.
        let mut points = [Cell::default(); 16];
        let mut count = 0;
        // Match family_surface's lattice order (Z outer, X inner), including equal-Y
        // max tie behavior.
        for cz in min_cz..=max_cz {
            for cx in min_cx..=max_cx {
                let cell = cells.feature_point(cx, cz);
                if cell.noise.abs() <= 0.5 {
                    debug_assert!(count < points.len());
                    points[count] = cell;
                    count += 1;
                }
            }
        }
        (points, count)
    }

    fn apply_chunk_candidates(
        &self,
        world: &World,
        min_x: i32,
        min_z: i32,
        family: Family,
        candidates: &[Cell],
        output: &mut Vec<Option<Surface>>,
    ) {
        let (_, frequency) = self.family_cells(family);
        for cell in candidates {
            let center = (cell.x as i32, cell.y as i32);
            let Some(feature_min_x) = center
                .0
                .div_euclid(16)
                .checked_mul(16)
                .and_then(|x| x.checked_sub(16))
            else {
                continue;
            };
            let Some(feature_min_z) = center
                .1
                .div_euclid(16)
                .checked_mul(16)
                .and_then(|z| z.checked_sub(16))
            else {
                continue;
            };
            let x0 = min_x.max(feature_min_x);
            let z0 = min_z.max(feature_min_z);
            let x1 = (min_x + 15).min(feature_min_x + SPAN as i32 - 1);
            let z1 = (min_z + 15).min(feature_min_z + SPAN as i32 - 1);
            if x0 > x1 || z0 > z1 {
                continue;
            }
            // A lattice point is only a candidate for a query whose original 3×3
            // neighborhood contains it. Find one overlapping block before building
            // or reading the basin, using integer bounds only.
            let probe = (x0..=x1).find_map(|x| {
                ((z0..=z1).find(|z| {
                    (f64::from(x) * frequency).floor() as i32 - 1 <= cell.cx
                        && cell.cx <= (f64::from(x) * frequency).floor() as i32 + 1
                        && (f64::from(*z) * frequency).floor() as i32 - 1 <= cell.cy
                        && cell.cy <= (f64::from(*z) * frequency).floor() as i32 + 1
                }))
                .map(|z| (x, z))
            });
            let Some((probe_x, probe_z)) = probe else {
                continue;
            };
            let _ = self.point_surface(world, probe_x, probe_z, family, *cell);
            let key = (family, center.0, center.1);
            let basin = self.basins.borrow_mut().get(&key).cloned().flatten();
            let Some(basin) = basin else { continue };
            for z in z0..=z1 {
                if cell.cy < (f64::from(z) * frequency).floor() as i32 - 1
                    || cell.cy > (f64::from(z) * frequency).floor() as i32 + 1
                {
                    continue;
                }
                for x in x0..=x1 {
                    if cell.cx < (f64::from(x) * frequency).floor() as i32 - 1
                        || cell.cx > (f64::from(x) * frequency).floor() as i32 + 1
                    {
                        continue;
                    }
                    let Some(surface) = basin.at(x, z) else {
                        continue;
                    };
                    let index = (z - min_z) as usize * 16 + (x - min_x) as usize;
                    if output.is_empty() {
                        output.resize(16 * 16, None);
                    }
                    if output[index].is_none_or(|old| surface.y() >= old.y()) {
                        output[index] = Some(surface);
                    }
                }
            }
        }
    }

    fn family_cells(&self, family: Family) -> (&TfgCellular2D, f64) {
        match family {
            Family::Cinder => (&self.cinder_cells, 0.009_f32 as f64),
            Family::Tuff => (&self.tuff_cells, 0.003_f32 as f64),
            Family::Tuya => (&self.tuya_cells, 0.0033_f32 as f64),
        }
    }

    #[cfg(test)]
    fn direct_surface(&self, world: &World, x: i32, z: i32) -> Option<Surface> {
        self.family_surface(world, x, z, Family::Cinder)
            .into_iter()
            .chain(self.family_surface(world, x, z, Family::Tuya))
            .chain(
                (world.surface_source_id() != 0)
                    .then(|| self.family_surface(world, x, z, Family::Tuff))
                    .flatten(),
            )
            .max_by_key(|surface| surface.y())
    }

    #[cfg(test)]
    fn family_surface(&self, world: &World, x: i32, z: i32, family: Family) -> Option<Surface> {
        let (cells, frequency) = match family {
            Family::Cinder => (&self.cinder_cells, 0.009_f32 as f64),
            Family::Tuff => (&self.tuff_cells, 0.003_f32 as f64),
            Family::Tuya => (&self.tuya_cells, 0.0033_f32 as f64),
        };
        let cx = (x as f64 * frequency).floor() as i32;
        let cz = (z as f64 * frequency).floor() as i32;
        let mut level = None;
        // A caldera can cross the nearest-cell boundary. Inspect every lattice point
        // whose bounded fill can cover this texel, independent of the nearest feature.
        for iz in cz - 1..=cz + 1 {
            for ix in cx - 1..=cx + 1 {
                let surface = self.point_surface(world, x, z, family, cells.feature_point(ix, iz));
                level = level
                    .into_iter()
                    .chain(surface)
                    .max_by_key(|s: &Surface| s.y());
            }
        }
        level
    }

    fn point_surface(
        &self,
        world: &World,
        x: i32,
        z: i32,
        family: Family,
        cell: Cell,
    ) -> Option<Surface> {
        if cell.noise.abs() > 0.5 {
            return None;
        }
        // The original placement casts toward zero, then selects that block's chunk.
        let center = (cell.x as i32, cell.y as i32);
        let min_x = center.0.div_euclid(16).checked_mul(16)?.checked_sub(16)?;
        let min_z = center.1.div_euclid(16).checked_mul(16)?.checked_sub(16)?;
        // Most raster samples exit here, before any biome or terrain sampling.
        if !(0..SPAN as i64).contains(&(i64::from(x) - i64::from(min_x)))
            || !(0..SPAN as i64).contains(&(i64::from(z) - i64::from(min_z)))
        {
            return None;
        }
        let key = (family, center.0, center.1);
        let cached = self.basins.borrow_mut().get(&key).cloned();
        if let Some(basin) = cached {
            return basin.and_then(|basin| basin.at(x, z));
        }
        let biome = world.region.biome_at_quart(center.0 >> 2, center.1 >> 2);
        let rarity = match (family, biome) {
            (Family::Tuya, id::TUYAS) => 2,
            (Family::Tuya, id::ICE_SHEET_TUYAS | id::ICE_SHEET_TUYAS_EDGE) => 3,
            (
                Family::Tuff,
                id::DORMANT_SHIELD_VOLCANO | id::EXTINCT_SHIELD_VOLCANO | id::SUNKEN_SHIELD_VOLCANO,
            ) => 2,
            (Family::Tuff, id::ANCIENT_SHIELD_VOLCANO | id::OLD_SHIELD_VOLCANO_SHORE) => 3,
            (Family::Cinder, id::CANYONS | id::DOLINE_CANYONS) => 6,
            (
                Family::Cinder,
                id::VOLCANIC_MOUNTAINS | id::VOLCANIC_MOUNTAIN_LAKE | id::ACTIVE_SHIELD_VOLCANO,
            ) => 4,
            (
                Family::Cinder,
                id::VOLCANIC_OCEANIC_MOUNTAINS | id::VOLCANIC_OCEANIC_MOUNTAIN_LAKE,
            ) => 2,
            _ => 0,
        };
        let eligible = rarity > 0 && cell.noise.abs() <= 1. / f64::from(rarity);
        let saved = eligible.then(|| saved_basin(world, min_x, min_z)).flatten();
        let complete = saved
            .as_ref()
            .is_some_and(|saved| saved.count == SPAN * SPAN);
        let random = if eligible && !complete && family == Family::Cinder {
            cinder(world, center)
        } else {
            None
        };
        let generated = eligible && !complete && (family == Family::Tuya || random.is_some());
        let mut basin = if generated {
            build_basin(world, center, min_x, min_z, random)
        } else {
            None
        };
        if let Some(saved) = saved {
            let basin = basin.get_or_insert_with(|| Basin {
                min_x,
                min_z,
                surface: vec![None; GRID * GRID],
            });
            for (index, known) in saved.known.into_iter().enumerate() {
                if known {
                    basin.surface[index] = saved.surface[index];
                }
            }
        }
        let basin = basin.map(Arc::new);
        let answer = basin.as_ref().and_then(|basin| basin.at(x, z));
        self.basins.borrow_mut().put(key, basin);
        answer
    }
}

/// Completed saved chunks take precedence over generation estimates. This is independent
/// of the optional block-detail overlay and costs at most nine cached chunk reads per site.
struct SavedCoverage {
    surface: Vec<Option<Surface>>,
    known: Vec<bool>,
    count: usize,
}

fn saved_basin(world: &World, min_x: i32, min_z: i32) -> Option<SavedCoverage> {
    if world.surface_source_id() == 0 {
        return None;
    }
    let mut chunks = std::collections::HashMap::new();
    for cz in min_z.div_euclid(16)..=(min_z + SPAN as i32 - 1).div_euclid(16) {
        for cx in min_x.div_euclid(16)..=(min_x + SPAN as i32 - 1).div_euclid(16) {
            if let Some(chunk) = world.saved_surface(cx, cz) {
                chunks.insert((cx, cz), chunk);
            }
        }
    }
    let mut surface = vec![None; GRID * GRID];
    let mut known = vec![false; GRID * GRID];
    let mut count = 0;
    for z in 0..SPAN {
        for x in 0..SPAN {
            let (wx, wz) = (min_x + x as i32, min_z + z as i32);
            let Some(chunk) = chunks.get(&(wx.div_euclid(16), wz.div_euclid(16))) else {
                continue;
            };
            let Some(column) = chunk
                .columns
                .get((wz.rem_euclid(16) * 16 + wx.rem_euclid(16)) as usize)
            else {
                continue;
            };
            known[(z + 1) * GRID + x + 1] = true;
            count += 1;
            surface[(z + 1) * GRID + x + 1] = Surface::from_block(&column.block, column.y);
        }
    }
    (count > 0).then_some(SavedCoverage {
        surface,
        known,
        count,
    })
}

#[derive(Clone)]
struct Cinder {
    water: bool,
    random: FeatureRandom,
}

fn cinder(world: &World, center: Center) -> Option<Cinder> {
    use crate::vegetation::{
        FeatureRandom, feature_biome_allowed, feature_index_in_step, feature_seed,
    };
    const FEATURE: &str = "tfg:earth/volcano/random_cinder_cone";
    let (cx, cz) = (center.0.div_euclid(16), center.1.div_euclid(16));
    if !feature_biome_allowed(world, cx, cz, FEATURE) {
        return None;
    }
    let Some(index) = feature_index_in_step(FEATURE, 8) else {
        return None;
    };
    let mut random = FeatureRandom::new(feature_seed(cx, cz, -64, 8, index));
    // Installed KubeJS: dead weight 1, active weight 4. Center placement consumes no RNG.
    let water = random.next_int_bound(5) == 0;
    if water {
        dead::advance_dirt(&mut random);
    }
    Some(Cinder { water, random })
}

fn build_basin(
    world: &World,
    center: Center,
    min_x: i32,
    min_z: i32,
    mut cinder: Option<Cinder>,
) -> Option<Basin> {
    let carving = cinder
        .as_mut()
        .map(|cone| rivulet::carve(world, center, &mut cone.random))
        .unwrap_or_default();
    let mut sampled = vec![None; GRID * GRID];
    let mut ground_at = |x: i32, z: i32| -> Option<i32> {
        let gx = x.checked_sub(min_x)?.checked_add(1)?;
        let gz = z.checked_sub(min_z)?.checked_add(1)?;
        if !(0..GRID as i32).contains(&gx) || !(0..GRID as i32).contains(&gz) {
            return None;
        }
        let index = gz as usize * GRID + gx as usize;
        if let Some(height) = sampled[index] {
            return Some(height);
        }
        let height = world.height(x, z).filter(|height| height.is_finite())?;
        let height = carving
            .ground
            .get(&(x, z))
            .copied()
            .unwrap_or(height as i32)
            .clamp(-64, 319);
        sampled[index] = Some(height);
        Some(height)
    };
    let sx = (center.0 - min_x + 1) as usize;
    let sz = (center.1 - min_z + 1) as usize;
    let center_ground = ground_at(center.0, center.1)?;
    let source_id = world.region.biome(center.0, center.1);
    let wet = crate::surface_water::is_surface_water(source_id, Some(f64::from(center_ground)));
    let start_y = if wet {
        (center_ground + 1).max(63)
    } else {
        center_ground.max(11) + 1
    };
    let water = cinder.as_ref().is_some_and(|cone| cone.water);
    let mut fill_ground = |index: usize| -> Option<i32> {
        let x = index % FILL_GRID;
        let z = index / FILL_GRID;
        let wx = min_x.checked_add(x as i32 + 1)?;
        let wz = min_z.checked_add(z as i32 + 1)?;
        let mut height = ground_at(wx, wz)?;
        if water {
            let biome = world.region.biome(wx, wz);
            // Water calderas replace no existing fluids. Treat modeled sea water as
            // a floor so submerged volume cannot satisfy the minimum.
            if crate::surface_water::is_surface_water(biome, world.height(wx, wz)) {
                height = height.max(crate::surface_water::SEA_SURFACE_BLOCK_Y);
            }
        }
        Some(height)
    };
    let filled = fill_columns_lazy(FILL_GRID, (sx - 2, sz - 2), start_y, &mut fill_ground);
    let mut surface = vec![None; GRID * GRID];
    if let Some((level, filled)) = filled {
        for (index, filled) in filled.into_iter().enumerate() {
            if filled {
                surface[(index / FILL_GRID + 2) * GRID + index % FILL_GRID + 2] = Some(if water {
                    Surface::Water(level)
                } else {
                    Surface::Lava(level)
                });
            }
        }
    }
    if let Some(cone) = &mut cinder {
        let mut fluid: Vec<_> = surface.iter().map(|s| s.map(Surface::y)).collect();
        let add = if cone.water {
            sources::add_water
        } else {
            sources::add
        };
        add(
            world,
            center,
            (min_x - 1, min_z - 1),
            GRID,
            &mut ground_at,
            &mut fluid,
            &mut cone.random,
        );
        for (top, fluid) in surface.iter_mut().zip(fluid) {
            if let Some(y) = fluid {
                *top = Some(if cone.water {
                    Surface::Water(y)
                } else {
                    Surface::Lava(y)
                });
            }
        }
        for ((x, z), y) in carving.magma.iter().filter(|_| !cone.water) {
            let (gx, gz) = (x - min_x + 1, z - min_z + 1);
            if gx > 0 && gz > 0 && gx < GRID as i32 - 1 && gz < GRID as i32 - 1 {
                let index = gz as usize * GRID + gx as usize;
                if surface[index].is_none() {
                    surface[index] = Some(Surface::Magma(*y));
                }
            }
        }
    }
    surface.iter().any(Option::is_some).then_some(Basin {
        min_x,
        min_z,
        surface,
    })
}

/// No cinder placement means no post-caldera source predictors need a complete terrain
/// window. Lazily sample only cells visited by minimax spill search and connected fill.
/// Complete height-window implementation retained as a reference for cinder sources and
/// as an output-equivalence oracle for the lazy fill implementation.
#[cfg(test)]
fn build_basin_eager(
    world: &World,
    center: Center,
    min_x: i32,
    min_z: i32,
    mut cinder: Option<Cinder>,
) -> Option<Basin> {
    let carving = cinder
        .as_mut()
        .map(|cone| rivulet::carve(world, center, &mut cone.random))
        .unwrap_or_default();
    let mut ground = Vec::with_capacity(GRID * GRID);
    for gz in 0..GRID {
        for gx in 0..GRID {
            let x = min_x.checked_add(gx as i32 - 1)?;
            let z = min_z.checked_add(gz as i32 - 1)?;
            let height = world.height(x, z).filter(|height| height.is_finite())?;
            // TFG fills y < 1 + max(heightNoise, seaLevel). Above sea level,
            // the ordinary column's highest possible ground block is heightNoise.
            ground.push(
                carving
                    .ground
                    .get(&(x, z))
                    .copied()
                    .unwrap_or(height as i32)
                    .clamp(-64, 319),
            );
        }
    }
    let sx = (center.0 - min_x + 1) as usize;
    let sz = (center.1 - min_z + 1) as usize;
    let center_ground = ground[sz * GRID + sx];
    let source_id = world.region.biome(center.0, center.1);
    let wet = crate::surface_water::is_surface_water(source_id, Some(f64::from(center_ground)));
    let start_y = if wet {
        (center_ground + 1).max(63)
    } else {
        center_ground.max(11) + 1
    };
    let mut surface = vec![None; GRID * GRID];
    let water = cinder.as_ref().is_some_and(|cone| cone.water);
    let mut fill_ground = Vec::with_capacity(FILL_GRID * FILL_GRID);
    for z in 2..GRID - 2 {
        for x in 2..GRID - 2 {
            let mut floor = ground[z * GRID + x];
            if water {
                let (wx, wz) = (min_x + x as i32 - 1, min_z + z as i32 - 1);
                let biome = world.region.biome(wx, wz);
                // Water calderas replace no existing fluids. Treat modeled sea water
                // as a floor, so submerged volume cannot satisfy the 20-block minimum.
                if crate::surface_water::is_surface_water(biome, world.height(wx, wz)) {
                    floor = floor.max(crate::surface_water::SEA_SURFACE_BLOCK_Y);
                }
            }
            fill_ground.push(floor);
        }
    }
    if let Some((level, filled)) = fill_columns(&fill_ground, FILL_GRID, (sx - 2, sz - 2), start_y)
    {
        for (index, filled) in filled.into_iter().enumerate() {
            if filled {
                surface[(index / FILL_GRID + 2) * GRID + index % FILL_GRID + 2] = Some(if water {
                    Surface::Water(level)
                } else {
                    Surface::Lava(level)
                });
            }
        }
    }
    if let Some(cone) = &mut cinder {
        let mut fluid: Vec<_> = surface.iter().map(|s| s.map(Surface::y)).collect();
        let add = if cone.water {
            sources::add_water
        } else {
            sources::add
        };
        let mut eager_ground_at = |x: i32, z: i32| -> Option<i32> {
            let gx = x.checked_sub(min_x)?.checked_add(1)?;
            let gz = z.checked_sub(min_z)?.checked_add(1)?;
            if !(0..GRID as i32).contains(&gx) || !(0..GRID as i32).contains(&gz) {
                return None;
            }
            Some(ground[gz as usize * GRID + gx as usize])
        };
        add(
            world,
            center,
            (min_x - 1, min_z - 1),
            GRID,
            &mut eager_ground_at,
            &mut fluid,
            &mut cone.random,
        );
        for (top, fluid) in surface.iter_mut().zip(fluid) {
            if let Some(y) = fluid {
                *top = Some(if cone.water {
                    Surface::Water(y)
                } else {
                    Surface::Lava(y)
                });
            }
        }
        // Rivulet cuts expose the configured magma block. Caldera water/lava and
        // later lava sources cover it, so only uncovered solid tops remain orange.
        for ((x, z), y) in carving.magma.iter().filter(|_| !cone.water) {
            let (gx, gz) = (x - min_x + 1, z - min_z + 1);
            if gx > 0 && gz > 0 && gx < GRID as i32 - 1 && gz < GRID as i32 - 1 {
                let index = gz as usize * GRID + gx as usize;
                if surface[index].is_none() {
                    surface[index] = Some(Surface::Magma(*y));
                }
            }
        }
    }
    surface.iter().any(Option::is_some).then_some(Basin {
        min_x,
        min_z,
        surface,
    })
}

/// Find the first leaking plane with a minimax path to the outside sample ring.
/// Equal-height cells are solid at that plane, so the last closed lava level is
/// the spill elevation itself. The outside ring matters: the Java feature tests
/// fillability before checking whether the adjacent block is outside its box.
#[cfg(test)]
fn fill_columns(
    ground: &[i32],
    side: usize,
    start: (usize, usize),
    start_y: i32,
) -> Option<(i32, Vec<bool>)> {
    let source = start.1 * side + start.0;
    let mut distance = vec![i32::MAX; ground.len()];
    let mut queue = BinaryHeap::new();
    distance[source] = ground[source];
    queue.push(Reverse((ground[source], source)));
    let level = loop {
        let Reverse((height, index)) = queue.pop()?;
        if distance[index] != height {
            continue;
        }
        let (x, z) = (index % side, index / side);
        if x == 0 || z == 0 || x + 1 == side || z + 1 == side {
            break height;
        }
        for next in [index - 1, index + 1, index - side, index + side] {
            let candidate = height.max(ground[next]);
            if candidate < distance[next] {
                distance[next] = candidate;
                queue.push(Reverse((candidate, next)));
            }
        }
    };
    if level < start_y {
        return None;
    }

    let mut filled = vec![false; ground.len()];
    let mut stack = vec![source];
    filled[source] = true;
    let mut volume = 0_i64;
    while let Some(index) = stack.pop() {
        volume += i64::from(level) - i64::from(ground[index]);
        let (x, z) = (index % side, index / side);
        if x == 0 || z == 0 || x + 1 == side || z + 1 == side {
            return None;
        }
        for next in [index - 1, index + 1, index - side, index + side] {
            if !filled[next] && ground[next] < level {
                filled[next] = true;
                stack.push(next);
            }
        }
    }
    (volume >= 20).then_some((level, filled))
}

/// Exact minimax search and volume flood with memoized, on-demand ground samples.
/// The index ordering, neighbor order and strict `< level` fill match `fill_columns`.
fn fill_columns_lazy(
    side: usize,
    start: (usize, usize),
    start_y: i32,
    ground_at: &mut impl FnMut(usize) -> Option<i32>,
) -> Option<(i32, Vec<bool>)> {
    let source = start.1 * side + start.0;
    let source_ground = ground_at(source)?;
    let mut distance = vec![i32::MAX; side * side];
    let mut queue = BinaryHeap::new();
    distance[source] = source_ground;
    queue.push(Reverse((source_ground, source)));
    let level = loop {
        let Reverse((height, index)) = queue.pop()?;
        if distance[index] != height {
            continue;
        }
        let (x, z) = (index % side, index / side);
        if x == 0 || z == 0 || x + 1 == side || z + 1 == side {
            break height;
        }
        for next in [index - 1, index + 1, index - side, index + side] {
            let candidate = height.max(ground_at(next)?);
            if candidate < distance[next] {
                distance[next] = candidate;
                queue.push(Reverse((candidate, next)));
            }
        }
    };
    if level < start_y {
        return None;
    }

    let mut filled = vec![false; side * side];
    let mut stack = vec![source];
    filled[source] = true;
    let mut volume = 0_i64;
    while let Some(index) = stack.pop() {
        volume += i64::from(level) - i64::from(ground_at(index)?);
        let (x, z) = (index % side, index / side);
        if x == 0 || z == 0 || x + 1 == side || z + 1 == side {
            return None;
        }
        for next in [index - 1, index + 1, index - side, index + side] {
            if !filled[next] && ground_at(next)? < level {
                filled[next] = true;
                stack.push(next);
            }
        }
    }
    (volume >= 20).then_some((level, filled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_cached_surface_matches_direct_queries_across_boundaries_and_seeds() {
        for seed in [0, 1, -918273645] {
            let mut world = World::new(seed, crate::model::Settings::default());
            // Exercise the tuff family gate and absent saved-chunk fallback too.
            if seed == 1 {
                world = world.with_surface_source(
                    Some(std::path::PathBuf::from(
                        "/virtual/volcanic-cache-equivalence",
                    )),
                    1,
                );
            }
            let sampler = Sampler::new(seed);
            for (x, z) in [
                (0, 0),
                (-1, -1),
                (-16, -16),
                (-17, 31),
                (15, 16),
                (16, 15),
                (31, 31),
                (-1456, 40768),
                (12352, -2048),
            ] {
                assert_eq!(
                    sampler.surface(&world, x, z),
                    sampler.direct_surface(&world, x, z),
                    "seed={seed} at {x},{z}"
                );
            }
            if seed == 0 {
                // Full-tile comparisons cover real cinder, tuya, tuff/shield and
                // negative-coordinate chunks. The old per-point implementation is
                // independent of the candidate-union/fill-copy path above.
                for (x, z) in [
                    (-1264, 4689),
                    (-2754, 4513),
                    (5518, -6365),
                    (19624, -3052),
                    (49954, 28225),
                ] as [(i32, i32); 5]
                {
                    let min_x = x.div_euclid(16) * 16;
                    let min_z = z.div_euclid(16) * 16;
                    for dz in 0..16 {
                        for dx in 0..16 {
                            let (sx, sz) = (min_x + dx, min_z + dz);
                            assert_eq!(
                                sampler.surface(&world, sx, sz),
                                sampler.direct_surface(&world, sx, sz),
                                "full tile seed={seed} at {sx},{sz}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn lazy_fill_matches_eager_basin_with_active_and_dead_cinder_sources() {
        let world = World::new(0, crate::model::Settings::default());
        let (mut active_count, mut dead_count) = (0, 0);
        for center in [
            (-1264, 4689),
            (-2754, 4513),
            (5518, -6365),
            (19624, -3052),
            (49954, 28225),
            (-14048, 40773),
        ] as [(i32, i32); 6]
        {
            let (cx, cz) = (center.0.div_euclid(16), center.1.div_euclid(16));
            let (min_x, min_z) = (cx * 16 - 16, cz * 16 - 16);
            let lazy_cinder = cinder(&world, center);
            let eager_cinder = cinder(&world, center);
            if let Some(cone) = &lazy_cinder {
                if cone.water {
                    dead_count += 1;
                } else {
                    active_count += 1;
                }
            }
            let lazy = build_basin(&world, center, min_x, min_z, lazy_cinder);
            let eager = build_basin_eager(&world, center, min_x, min_z, eager_cinder);
            assert_eq!(
                lazy.as_ref().map(|basin| &basin.surface),
                eager.as_ref().map(|basin| &basin.surface),
                "cinder basin at {center:?}"
            );
        }
        assert!(
            active_count > 0,
            "test sites must include active cinder cones"
        );
        assert!(dead_count > 0, "test sites must include dead cinder cones");
    }

    #[test]
    fn closed_bowl_fills_to_its_spill_and_leaves_walls_dry() {
        let mut ground = vec![10; 81];
        for z in 3..=5 {
            for x in 3..=5 {
                ground[z * 9 + x] = 5;
            }
        }
        let (level, fill) = fill_columns(&ground, 9, (4, 4), 6).unwrap();
        assert_eq!(level, 10);
        assert_eq!(fill.iter().filter(|filled| **filled).count(), 9);
        assert!(!fill[4 * 9 + 2]);
    }

    #[test]
    fn an_open_crater_and_a_tiny_puddle_are_rejected() {
        let mut ground = vec![10; 81];
        for x in 0..=4 {
            ground[4 * 9 + x] = 5;
        }
        assert!(fill_columns(&ground, 9, (4, 4), 6).is_none());
        let mut tiny = vec![10; 81];
        tiny[40] = 9;
        assert!(fill_columns(&tiny, 9, (4, 4), 10).is_none());
    }

    #[test]
    fn solid_terrain_just_outside_the_allowed_box_can_contain_lava() {
        let mut ground = vec![8; 49];
        for z in 1..6 {
            for x in 1..6 {
                ground[z * 7 + x] = 2;
            }
        }
        let (level, fill) = fill_columns(&ground, 7, (3, 3), 3).unwrap();
        assert_eq!(level, 8);
        assert_eq!(fill.iter().filter(|filled| **filled).count(), 25);
        assert!(fill_columns(&ground, 7, (3, 3), 63).is_none());
    }
}
