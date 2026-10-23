//! Worker-local TFG samplers reuse shared immutable region data.
use crate::{
    biome,
    dimension::Dimension,
    geology,
    model::{Climate, Point, Probe, Settings, TreeEstimate},
    region::Generator,
};
use std::{cell::RefCell, num::NonZeroUsize};

pub struct World {
    pub seed: i64,
    pub region: Generator,
    pub dimension: Dimension,
    pub slice_y: i32,
    beneath: Option<crate::beneath::Beneath>,
    planetary: Option<crate::planetary::Planetary>,
    planetary_cache: RefCell<lru::LruCache<(i32, i32), u16>>,
    beneath_cache: RefCell<lru::LruCache<(i32, i32, i32), u16>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planetary_probes_do_not_sample_earth_climate_or_geology() {
        for (dimension, range) in [(Dimension::Moon, 2000..2010), (Dimension::Mars, 2100..2114)] {
            let world = World::for_dimension(0, Settings::default(), dimension, 96);
            for (x, z) in [(338, 514), (-622, 82), (842, -270)] {
                let id = world.biome_id(x, z);
                assert!(range.contains(&id));
                let probe = world.sample(x, z, false);
                assert_eq!(probe.biome, world.biome_at(x, 80, z));
                assert!(!probe.ocean);
                assert_eq!(probe.rocks, Probe::default().rocks);
                assert_eq!(probe.climate.rainfall, Climate::default().rainfall);
                assert_eq!(probe.climate.temperature, Climate::default().temperature);
                assert!(world.rock_at(x, 64, z).is_none());
                assert_eq!(world.tree_estimate(&probe, 10_000.).per_hectare, 0.);
            }
        }
    }
}
impl World {
    pub fn new(seed: i64, settings: Settings) -> Self {
        Self::for_dimension(seed, settings, Dimension::Overworld, 64)
    }
    pub fn for_dimension(
        seed: i64,
        settings: Settings,
        dimension: Dimension,
        slice_y: i32,
    ) -> Self {
        Self {
            seed,
            region: Generator::new(seed, settings),
            dimension,
            slice_y: if dimension == Dimension::Beneath {
                dimension.band(slice_y).1
            } else {
                dimension.default_y()
            },
            beneath: (dimension == Dimension::Beneath).then(|| crate::beneath::Beneath::new(seed)),
            planetary: dimension
                .is_planetary()
                .then(|| crate::planetary::Planetary::new(seed, dimension)),
            planetary_cache: RefCell::new(lru::LruCache::new(
                NonZeroUsize::new(if dimension.is_planetary() { 8192 } else { 1 }).unwrap(),
            )),
            beneath_cache: RefCell::new(lru::LruCache::new(
                NonZeroUsize::new(if dimension == Dimension::Beneath {
                    8192
                } else {
                    1
                })
                .unwrap(),
            )),
        }
    }
    pub fn biome_id(&self, x: i32, z: i32) -> u16 {
        if let Some(planetary) = &self.planetary {
            let key = (x >> 2, z >> 2);
            if let Some(id) = self.planetary_cache.borrow_mut().get(&key).copied() {
                return id;
            }
            let id = planetary.biome_id(x, z);
            self.planetary_cache.borrow_mut().put(key, id);
            id
        } else if let Some(beneath) = &self.beneath {
            self.cached_beneath_id(beneath, x, self.dimension.band(self.slice_y).0 + 8, z)
        } else {
            self.region.biome(x, z)
        }
    }
    pub fn biome_at(&self, x: i32, y: i32, z: i32) -> String {
        if let Some(planetary) = &self.planetary {
            crate::planetary::biome_name(planetary.biome_id_at(x, y, z)).to_owned()
        } else if let Some(beneath) = &self.beneath {
            crate::beneath::biome_name(self.cached_beneath_id(beneath, x, y, z)).to_owned()
        } else {
            self.biome(x, z)
        }
    }
    fn cached_beneath_id(&self, beneath: &crate::beneath::Beneath, x: i32, y: i32, z: i32) -> u16 {
        let key = (x >> 2, y >> 2, z >> 2);
        let cached = self.beneath_cache.borrow_mut().get(&key).copied();
        if let Some(id) = cached {
            return id;
        }
        let id = beneath.biome_id(x, y, z);
        self.beneath_cache.borrow_mut().put(key, id);
        id
    }
    #[inline]
    pub fn biome(&self, x: i32, z: i32) -> String {
        biome::biome_name(self.biome_id(x, z)).to_owned()
    }
    pub fn climate(&self, x: i32, z: i32) -> Climate {
        if self.dimension != Dimension::Overworld {
            return Climate::default();
        }
        self.region.climate(x, z)
    }
    pub fn sample(&self, x: i32, z: i32, height: bool) -> Probe {
        if self.dimension != Dimension::Overworld {
            return Probe {
                point: Point {
                    x: x as f64,
                    z: z as f64,
                },
                biome: self.biome(x, z),
                height: height.then(|| self.height(x, z)).flatten(),
                ..Probe::default()
            };
        }
        let id = self.region.biome(x, z);
        let climate = self.region.climate(x, z);
        let rocks = geology::rock_stack(self.region.rock_point_at(x, z));
        let hotspot = self.region.hotspot_at(x, z);
        Probe {
            point: Point {
                x: x as f64,
                z: z as f64,
            },
            biome: biome::biome_name(id).to_owned(),
            ocean: biome::is_ocean(id),
            climate,
            height: if height { self.height(x, z) } else { None },
            forest: self.region.forest_type(x >> 4, z >> 4),
            rocks,
            hotspot: hotspot.0,
            hotspot_age: hotspot.1 as f64,
        }
    }
    pub fn probe(&self, x: i32, z: i32) -> Probe {
        self.sample(x, z, true)
    }
    pub fn height(&self, x: i32, z: i32) -> Option<f64> {
        if let Some(planetary) = &self.planetary {
            return planetary.height(x, z);
        }
        if self.dimension != Dimension::Overworld {
            return None;
        }
        geology::surface_height(&self.region, self.seed, x, z)
    }
    pub fn rock_at(&self, x: i32, y: i32, z: i32) -> Option<String> {
        if self.dimension != Dimension::Overworld {
            return None;
        }
        let h = self.height(x, z)?;
        let (point, layer) = self.region.rock_layer_at(x, y, z, h as i32);
        Some(geology::sample_rock_layer(point, layer))
    }
    pub fn tree_estimate(&self, p: &Probe, area: f64) -> TreeEstimate {
        if self.dimension != Dimension::Overworld {
            return TreeEstimate::default();
        }
        geology::estimate_trees(p, area)
    }
}
