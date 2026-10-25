//! TFC aquifer cells, including local ceilings and pressure barriers.
use super::Cell;
use crate::{
    planetary::noise::{self, NormalNoise},
    random::Xoroshiro,
    world::World,
};
use lru::LruCache;
use std::{num::NonZeroUsize, sync::OnceLock};
#[derive(Clone, Copy)]
struct Entry {
    fluid: Cell,
    level: i32,
}
impl Entry {
    fn at(self, y: i32) -> Cell {
        if y < self.level {
            self.fluid
        } else {
            Cell::Air
        }
    }
}
pub(super) struct Aquifer {
    lo: u64,
    hi: u64,
    cell_seed: i64,
    cellular: Cellular,
    barrier: NormalNoise,
    ceilings: LruCache<(i32, i32), [i32; 16]>,
    positions: LruCache<(i32, i32, i32), [i32; 3]>,
    entries: LruCache<(i32, i32, i32, i32, i32), Entry>,
}
impl Aquifer {
    pub fn new(seed: i64) -> Self {
        let (lo, hi) = noise::root_positional(seed);
        let mut r = noise::named_source(lo, hi, "aquifer_fluid_cell_noise");
        let cell_seed = r.next_long();
        let cellular = Cellular::new(r.next_long());
        Self {
            lo,
            hi,
            cell_seed,
            cellular,
            barrier: NormalNoise::new(seed, "minecraft:aquifer_barrier", -3, &[1.]),
            ceilings: LruCache::new(NonZeroUsize::new(128).unwrap()),
            positions: LruCache::new(NonZeroUsize::new(8192).unwrap()),
            entries: LruCache::new(NonZeroUsize::new(8192).unwrap()),
        }
    }
    pub fn sample(&mut self, world: &World, x: i32, y: i32, z: i32, density: f64) -> Cell {
        if density > 0. {
            return Cell::Solid;
        }
        if y < -54 {
            return Cell::Lava;
        }
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        let (gx, gy, gz) = (
            (x - 5).div_euclid(16),
            y.div_euclid(12),
            (z - 5).div_euclid(16),
        );
        let mut near = [(i32::MAX, [0; 3]); 3];
        for dx in 0..=1 {
            for dy in -1..=1 {
                for dz in 0..=1 {
                    let key = (gx + dx, gy + dy, gz + dz);
                    let pos = if let Some(p) = self.positions.get(&key) {
                        *p
                    } else {
                        let mut r = positional(self.lo, self.hi, key.0, key.1, key.2);
                        let p = [
                            key.0 * 16 + r.next_int_bound(10) + 3,
                            key.1 * 12 + r.next_int_bound(8) + 2,
                            key.2 * 16 + r.next_int_bound(10) + 3,
                        ];
                        self.positions.put(key, p);
                        p
                    };
                    let d = (pos[0] - x).pow(2) + (pos[1] - y).pow(2) + (pos[2] - z).pow(2);
                    if d <= near[0].0 {
                        near[2] = near[1];
                        near[1] = near[0];
                        near[0] = (d, pos)
                    } else if d <= near[1].0 {
                        near[2] = near[1];
                        near[1] = (d, pos)
                    } else if d <= near[2].0 {
                        near[2] = (d, pos)
                    }
                }
            }
        }
        let entries = near.map(|(_, p)| self.entry(world, cx, cz, p));
        let sim = |a: usize, b: usize| 1. - (near[b].0 - near[a].0).abs() as f64 / 25.;
        let mut contribution = 0.;
        if sim(0, 1) > -1. {
            let mut barrier = None;
            let p01 = self.pressure(x, y, z, &mut barrier, entries[0], entries[1]);
            let p02 = self.pressure(x, y, z, &mut barrier, entries[0], entries[2]);
            let p12 = self.pressure(x, y, z, &mut barrier, entries[1], entries[2]);
            contribution = (2.
                * sim(0, 1).max(0.)
                * p01.max((p02 * sim(0, 2).max(0.)).max(p12 * sim(1, 2).max(0.))))
            .max(0.);
        }
        if density + contribution <= 0. {
            entries[0].at(y)
        } else {
            Cell::Solid
        }
    }
    fn ceiling(&mut self, world: &World, cx: i32, cz: i32, p: [i32; 3]) -> i32 {
        let key = (cx, cz);
        if self.ceilings.peek(&key).is_none() {
            let mut h = [0.; 121];
            for ix in 0..11 {
                for iz in 0..11 {
                    let (x, z) = (cx * 16 - 32 + ix as i32 * 8, cz * 16 - 32 + iz as i32 * 8);
                    let mut v = crate::geology::aquifer_height(&world.region, world.seed, x, z)
                        .unwrap_or(47.);
                    let edges = world.region.river_edges_at(x, z);
                    if v > 39.
                        && crate::rivers::sample_river_edge(&edges, x as f64, z as f64)
                            .is_some_and(|r| r.dist_sq_blocks < 225.)
                    {
                        v = 39.;
                    }
                    if v > 63. {
                        v = 0.3 * 63. + 0.7 * v;
                    }
                    h[ix + iz * 11] = v;
                }
            }
            let a = std::array::from_fn(|i| {
                let (ix, iz) = ((1 + i % 4) * 2, (1 + i / 4) * 2);
                let mut min = f64::MAX;
                for dx in 0..5 {
                    for dz in 0..5 {
                        min = min.min(h[ix + dx - 2 + (iz + dz - 2) * 11]);
                    }
                }
                min as i32
            });
            self.ceilings.put(key, a);
        }
        let ix = (p[0] - cx * 16 + 16).div_euclid(16).clamp(0, 3) as usize;
        let iz = (p[2] - cz * 16 + 16).div_euclid(16).clamp(0, 3) as usize;
        self.ceilings.get(&key).unwrap()[ix + iz * 4]
    }
    fn entry(&mut self, world: &World, cx: i32, cz: i32, p: [i32; 3]) -> Entry {
        let key = (cx, cz, p[0], p[1], p[2]);
        if let Some(e) = self.entries.get(&key) {
            return *e;
        }
        let ceiling = self.ceiling(world, cx, cz, p);
        let e = if p[1] >= ceiling {
            Entry {
                fluid: Cell::Water,
                level: 63,
            }
        } else {
            let (cell_noise, cell_y) =
                self.cellular
                    .cell(p[0] as f64, f64::from(p[1] as f32 / 0.6_f32), p[2] as f64);
            let (n, cy) = (cell_noise as f32, cell_y as f32);
            if n < 0.25 || cy > 53. && n < 0.5 {
                Entry {
                    fluid: Cell::Water,
                    level: -65,
                }
            } else {
                let mut r = Xoroshiro::from_state(
                    self.cell_seed as u64,
                    i64::from(n.to_bits() as i32) as u64,
                );
                let level =
                    ((r.next_float() - r.next_float() - 2.) * 5. + cy).min(ceiling as f32) as i32;
                let lava = cy < 40. && r.next_int_bound(3) == 0;
                Entry {
                    fluid: if lava { Cell::Lava } else { Cell::Water },
                    level,
                }
            }
        };
        self.entries.put(key, e);
        e
    }
    fn pressure(
        &self,
        x: i32,
        y: i32,
        z: i32,
        barrier: &mut Option<f64>,
        a: Entry,
        b: Entry,
    ) -> f64 {
        if matches!(
            (a.at(y), b.at(y)),
            (Cell::Water, Cell::Lava) | (Cell::Lava, Cell::Water)
        ) {
            return 1.;
        }
        let delta = (a.level - b.level).abs();
        if delta == 0 {
            return 0.;
        }
        let above = y as f64 + 0.5 - 0.5 * (a.level + b.level) as f64;
        let near = 0.5 * delta as f64 - above.abs();
        let p = if above > 0. {
            near / if near > 0. { 1.5 } else { 2.5 }
        } else {
            (near + 3.) / if near > -3. { 3. } else { 10. }
        };
        if !(-2. ..=2.).contains(&p) {
            return p;
        }
        *barrier.get_or_insert_with(|| self.barrier.sample(x as f64, y as f64 * 0.5, z as f64)) + p
    }
}
pub(super) fn positional(lo: u64, hi: u64, x: i32, y: i32, z: i32) -> Xoroshiro {
    // Mth.getSeed promotes X's int multiplication only after it has overflowed.
    let mut k = i64::from(x.wrapping_mul(3_129_871))
        ^ i64::from(z).wrapping_mul(116_129_781)
        ^ i64::from(y);
    k = k
        .wrapping_mul(k)
        .wrapping_mul(42_317_861)
        .wrapping_add(k.wrapping_mul(11));
    Xoroshiro::from_state(lo ^ ((k >> 16) as u64), hi)
}
struct Cellular {
    seed: i32,
}
impl Cellular {
    fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ (seed as u64 >> 32) as i64) as i32,
        }
    }
    fn cell(&self, x: f64, y: f64, z: f64) -> (f64, f64) {
        static V: OnceLock<Vec<f32>> = OnceLock::new();
        let v = V.get_or_init(|| {
            serde_json::from_str(include_str!("assets/cellular_vectors.json")).unwrap()
        });
        let f = f64::from(0.015_f32);
        let (x, y, z) = (x * f, y * f, z * f);
        let round = |n: f64| {
            if n >= 0. {
                (n + 0.5) as i32
            } else {
                (n - 0.5) as i32
            }
        };
        let (xr, yr, zr) = (round(x), round(y), round(z));
        let mut nearest = f64::MAX;
        let (mut noise, mut cy) = (0, 0.);
        for xi in xr - 1..=xr + 1 {
            for yi in yr - 1..=yr + 1 {
                for zi in zr - 1..=zr + 1 {
                    let hash = (self.seed
                        ^ xi.wrapping_mul(501_125_321)
                        ^ yi.wrapping_mul(1_136_930_381)
                        ^ zi.wrapping_mul(1_720_413_743))
                    .wrapping_mul(0x27d4eb2d);
                    let i = (hash & 1020) as usize;
                    let jitter = f64::from(0.39614353_f32);
                    let (px, py, pz) = (
                        xi as f64 + v[i] as f64 * jitter,
                        yi as f64 + v[i | 1] as f64 * jitter,
                        zi as f64 + v[i | 2] as f64 * jitter,
                    );
                    let d = (px - x).powi(2) + (py - y).powi(2) + (pz - z).powi(2);
                    if d < nearest {
                        nearest = d;
                        noise = hash;
                        cy = py;
                    }
                }
            }
        }
        (f64::from(noise as f32 * (1. / 2147483648_f32)), cy / f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_java_aquifer_cells() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/generator-accuracy/surface-noise-java.json"
        ))
        .unwrap();
        for row in f["samples"].as_array().unwrap() {
            let n = Cellular::new(row["seed"].as_i64().unwrap());
            let (noise, y) = n.cell(
                row["x"].as_f64().unwrap(),
                f64::from(row["y"].as_i64().unwrap() as f32 / 0.6_f32),
                row["z"].as_f64().unwrap(),
            );
            assert_eq!(noise, row["cell_noise"].as_f64().unwrap());
            assert!((y - row["cell_y"].as_f64().unwrap()).abs() < 1e-10);
        }
    }
}
