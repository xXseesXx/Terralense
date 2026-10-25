//! Minecraft 1.20.1 Xoroshiro-seeded Improved/NormalNoise used by planetary climate routers.
use crate::random::Xoroshiro;

const INPUT_FACTOR: f64 = 1.0181268882175227;

#[derive(Clone)]
pub(super) struct ImprovedNoise {
    xo: f64,
    yo: f64,
    zo: f64,
    perm: [u8; 256],
}
impl ImprovedNoise {
    pub(super) fn new(r: &mut Xoroshiro) -> Self {
        let (xo, yo, zo) = (
            r.next_double() * 256.,
            r.next_double() * 256.,
            r.next_double() * 256.,
        );
        let mut perm = [0; 256];
        for (i, v) in perm.iter_mut().enumerate() {
            *v = i as u8;
        }
        for i in 0..256 {
            let j = i + r.next_int_bound((256 - i) as i32) as usize;
            perm.swap(i, j);
        }
        Self { xo, yo, zo, perm }
    }
    #[inline]
    fn p(&self, i: i32) -> usize {
        self.perm[(i & 255) as usize] as usize
    }
    #[inline]
    fn fade(t: f64) -> f64 {
        t * t * t * (t * (t * 6. - 15.) + 10.)
    }
    #[inline]
    fn lerp(t: f64, a: f64, b: f64) -> f64 {
        a + t * (b - a)
    }
    #[inline]
    fn grad(hash: usize, x: f64, y: f64, z: f64) -> f64 {
        let h = hash & 15;
        let u = if h < 8 { x } else { y };
        let v = if h < 4 {
            y
        } else if h == 12 || h == 14 {
            x
        } else {
            z
        };
        (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
    }
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.sample_with_y_offset(x, y, z, 0., 0.)
    }
    /// Vanilla `ImprovedNoise.noise(x, y, z, yOffset, ySmear)` used by blended terrain.
    pub(super) fn sample_with_y_offset(
        &self,
        x: f64,
        y: f64,
        z: f64,
        y_offset: f64,
        y_smear: f64,
    ) -> f64 {
        let xp = x + self.xo;
        let yp = y + self.yo;
        let zp = z + self.zo;
        let (xi, yi, zi) = (xp.floor() as i32, yp.floor() as i32, zp.floor() as i32);
        let (xd, raw_yd, zd) = (xp - xi as f64, yp - yi as f64, zp - zi as f64);
        let mut gradient_y = raw_yd;
        if y_offset != 0. {
            let y_for_smear = if y_smear >= 0. && y_smear < raw_yd {
                y_smear
            } else {
                raw_yd
            };
            let smear_offset = (y_for_smear / y_offset + 1.0e-7).floor() * y_offset;
            gradient_y -= smear_offset;
        }
        let (a, b) = (self.p(xi) as i32, self.p(xi.wrapping_add(1)) as i32);
        let aa = self.p(a.wrapping_add(yi)) as i32;
        let ab = self.p(a.wrapping_add(yi).wrapping_add(1)) as i32;
        let ba = self.p(b.wrapping_add(yi)) as i32;
        let bb = self.p(b.wrapping_add(yi).wrapping_add(1)) as i32;
        let n000 = Self::grad(self.p(aa.wrapping_add(zi)), xd, gradient_y, zd);
        let n100 = Self::grad(self.p(ba.wrapping_add(zi)), xd - 1., gradient_y, zd);
        let n010 = Self::grad(self.p(ab.wrapping_add(zi)), xd, gradient_y - 1., zd);
        let n110 = Self::grad(self.p(bb.wrapping_add(zi)), xd - 1., gradient_y - 1., zd);
        let n001 = Self::grad(
            self.p(aa.wrapping_add(zi).wrapping_add(1)),
            xd,
            gradient_y,
            zd - 1.,
        );
        let n101 = Self::grad(
            self.p(ba.wrapping_add(zi).wrapping_add(1)),
            xd - 1.,
            gradient_y,
            zd - 1.,
        );
        let n011 = Self::grad(
            self.p(ab.wrapping_add(zi).wrapping_add(1)),
            xd,
            gradient_y - 1.,
            zd - 1.,
        );
        let n111 = Self::grad(
            self.p(bb.wrapping_add(zi).wrapping_add(1)),
            xd - 1.,
            gradient_y - 1.,
            zd - 1.,
        );
        // Minecraft uses the unsmeared cell-local Y for interpolation, and only smears the Y
        // offset passed to the gradient function.
        let (u, v, w) = (Self::fade(xd), Self::fade(raw_yd), Self::fade(zd));
        Self::lerp(
            w,
            Self::lerp(v, Self::lerp(u, n000, n100), Self::lerp(u, n010, n110)),
            Self::lerp(v, Self::lerp(u, n001, n101), Self::lerp(u, n011, n111)),
        )
    }
}

