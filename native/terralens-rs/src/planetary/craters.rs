//! Seeded surface carving from TFG's Moon crater geode features.
//!
//! The Moon's crater features are ordinary Minecraft `GeodeFeature`s with air for every layer.
//! Their placer and all geometric parameters are taken from the installed datapack and Minecraft
//! 1.20.1. This module keeps crater sampling separate from terrain density evaluation so callers
//! can supply pre-decoration surface heights. Candidate volumes are clipped to source chunks in
//! the target chunk's 3x3 `WorldGenRegion` write radius. Later features in a source chunk use the
//! surface left by earlier sorted feature indices.
//!
//! Installed TFG source provenance (SHA-256, relative to `kubejs/`):
//! * configured crater wrappers: `data/tfg/worldgen/configured_feature/moon/crater/extra_large.json`
//!   f67e16c1abec38cb7d924d44cbff5a4289ced8d9ae47a35dacdc8e3624561bef; `large.json`
//!   5c00b1f675fab6adf3bf7c2bfd6116ac6bc2b10f99ce51269730e25338c89b05; `medium.json`
//!   914e22d23597d6cc80488359e25a3568772aa2b4d6c62117c6eb1b48246310cb; `small.json`
//!   d3f193746e5ff51ee306f1659d42550c50c486eb0e8c42508508803a6627c085.
//! * wrapped geode configurations: `extra_large_bowl.json` 8fdf1d3bd7db4e553a326b6e6822d71f7b3a9e04f4415bc9ab4e6f1edfc32e96;
//!   `large_bowl.json` 73a0b0c9a56ed30a81bf5aac6ec7d1e87f0c310aa3f5864ed887f73b86730624.
//! * placed feature values: `data/tfg/worldgen/placed_feature/moon/crater/extra_large.json`
//!   d6500c1630fc1fe86572e62b2a5ab9463785cc4cbf9bd47ddcf63f1d36936939; `large.json`
//!   47539f350050c6fa253b74aadd7d4ad6ad100a633e2946b06f7f5fc4b240ea8c; `medium.json`
//!   aad548a62f4ff1181c0fee331d578f85b94601dc553da10fc2f8c6ec7cdd0f81; `small.json`
//!   d26acbe6cf3a0b542db41b6c2b552aaad2c6302ebac989349568dbbd92db5d22.
//! * feature order: `server_scripts/tfg/moon/tags.moon.js`
//!   4fe8730a6d0d0d6440499fee165da111d1a286d23d442cc1332ded96f5a50945 (extra_large, large,
//!   medium, small); the Moon biome JSONs put this tag at decoration step 1 except
//!   `lunar_chorus_dense`, whose step-1 feature list is empty.

use crate::vegetation::FeatureRandom;
use lru::LruCache;
use std::{num::NonZeroUsize, sync::Mutex};

const PLACEMENT_CACHE_CAPACITY: usize = 4096;

#[derive(Clone, Copy, Debug)]
struct Crater {
    x: i32,
    y: i32,
    z: i32,
    min: i32,
    max: i32,
    points: [(i32, i32, i32, i32); 4],
    point_count: usize,
    threshold: f64,
}

/// Applies the four placed crater geodes around one sampled column.
pub(super) struct Craters {
    seed: i64,
    cache: Mutex<LruCache<(i32, i32), [Option<Crater>; 4]>>,
}

/// Candidate crater volumes that can affect a single map column.
pub(super) struct CraterColumn {
    x: i32,
    z: i32,
    craters: Vec<Crater>,
}

