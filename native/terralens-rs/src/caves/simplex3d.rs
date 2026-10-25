// Derived from FastNoiseLite 1.0.1, Copyright (c) 2020 Jordan Peck and contributors.
// MIT license: licenses/FASTNOISELITE-MIT.txt in the repository root.
//! TFC OpenSimplex2S with ImproveXZPlanes, used for undercut river walls.
use std::sync::OnceLock;
pub(super) struct Simplex3 {
    seed: i32,
}
impl Simplex3 {
    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ (seed >> 32)) as i32,
        }
    }
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        // OpenSimplex3D.octaves(2).spread(.1f).scaled(0,3).
        let frequency = 0.5 * f64::from(0.1_f32);
        let (mut x, mut y, mut z) = (x * frequency, y * frequency, z * frequency);
        let xz = x + z;
        let s = xz * -0.211324865405187;
        y *= 0.577350269189626;
        x += s - y;
        z += s - y;
        y += xz * 0.577350269189626;
        let mut sum = 0_f32;
        let mut amp = 1_f32 / (1. + 0.5);
        for octave in 0..2 {
            sum += single(self.seed.wrapping_add(octave), x, y, z) * amp;
            x *= 2.;
            y *= 2.;
            z *= 2.;
            amp *= 0.5;
        }
        1.5 + f64::from(sum) * 1.5
    }
}
// Operation order follows the pinned FastNoiseLite OpenSimplex2S kernel.
// Lattice coordinates stay unprimed until grad(), which uses wrapping products.
#[allow(unused_parens)]
fn single(seed: i32, x: f64, y: f64, z: f64) -> f32 {
    let i: i32 = x.floor() as i32;
    let j: i32 = y.floor() as i32;
    let k: i32 = z.floor() as i32;
    let xi: f32 = (x - i as f64) as f32;
    let yi: f32 = (y - j as f64) as f32;
    let zi: f32 = (z - k as f64) as f32;
    let seed2: i32 = seed.wrapping_add(1293373);
    let x_nmask: i32 = (-0.5_f32 - xi) as i32;
    let y_nmask: i32 = (-0.5_f32 - yi) as i32;
    let z_nmask: i32 = (-0.5_f32 - zi) as i32;
    let x0: f32 = xi + x_nmask as f32;
    let y0: f32 = yi + y_nmask as f32;
    let z0: f32 = zi + z_nmask as f32;
    let a0: f32 = 0.75_f32 - x0 * x0 - y0 * y0 - z0 * z0;
    let mut value: f32 = (a0 * a0)
        * (a0 * a0)
        * grad(
            seed,
            i + (x_nmask & 1),
            j + (y_nmask & 1),
            k + (z_nmask & 1),
            x0,
            y0,
            z0,
        );
    let x1: f32 = xi - 0.5_f32;
    let y1: f32 = yi - 0.5_f32;
    let z1: f32 = zi - 0.5_f32;
    let a1: f32 = 0.75_f32 - x1 * x1 - y1 * y1 - z1 * z1;
    value += (a1 * a1) * (a1 * a1) * grad(seed2, i + 1, j + 1, k + 1, x1, y1, z1);
    let x_aflip_mask0: f32 = (((x_nmask | 1) << 1) as f32) * x1;
    let y_aflip_mask0: f32 = (((y_nmask | 1) << 1) as f32) * y1;
    let z_aflip_mask0: f32 = (((z_nmask | 1) << 1) as f32) * z1;
    let x_aflip_mask1: f32 = ((-2 - (x_nmask << 2)) as f32) * x1 - 1.0_f32;
    let y_aflip_mask1: f32 = ((-2 - (y_nmask << 2)) as f32) * y1 - 1.0_f32;
    let z_aflip_mask1: f32 = ((-2 - (z_nmask << 2)) as f32) * z1 - 1.0_f32;
    let mut skip5 = false;
    let a2: f32 = x_aflip_mask0 + a0;
    if a2 > 0. {
        let x2: f32 = x0 - ((x_nmask | 1) as f32);
        let y2: f32 = y0;
        let z2: f32 = z0;
        value += (a2 * a2)
            * (a2 * a2)
            * grad(
                seed,
                i + (!x_nmask & 1),
                j + (y_nmask & 1),
                k + (z_nmask & 1),
                x2,
                y2,
                z2,
            );
    } else {
        let a3: f32 = y_aflip_mask0 + z_aflip_mask0 + a0;
        if a3 > 0. {
            let x3: f32 = x0;
            let y3: f32 = y0 - ((y_nmask | 1) as f32);
            let z3: f32 = z0 - ((z_nmask | 1) as f32);
            value += (a3 * a3)
                * (a3 * a3)
                * grad(
                    seed,
                    i + (x_nmask & 1),
                    j + (!y_nmask & 1),
                    k + (!z_nmask & 1),
                    x3,
                    y3,
                    z3,
                );
        }
        let a4: f32 = x_aflip_mask1 + a1;
        if a4 > 0. {
            let x4: f32 = ((x_nmask | 1) as f32) + x1;
            let y4: f32 = y1;
            let z4: f32 = z1;
            value +=
                (a4 * a4) * (a4 * a4) * grad(seed2, i + (x_nmask & 2), j + 1, k + 1, x4, y4, z4);
            skip5 = true;
        }
    }
    let mut skip9 = false;
    let a6: f32 = y_aflip_mask0 + a0;
    if a6 > 0. {
        let x6: f32 = x0;
        let y6: f32 = y0 - ((y_nmask | 1) as f32);
        let z6: f32 = z0;
        value += (a6 * a6)
            * (a6 * a6)
            * grad(
                seed,
                i + (x_nmask & 1),
                j + (!y_nmask & 1),
                k + (z_nmask & 1),
                x6,
                y6,
                z6,
            );
    } else {
        let a7: f32 = x_aflip_mask0 + z_aflip_mask0 + a0;
        if a7 > 0. {
            let x7: f32 = x0 - ((x_nmask | 1) as f32);
            let y7: f32 = y0;
            let z7: f32 = z0 - ((z_nmask | 1) as f32);
            value += (a7 * a7)
                * (a7 * a7)
                * grad(
                    seed,
                    i + (!x_nmask & 1),
                    j + (y_nmask & 1),
                    k + (!z_nmask & 1),
                    x7,
                    y7,
                    z7,
                );
        }
        let a8: f32 = y_aflip_mask1 + a1;
        if a8 > 0. {
            let x8: f32 = x1;
            let y8: f32 = ((y_nmask | 1) as f32) + y1;
            let z8: f32 = z1;
            value +=
                (a8 * a8) * (a8 * a8) * grad(seed2, i + 1, j + (y_nmask & 2), k + 1, x8, y8, z8);
            skip9 = true;
        }
    }
    let mut skip_d = false;
    let a_a: f32 = z_aflip_mask0 + a0;
    if a_a > 0. {
        let x_a: f32 = x0;
        let y_a: f32 = y0;
        let z_a: f32 = z0 - ((z_nmask | 1) as f32);
        value += (a_a * a_a)
            * (a_a * a_a)
            * grad(
                seed,
                i + (x_nmask & 1),
                j + (y_nmask & 1),
                k + (!z_nmask & 1),
                x_a,
                y_a,
                z_a,
            );
    } else {
        let a_b: f32 = x_aflip_mask0 + y_aflip_mask0 + a0;
        if a_b > 0. {
            let x_b: f32 = x0 - ((x_nmask | 1) as f32);
            let y_b: f32 = y0 - ((y_nmask | 1) as f32);
            let z_b: f32 = z0;
            value += (a_b * a_b)
                * (a_b * a_b)
                * grad(
                    seed,
                    i + (!x_nmask & 1),
                    j + (!y_nmask & 1),
                    k + (z_nmask & 1),
                    x_b,
                    y_b,
                    z_b,
                );
        }
        let a_c: f32 = z_aflip_mask1 + a1;
        if a_c > 0. {
            let x_c: f32 = x1;
            let y_c: f32 = y1;
            let z_c: f32 = ((z_nmask | 1) as f32) + z1;
            value += (a_c * a_c)
                * (a_c * a_c)
                * grad(seed2, i + 1, j + 1, k + (z_nmask & 2), x_c, y_c, z_c);
            skip_d = true;
        }
    }
    if !skip5 {
        let a5: f32 = y_aflip_mask1 + z_aflip_mask1 + a1;
        if a5 > 0. {
            let x5: f32 = x1;
            let y5: f32 = ((y_nmask | 1) as f32) + y1;
            let z5: f32 = ((z_nmask | 1) as f32) + z1;
            value += (a5 * a5)
                * (a5 * a5)
                * grad(
                    seed2,
                    i + 1,
                    j + (y_nmask & 2),
                    k + (z_nmask & 2),
                    x5,
                    y5,
                    z5,
                );
        }
    }
    if !skip9 {
        let a9: f32 = x_aflip_mask1 + z_aflip_mask1 + a1;
        if a9 > 0. {
            let x9: f32 = ((x_nmask | 1) as f32) + x1;
            let y9: f32 = y1;
            let z9: f32 = ((z_nmask | 1) as f32) + z1;
            value += (a9 * a9)
                * (a9 * a9)
                * grad(
                    seed2,
                    i + (x_nmask & 2),
                    j + 1,
                    k + (z_nmask & 2),
                    x9,
                    y9,
                    z9,
                );
        }
    }
    if !skip_d {
        let a_d: f32 = x_aflip_mask1 + y_aflip_mask1 + a1;
        if a_d > 0. {
            let x_d: f32 = ((x_nmask | 1) as f32) + x1;
            let y_d: f32 = ((y_nmask | 1) as f32) + y1;
            let z_d: f32 = z1;
            value += (a_d * a_d)
                * (a_d * a_d)
                * grad(
                    seed2,
                    i + (x_nmask & 2),
                    j + (y_nmask & 2),
                    k + 1,
                    x_d,
                    y_d,
                    z_d,
                );
        }
    }
    value * 9.046026385208288_f32
}