#[derive(Clone)]
struct Perlin {
    levels: Vec<Option<ImprovedNoise>>,
    amps: Vec<f64>,
    input: f64,
    value: f64,
}
impl Perlin {
    fn new(random: &mut Xoroshiro, first: i32, amps: &[f64]) -> Self {
        let (lo, hi) = (random.next_long() as u64, random.next_long() as u64);
        let mut levels = Vec::with_capacity(amps.len());
        for (i, a) in amps.iter().enumerate() {
            if *a == 0. {
                levels.push(None);
                continue;
            }
            let name = format!("octave_{}", first + i as i32);
            levels.push(Some(ImprovedNoise::new(&mut named_source(lo, hi, &name))));
        }
        Self {
            levels,
            amps: amps.to_vec(),
            input: 2f64.powi(first),
            value: 2f64.powi(amps.len() as i32 - 1) / (2f64.powi(amps.len() as i32) - 1.),
        }
    }
    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let (mut total, mut f, mut a) = (0., self.input, self.value);
        for (n, amp) in self.levels.iter().zip(&self.amps) {
            if let Some(n) = n {
                total += amp * n.sample(wrap(x * f), wrap(y * f), wrap(z * f)) * a;
            }
            f *= 2.;
            a *= 0.5;
        }
        total
    }
}

#[derive(Clone)]
pub(crate) struct NormalNoise {
    first: Perlin,
    second: Perlin,
    factor: f64,
}
impl NormalNoise {
    pub fn new(seed: i64, name: &str, first: i32, amps: &[f64]) -> Self {
        let (lo, hi) = root_positional(seed);
        let mut random = named_source(lo, hi, name);
        let first_noise = Perlin::new(&mut random, first, amps);
        let second = Perlin::new(&mut random, first, amps);
        let active: Vec<_> = amps
            .iter()
            .enumerate()
            .filter_map(|(i, a)| (*a != 0.).then_some(i))
            .collect();
        let span = active
            .last()
            .zip(active.first())
            .map(|(h, l)| h - l + 1)
            .unwrap_or(1);
        Self {
            first: first_noise,
            second,
            factor: (1. / 6.) / (0.1 * (1. + 1. / span as f64)),
        }
    }
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        (self.first.sample(x, y, z)
            + self
                .second
                .sample(x * INPUT_FACTOR, y * INPUT_FACTOR, z * INPUT_FACTOR))
            * self.factor
    }
}

pub(crate) fn root_positional(seed: i64) -> (u64, u64) {
    let mut random = Xoroshiro::new(seed);
    (random.next_long() as u64, random.next_long() as u64)
}
pub(crate) fn named_source(lo: u64, hi: u64, name: &str) -> Xoroshiro {
    let digest = md5::compute(name.as_bytes());
    let read = |offset: usize| u64::from_be_bytes(digest.0[offset..offset + 8].try_into().unwrap());
    Xoroshiro::from_state(read(0) ^ lo, read(8) ^ hi)
}
pub(super) fn positional_named_source(seed: i64, name: &str) -> Xoroshiro {
    let (lo, hi) = root_positional(seed);
    named_source(lo, hi, name)
}
#[inline]
fn wrap(v: f64) -> f64 {
    v - (v / 33_554_432. + 0.5).floor() * 33_554_432.
}

#[cfg(test)]
mod tests {
    use super::ImprovedNoise;
    use crate::random::Xoroshiro;

    #[test]
    fn y_smear_matches_minecraft_1_20_1_oracle_including_negative_coordinates() {
        // Captured by calling ImprovedNoise.noise(x, y, z, yOffset, ySmear) from the local
        // 1.20.1 joined/SRG jar with XoroshiroRandomSource(123456789L).
        let cases = [
            ([-12.3, -45.67, 89.1, 12.5, 26.78], 0.050629743421553620),
            ([123.456, -7.125, -99.3, 32.0, -3.4], -0.010489494090216489),
            ([0.25, 14.75, -5.5, 3.0, 0.1], -0.30857438133947010),
            ([128.0, 33.0, -256.0, 0.5, 0.7], -0.39232735860885370),
            ([-0.75, -0.25, -0.5, 0.0, 0.0], 0.070929077783484120),
        ];
        let noise = ImprovedNoise::new(&mut Xoroshiro::new(123_456_789));
        for (args, expected) in cases {
            let actual = noise.sample_with_y_offset(args[0], args[1], args[2], args[3], args[4]);
            assert!(
                (actual - expected).abs() < 1e-14,
                "({args:?}) expected {expected:.17} but got {actual:.17}"
            );
        }
    }
}