impl Craters {
    pub(super) fn new(seed: i64) -> Self {
        Self {
            seed,
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(PLACEMENT_CACHE_CAPACITY).unwrap(),
            )),
        }
    }

    /// Prepare the exact placed crater candidates which could intersect one column. `eligible`
    /// should match the crater feature's biome-decoration membership for the source chunk.
    pub(super) fn column(
        &self,
        x: i32,
        z: i32,
        surface: &impl Fn(i32, i32) -> Option<i32>,
        eligible: &impl Fn(i32, i32) -> bool,
    ) -> CraterColumn {
        // WorldGenRegion rejects setBlock outside its configured write radius of one chunk.
        // Crater geometries extend up to 48 blocks, but only placements from the target chunk's
        // 3x3 generation neighborhood can affect this saved column.
        let target_chunk_x = x.div_euclid(16);
        let target_chunk_z = z.div_euclid(16);
        let min_chunk_x = x.saturating_sub(48).div_euclid(16).max(target_chunk_x - 1);
        let max_chunk_x = x.saturating_add(48).div_euclid(16).min(target_chunk_x + 1);
        let min_chunk_z = z.saturating_sub(48).div_euclid(16).max(target_chunk_z - 1);
        let max_chunk_z = z.saturating_add(48).div_euclid(16).min(target_chunk_z + 1);
        let mut craters = Vec::new();
        for cz in min_chunk_z..=max_chunk_z {
            for cx in min_chunk_x..=max_chunk_x {
                craters.extend(
                    self.placements(cx, cz, surface, eligible)
                        .into_iter()
                        .flatten(),
                );
            }
        }
        CraterColumn { x, z, craters }
    }

    fn placements(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        surface: &impl Fn(i32, i32) -> Option<i32>,
        eligible: &impl Fn(i32, i32) -> bool,
    ) -> [Option<Crater>; 4] {
        let key = (chunk_x, chunk_z);
        if let Ok(mut cache) = self.cache.lock() {
            if let Some(placements) = cache.get(&key) {
                return *placements;
            }
        }
        let eligible = eligible(chunk_x, chunk_z);
        let mut placements = [None; 4];
        let mut prior = Vec::with_capacity(3);
        if eligible {
            for (index, config) in [
                (35, 8, -48, 48, 30.0), // tfg:moon/crater/extra_large
                (25, 6, -32, 32, 20.0), // tfg:moon/crater/large
                (20, 2, -32, 32, 10.0), // tfg:moon/crater/medium
                (15, -1, -16, 16, 5.0), // tfg:moon/crater/small
            ]
            .into_iter()
            .enumerate()
            {
                // ChunkGenerator executes each sorted feature index immediately after the prior
                // one. The later HeightmapPlacement therefore sees air carved by earlier crater
                // geodes from this same source chunk.
                let current = self.place(chunk_x, chunk_z, index, config, &|x, z| {
                    let top = surface(x, z)?;
                    let mut y = top;
                    while y > top.saturating_sub(96) {
                        if prior.iter().any(|crater: &Crater| crater.carves(x, y, z)) {
                            y = y.saturating_sub(1);
                        } else {
                            break;
                        }
                    }
                    Some(y)
                });
                if let Some(crater) = current {
                    prior.push(crater);
                    placements[index] = Some(crater);
                }
            }
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(key, placements);
        }
        placements
    }

    fn place(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        feature_index: usize,
        (chance, y_offset, min, max, outer_layer): (u32, i32, i32, i32, f64),
        surface: &impl Fn(i32, i32) -> Option<i32>,
    ) -> Option<Crater> {
        let x = chunk_x * 16;
        let z = chunk_z * 16;
        let mut decorator = FeatureRandom::new(self.seed);
        let x_multiplier = decorator.next_long() as u64 | 1;
        let z_multiplier = decorator.next_long() as u64 | 1;
        let decoration_seed = (self.seed as u64)
            ^ (x as u64)
                .wrapping_mul(x_multiplier)
                .wrapping_add((z as u64).wrapping_mul(z_multiplier));
        // The crater tag is the sole Moon feature set in decoration step 1. FeatureSorter keeps
        // its tag's four placed-feature order, giving stable indices 0..3.
        let feature_seed = decoration_seed
            .wrapping_add(feature_index as u64)
            .wrapping_add(10_000);
        let mut random = FeatureRandom::new(feature_seed as i64);
        // `RarityFilter` consumes a float and accepts values below 1/chance.
        if random.next_float() as f64 >= 1.0 / chance as f64 {
            return None;
        }

        // HeightmapPlacement returns the first motion-blocking Y, one above the top solid block.
        // The codec decodes each JSON integer as ConstantInt. ConstantInt::sample consumes no
        // random values, so x/z offsets are exactly zero and the signed y_spread is a direct
        // offset (including the small crater's -1).
        let y = surface(x, z)?.checked_add(1)?.checked_add(y_offset)?;
        let count = 3 + random.next_int_bound(2) as usize; // distribution_points: uniform 3..4
        let factor = count as f64 / 4.0; // outer_wall_distance.min()
        let mut points = [(0, 0, 0, 0); 4];
        for point in points.iter_mut().take(count) {
            let dx = 4 + random.next_int_bound(3); // outer_wall_distance: uniform 4..6
            let dy = 4 + random.next_int_bound(3);
            let dz = 4 + random.next_int_bound(3);
            let offset = 1 + random.next_int_bound(2); // point_offset: uniform 1..2
            *point = (x + dx, y + dy, z + dz, offset);
        }
        // GeodeFeature aborts only after invalid_blocks_threshold (4) is exceeded; the configured
        // distribution count is at most 4, so the guard cannot reject these crater features.
        Some(Crater {
            x,
            y,
            z,
            min,
            max,
            points,
            point_count: count,
            threshold: 1.0 / (outer_layer + factor).sqrt(),
        })
    }
}

impl Crater {
    #[inline]
    fn carves(self, x: i32, y: i32, z: i32) -> bool {
        let (dx, dy, dz) = (x - self.x, y - self.y, z - self.z);
        if dx < self.min
            || dx > self.max
            || dy < self.min
            || dy > self.max
            || dz < self.min
            || dz > self.max
        {
            return false;
        }
        let influence: f64 = self.points[..self.point_count]
            .iter()
            .map(|&(px, py, pz, weight)| {
                1.0 / (((x - px).pow(2) + (y - py).pow(2) + (z - pz).pow(2) + weight) as f64).sqrt()
            })
            .sum();
        influence >= self.threshold
    }
}

