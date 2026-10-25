//! TFG surface openings: terrain/cave density, seeded carvers and aquifer fluids.
//!
//! Raw biome height remains separate. This module resolves physical solid/fluid
//! columns for local map views and placement checks without generating a save.
mod aquifer;
mod carvers;
mod density;
mod simplex3d;
use crate::{
    biome::ids::BIOME_IDS,
    geology::{self, ColumnDensityContext},
    model::Settings,
    noise::OpenSimplex2D,
    world::World,
};
use lru::LruCache;
use serde_json::Value;
use std::{
    cell::RefCell,
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Cell {
    Solid,
    Air,
    Water,
    Lava,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    pub solid_y: i32,
    pub fluid: Option<(Cell, i32)>,
    pub original_solid_y: i32,
    pub surface_builder_y: i32,
}
struct Sampler {
    seed: i64,
    settings: Settings,
    density: density::Density,
    aquifer: aquifer::Aquifer,
    carvers: carvers::Carvers,
    columns: LruCache<(i32, i32), Column>,
    inputs: LruCache<(i32, i32), Arc<Inputs>>,
    river: RiverDensity,
    lake_blob: OpenSimplex2D,
    lake_height: OpenSimplex2D,
    lake_center: OpenSimplex2D,
}
struct Inputs {
    context: ColumnDensityContext,
    raw_height: i32,
    lake_weight: f64,
    lake_height: f64,
    lake_center: f64,
    river_heights: Vec<f64>,
}
thread_local! {static SAMPLER:RefCell<Option<Sampler>>=const{RefCell::new(None)};}
fn with_sampler<T>(world: &World, f: impl FnOnce(&mut Sampler) -> Option<T>) -> Option<T> {
    if world.dimension != crate::dimension::Dimension::Overworld {
        return None;
    }
    SAMPLER.with(|cache| {
        let mut c = cache.borrow_mut();
        if c.as_ref()
            .is_none_or(|s| s.seed != world.seed || s.settings != world.region.settings)
        {
            *c = Some(Sampler::new(world));
        }
        f(c.as_mut().unwrap())
    })
}
/// Physical seed-only column before vegetation and structures.
pub fn column(world: &World, x: i32, z: i32) -> Option<Column> {
    #[cfg(test)]
    crate::world::count_test_cave_query();
    with_sampler(world, |s| s.column(world, x, z))
}
/// Classify a generated terrain voxel; fluids are determined by TFC aquifers.
pub fn state_at(world: &World, x: i32, y: i32, z: i32) -> Option<Cell> {
    with_sampler(world, |s| {
        let i = s.inputs(world, x, z)?;
        let shapes = s.carvers.shapes(world, x.div_euclid(16), z.div_euclid(16));
        Some(s.state(world, x, y, z, &i, &shapes))
    })
}

/// Stage evidence for the independent saved-voxel audit, absent from release UI.
#[cfg(feature = "diagnostics")]
pub fn diagnostic_voxel(world: &World, x: i32, y: i32, z: i32) -> Option<serde_json::Value> {
    with_sampler(world, |s| {
        let i = s.inputs(world, x, z)?;
        let shapes = s.carvers.shapes(world, x.div_euclid(16), z.div_euclid(16));
        let before = s.state(world, x, y, z, &i, &[]);
        let hit = carvers::at(&shapes, world, x, y, z);
        let aquifer_at_zero = s.aquifer.sample(world, x, y, z, 0.);
        let after = s.state(world, x, y, z, &i, &shapes);
        Some(
            serde_json::json!({"before_air_carvers":before,"carver_hit_debug":hit,
            "aquifer_at_zero_density":aquifer_at_zero,"final":after,
            "raw_height":i.raw_height,"river_cave_weight":i.context.initial_cave_weight}),
        )
    })
}
/// Apply physical terrain fluids to a local map sample. Volcanic and saved
/// surfaces retain priority; distant tiles use the inexpensive regional pass.
pub(crate) fn populate(world: &World, x: i32, z: i32, p: &mut crate::model::Probe) {
    populate_with_options(
        world,
        x,
        z,
        p,
        &crate::display::TerrainDetailOptions::full(),
    );
}

pub(crate) fn populate_with_options(
    world: &World,
    x: i32,
    z: i32,
    p: &mut crate::model::Probe,
    options: &crate::display::TerrainDetailOptions,
) {
    if !options.physical_surface || !options.caves || p.ground_saved {
        return;
    }
    if (options.volcanoes || options.springs) && p.volcanic_surface().is_some() {
        return;
    }
    if let Some(c) = column(world, x, z) {
        if options.rivers_lakes {
            // An ocean-labelled source may contain exposed terrain. Physical
            // dryness must also clear Probe's ocean-water fallback.
            p.ocean &= c.fluid.is_some_and(|(cell, _)| cell == Cell::Water);
            p.surface_water = false;
            p.surface_water_y = None;
            p.surface_lava = None;
            if let Some((fluid, y)) = c.fluid {
                if options.large_rocks_special_soils
                    && let Some(rock) = crate::ground::rock_above_fluid(world, x, z, y)
                {
                    p.ocean = false;
                    p.ground_y = Some(rock.y);
                    p.ground_block = Some(rock.block.to_string());
                    if p.height.is_some() {
                        p.height = Some(f64::from(rock.y))
                    }
                    return;
                }
                match fluid {
                    Cell::Water => {
                        p.surface_water = true;
                        p.surface_water_y = Some(y)
                    }
                    Cell::Lava => p.surface_lava = Some(y),
                    _ => {}
                }
            }
        }
        if p.height.is_some() {
            p.height = Some(f64::from(c.solid_y))
        }
    }
}
impl Sampler {
    fn new(world: &World) -> Self {
        let field = |seed, octaves, spread, min, max| {
            let mut n = OpenSimplex2D::new(seed);
            n.octaves(octaves).spread(spread).scaled(min, max);
            n
        };
        Self {
            seed: world.seed,
            settings: world.region.settings.clone(),
            density: density::Density::new(world.seed),
            aquifer: aquifer::Aquifer::new(world.seed),
            carvers: carvers::Carvers::new(world.seed),
            columns: LruCache::new(NonZeroUsize::new(131_072).unwrap()),
            inputs: LruCache::new(NonZeroUsize::new(8192).unwrap()),
            river: RiverDensity::new(world.seed),
            lake_blob: field(world.seed.wrapping_add(1), 1, f64::from(0.04_f32), -1., 1.),
            lake_height: field(world.seed.wrapping_add(2), 4, f64::from(0.2_f32), 2., 18.),
            lake_center: field(world.seed.wrapping_add(3), 2, f64::from(0.06_f32), 59., 67.),
        }
    }
    fn inputs(&mut self, world: &World, x: i32, z: i32) -> Option<Arc<Inputs>> {
        if let Some(i) = self.inputs.get(&(x, z)) {
            return Some(i.clone());
        }
        let context = geology::column_density_context(&world.region, world.seed, x, z)?;
        let lake_weight = context
            .biomes
            .iter()
            .filter(|(id, _)| underground_lakes(*id))
            .map(|(_, w)| w)
            .sum();
        let lake_height = ((f64::from(0.7_f32) - self.lake_blob.sample(x as f64, z as f64).abs())
            * f64::from(1_f32 / 0.3_f32))
        .clamp(0., 1.)
            * self.lake_height.sample(x as f64, z as f64);
        let lake_center = self.lake_center.sample(x as f64, z as f64);
        let river_heights = self.river.height.samples(&context, x, z);
        let i = Arc::new(Inputs {
            context,
            raw_height: world.height(x, z)? as i32,
            lake_weight,
            lake_height,
            lake_center,
            river_heights,
        });
        self.inputs.put((x, z), i.clone());
        Some(i)
    }
    fn state(
        &mut self,
        world: &World,
        x: i32,
        y: i32,
        z: i32,
        i: &Inputs,
        shapes: &[carvers::Shape],
    ) -> Cell {
        if y > i.raw_height.max(63) {
            return Cell::Air;
        }
        if y < -64 {
            return Cell::Solid;
        }
        let initial = i.lake_weight
            * (f64::from(0.4_f32)
                + f64::from(0.05_f32) * (i.lake_height - (i.lake_center - y as f64).abs()))
            .clamp(0., 1.);
        // TFG applies both the river and shore sums. The default shore sampler passes
        // the initial biome noise through, including when a river cave transforms it.
        let mut noise = self.river.noise(i, x, y, z, initial) + initial;
        noise = 0.4 - noise;
        if y > i.raw_height {
            noise -= f64::from(y - i.raw_height) * f64::from(0.2_f32);
        }
        let density = self.density.sample(x, y, z, noise.clamp(-1., 1.));
        let mut cell = if density > 0. {
            Cell::Solid
        } else {
            self.aquifer.sample(world, x, y, z, density)
        };
        if cell == Cell::Solid
            && let Some(debug) = carvers::at(shapes, world, x, y, z)
        {
            let c = if y <= if debug { -64 } else { -56 } {
                Cell::Lava
            } else {
                self.aquifer.sample(world, x, y, z, 0.)
            };
            cell = if debug && c == Cell::Lava {
                Cell::Solid
            } else {
                c
            };
        }
        cell
    }
    fn column(&mut self, world: &World, x: i32, z: i32) -> Option<Column> {
        if let Some(c) = self.columns.get(&(x, z)) {
            return Some(*c);
        }
        let i = self.inputs(world, x, z)?;
        let original = crate::surface_water::ordinary_solid_top(i.raw_height as f64)?;
        let shapes = self
            .carvers
            .shapes(world, x.div_euclid(16), z.div_euclid(16));
        let mut surface_builder_y = -64;
        let mut fluid = None;
        for y in (-64..=original.max(62)).rev() {
            match self.state(world, x, y, z, &i, &[]) {
                Cell::Solid => {
                    surface_builder_y = y;
                    break;
                }
                Cell::Air => {}
                cell => {
                    if fluid.is_none() {
                        fluid = Some((cell, y))
                    }
                }
            }
        }
        // Carvers only replace solid cells. If the first solid cell is outside
        // all trajectories, the first pass already resolved the visible floor.
        let mut solid_y = surface_builder_y;
        if carvers::at(&shapes, world, x, surface_builder_y, z).is_some() {
            for y in (-64..=surface_builder_y).rev() {
                match self.state(world, x, y, z, &i, &shapes) {
                    Cell::Solid => {
                        solid_y = y;
                        break;
                    }
                    Cell::Air => {}
                    cell => {
                        if fluid.is_none() {
                            fluid = Some((cell, y))
                        }
                    }
                }
            }
        }
        let c = Column {
            solid_y,
            fluid,
            original_solid_y: original,
            surface_builder_y,
        };
        self.columns.put((x, z), c);
        Some(c)
    }
}
fn underground_lakes(id: u16) -> bool {
    static IDS: OnceLock<Vec<bool>> = OnceLock::new();
    IDS.get_or_init(|| {
        let v: Value = serde_json::from_str(include_str!("caves/assets/biomes.json")).unwrap();
        BIOME_IDS
            .iter()
            .map(|id| {
                v[id.rsplit('/').next().unwrap()]["underground_lakes"]
                    .as_bool()
                    .unwrap_or(false)
            })
            .collect()
    })
    .get(id as usize)
    .copied()
    .unwrap_or(false)
}
struct RiverDensity {
    height: RiverHeights,
    center: OpenSimplex2D,
    thickness: OpenSimplex2D,
    cave: usize,
    none: usize,
    tall: usize,
    tall_dist: OpenSimplex2D,
    tall_cliff: simplex3d::Simplex3,
}
struct RiverHeights {
    inner: crate::geology::river_height::RiverHeightSamplers,
}
impl RiverHeights {
    fn samples(&self, c: &ColumnDensityContext, x: i32, z: i32) -> Vec<f64> {
        (0..self.inner.count())
            .map(|k| {
                c.river.as_ref().map_or(c.pre_river_height, |info| {
                    self.inner.sample(
                        k,
                        info,
                        x as f64,
                        z as f64,
                        c.pre_river_height,
                        c.initial_cave_weight,
                        c.river_weights[k],
                    )
                })
            })
            .collect()
    }
}
impl RiverDensity {
    fn new(seed: i64) -> Self {
        // Preserve the seed stream shared by TFGRiverBlendType.ALL. Height samplers
        // already implement every profile's consumption, including the unused 3D cliff.
        let (center_seed, height_seed) = crate::geology::river_height::cave_noise_seeds(seed);
        let mut center = OpenSimplex2D::new(center_seed);
        center
            .octaves(2)
            .spread(f64::from(0.02_f32))
            .scaled(60., 66.);
        let mut thickness = OpenSimplex2D::new(height_seed);
        thickness
            .octaves(4)
            .spread(f64::from(0.15_f32))
            .scaled(8., 14.);
        let (ds, cs) = crate::geology::river_height::tall_canyon_noise_seeds(seed);
        let mut tall_dist = OpenSimplex2D::new(ds);
        tall_dist
            .octaves(4)
            .spread(f64::from(0.05_f32))
            .scaled(f64::from(-0.3_f32), f64::from(0.2_f32));
        let tall_cliff = simplex3d::Simplex3::new(cs);
        let inner = crate::geology::river_height::RiverHeightSamplers::new(seed);
        let tall = inner.type_for("whorled_canyons");
        let cave = inner.cave_index();
        let none = inner.none_index();
        Self {
            height: RiverHeights { inner },
            center,
            thickness,
            cave,
            none,
            tall,
            tall_dist,
            tall_cliff,
        }
    }
    fn noise(&self, i: &Inputs, x: i32, y: i32, z: i32, initial: f64) -> f64 {
        let Some(info) = i.context.river.as_ref() else {
            return initial;
        };
        let mut out = 0.;
        for k in 0..i.river_heights.len() {
            let w = i.context.river_weights[k];
            if w <= 0. || !w.is_finite() {
                continue;
            }
            let n = if k == self.none {
                initial
            } else if k == self.tall {
                let distance = (info.dist_sq_blocks / info.width_sq * 1.3
                    + self.tall_dist.sample(x as f64, z as f64))
                .max(0.)
                    * (0.7 + ((info.width_sq - 144.) / (324. - 144.)).clamp(0., 1.) * (1.1 - 0.7));
                let cliff = self.tall_cliff.sample(x as f64, y as f64, z as f64);
                let raw = if y > 98 {
                    0.
                } else if y > 83 {
                    f64::from(1_f32 - (y - 83) as f32 / 15_f32) * cliff
                } else if y > 63 {
                    cliff
                } else if y > 55 {
                    (y - 55) as f64 / 8. * cliff
                } else {
                    0.
                };
                raw + (initial - raw) * distance.clamp(0., 1.)
            } else if k == self.cave {
                let weight = i.context.initial_cave_weight;
                let mut d = (y as f64 - self.center.sample(x as f64, z as f64))
                    / self.thickness.sample(x as f64, z as f64);
                if d > 0. {
                    d = d * weight * weight;
                }
                let d2 = (info.dist_sq_blocks / info.width_sq * 1.3 - 0.1).clamp(0., 1.);
                let a = (1. - d * d).max(0.);
                (a + d2 * (initial - a)) * (1. - ((weight - 0.5) / (0.25 - 0.5)).clamp(0., 1.))
            } else if y as f64 > i.river_heights[k] {
                0.
            } else {
                initial
            };
            out += w * n;
        }
        out
    }
}