fn grad(seed: i32, x: i32, y: i32, z: i32, dx: f32, dy: f32, dz: f32) -> f32 {
    static G: OnceLock<Vec<f32>> = OnceLock::new();
    let g =
        G.get_or_init(|| serde_json::from_str(include_str!("assets/gradients3d.json")).unwrap());
    let mut h = (seed
        ^ x.wrapping_mul(501_125_321)
        ^ y.wrapping_mul(1_136_930_381)
        ^ z.wrapping_mul(1_720_413_743))
    .wrapping_mul(0x27d4eb2d);
    h ^= h >> 15;
    let i = (h & 252) as usize;
    dx * g[i] + dy * g[i | 1] + dz * g[i | 2]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_java_undercut_noise() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/generator-accuracy/surface-noise-java.json"
        ))
        .unwrap();
        let mut max_error = 0_f64;
        for row in f["samples"].as_array().unwrap() {
            let n = Simplex3::new(row["seed"].as_i64().unwrap());
            let got = n.sample(
                row["x"].as_f64().unwrap(),
                row["y"].as_f64().unwrap(),
                row["z"].as_f64().unwrap(),
            );
            max_error = max_error.max((got - row["simplex"].as_f64().unwrap()).abs());
        }
        assert!(max_error < 0.000002, "max Java noise error {max_error}");
    }
}