impl CraterColumn {
    #[inline]
    pub(super) fn carves(&self, y: i32) -> bool {
        self.craters
            .iter()
            .any(|crater| crater.carves(self.x, y, self.z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_place(
        seed: i64,
        chunk: (i32, i32),
        index: usize,
        config: (u32, i32, i32, i32, f64),
        surface_y: i32,
        expected_origin: (i32, i32, i32),
        expected_points: &[(i32, i32, i32, i32)],
    ) {
        let crater = Craters::new(seed)
            .place(chunk.0, chunk.1, index, config, &|_, _| Some(surface_y))
            .expect("oracle candidate passes rarity filter");
        assert_eq!((crater.x, crater.y, crater.z), expected_origin);
        assert_eq!(
            &crater.points[..crater.point_count],
            expected_points,
            "Minecraft 1.20.1 WorldgenRandom / GeodeFeature candidate stream"
        );
    }

    #[test]
    fn crater_candidates_match_minecraft_1201_worldgenrandom_oracle() {
        // Values independently emitted by the installed 1.20.1 client classes
        // WorldgenRandom + XoroshiroRandomSource for seed 0, decoration step 1.
        check_place(
            0,
            (-91, -100),
            0,
            (35, 8, -48, 48, 30.0),
            100,
            (-1456, 109, -1600),
            &[
                (-1452, 113, -1595, 2),
                (-1450, 113, -1596, 2),
                (-1450, 114, -1596, 1),
                (-1452, 114, -1596, 2),
            ],
        );
        check_place(
            0,
            (-100, -100),
            1,
            (25, 6, -32, 32, 20.0),
            50,
            (-1600, 57, -1600),
            &[
                (-1596, 62, -1594, 2),
                (-1595, 63, -1595, 2),
                (-1595, 63, -1595, 1),
            ],
        );
        check_place(
            0,
            (-94, -100),
            2,
            (20, 2, -32, 32, 10.0),
            50,
            (-1504, 53, -1600),
            &[
                (-1498, 57, -1596, 2),
                (-1499, 59, -1595, 2),
                (-1500, 58, -1594, 1),
            ],
        );
        check_place(
            0,
            (-91, -100),
            3,
            (15, -1, -16, 16, 5.0),
            50,
            (-1456, 50, -1600),
            &[
                (-1451, 55, -1595, 1),
                (-1451, 56, -1596, 1),
                (-1452, 54, -1596, 2),
                (-1450, 55, -1595, 2),
            ],
        );
    }

    #[test]
    fn crater_mask_uses_geode_feature_inclusive_bounds_and_inverse_distance() {
        let crater = Crater {
            x: 0,
            y: 0,
            z: 0,
            min: -2,
            max: 2,
            points: [(0, 0, 0, 1); 4],
            point_count: 1,
            threshold: 0.4,
        };
        assert!(crater.carves(0, 0, 0));
        assert!(!crater.carves(3, 0, 0));
        assert!(crater.carves(-2, 0, 0));
    }

    #[test]
    fn eligibility_and_placement_cache_is_chunk_scoped_and_clips_write_radius() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let crater_sampler = Craters::new(0);
        let eligibility_calls = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&eligibility_calls);
        let eligible = move |_, _| {
            calls.fetch_add(1, Ordering::Relaxed);
            true
        };
        let first = crater_sampler.column(0, 0, &|_, _| Some(100), &eligible);
        assert_eq!(eligibility_calls.load(Ordering::Relaxed), 9);
        assert!(first.craters.iter().all(|crater| {
            (-1..=1).contains(&crater.x.div_euclid(16))
                && (-1..=1).contains(&crater.z.div_euclid(16))
        }));

        let _same_chunk = crater_sampler.column(15, 15, &|_, _| Some(100), &eligible);
        assert_eq!(eligibility_calls.load(Ordering::Relaxed), 9);

        let disabled_sampler = Craters::new(0);
        let surface_calls = AtomicUsize::new(0);
        let disabled = disabled_sampler.column(
            0,
            0,
            &|_, _| {
                surface_calls.fetch_add(1, Ordering::Relaxed);
                Some(100)
            },
            &|_, _| false,
        );
        assert!(disabled.craters.is_empty());
        assert_eq!(surface_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn later_feature_heightmap_sees_prior_geode_in_the_same_source_chunk() {
        let terrain = super::super::terrain::Terrain::new(0, crate::dimension::Dimension::Moon);
        let crater_sampler = Craters::new(0);
        let placements = crater_sampler.placements(
            21,
            39,
            &|x, z| terrain.base_height(x, z).map(|y| y as i32),
            &|_, _| true,
        );
        let medium = placements[2].expect("medium crater passes the seed-zero rarity filter");
        let small = placements[3].expect("small crater passes the seed-zero rarity filter");

        // The medium geode cuts its source column, so the following small feature samples a
        // lower MOTION_BLOCKING heightmap at the same chunk origin.
        assert_eq!(terrain.base_height(336, 624), Some(106.0));
        assert!(medium.carves(336, 106, 624));
        assert!(small.y < 106);
    }
}
