//! Java-compatible deterministic random streams used by Minecraft and TFC.
//!
//! Arithmetic deliberately uses wrapping integers at the same 32/48/64-bit boundaries as Java.

const MASK48: u64 = (1u64 << 48) - 1;
const JAVA_MULTIPLIER: u64 = 0x5deece66d;
const JAVA_ADDEND: u64 = 0xb;
const GOLDEN: u64 = 0x9e3779b97f4a7c15;
const SILVER: u64 = 0x6a09e667f3bcc909;

#[derive(Clone, Debug)]
pub struct JavaRandom {
    seed: u64,
    gaussian: Option<f64>,
}

impl JavaRandom {
    pub fn new(seed: i64) -> Self {
        let mut r = Self {
            seed: 0,
            gaussian: None,
        };
        r.set_seed(seed);
        r
    }
    pub fn set_seed(&mut self, seed: i64) {
        self.seed = ((seed as u64) ^ JAVA_MULTIPLIER) & MASK48;
        self.gaussian = None;
    }
    pub fn next(&mut self, bits: u32) -> i32 {
        assert!((1..=32).contains(&bits));
        self.seed = self
            .seed
            .wrapping_mul(JAVA_MULTIPLIER)
            .wrapping_add(JAVA_ADDEND)
            & MASK48;
        (self.seed >> (48 - bits)) as i32
    }
    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }
    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if bound & -bound == bound {
            return (((bound as i64) * (self.next(31) as i64)) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }
    pub fn next_int_bounded(&mut self, bound: i32) -> i32 {
        self.next_int_bound(bound)
    }
    pub fn next_long(&mut self) -> i64 {
        ((self.next(32) as i64) << 32).wrapping_add(self.next(32) as i64)
    }
    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }
    pub fn next_float(&mut self) -> f32 {
        (self.next(24) as f32) / (1u32 << 24) as f32
    }
    pub fn next_double(&mut self) -> f64 {
        let hi = self.next(26) as u64;
        let lo = self.next(27) as u64;
        ((hi << 27) + lo) as f64 * (1.0 / (1u64 << 53) as f64)
    }
    pub fn next_gaussian(&mut self) -> f64 {
        if let Some(g) = self.gaussian.take() {
            return g;
        }
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s >= 1.0 || s == 0.0 {
                continue;
            }
            let m = (-2.0 * s.ln() / s).sqrt();
            self.gaussian = Some(v2 * m);
            return v1 * m;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Xoroshiro {
    lo: u64,
    hi: u64,
    gaussian: Option<f64>,
}

pub fn mix_stafford13(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
pub fn upgrade_seed(seed: i64) -> (u64, u64) {
    let lo = (seed as u64) ^ SILVER;
    (mix_stafford13(lo), mix_stafford13(lo.wrapping_add(GOLDEN)))
}

impl Xoroshiro {
    pub fn new(seed: i64) -> Self {
        let (lo, hi) = upgrade_seed(seed);
        Self::from_state(lo, hi)
    }
    pub fn from_seed(seed: i64) -> Self {
        Self::new(seed)
    }
    pub fn from_state(mut lo: u64, mut hi: u64) -> Self {
        if lo | hi == 0 {
            lo = GOLDEN;
            hi = SILVER;
        }
        Self {
            lo,
            hi,
            gaussian: None,
        }
    }
    pub fn set_seed(&mut self, seed: i64) {
        let (lo, hi) = upgrade_seed(seed);
        self.lo = lo;
        self.hi = hi;
        self.gaussian = None;
    }
    pub fn next_long(&mut self) -> i64 {
        let s0 = self.lo;
        let s1 = self.hi;
        let result = s0.wrapping_add(s1).rotate_left(17).wrapping_add(s0);
        let x = s1 ^ s0;
        self.lo = s0.rotate_left(49) ^ x ^ (x << 21);
        self.hi = x.rotate_left(28);
        result as i64
    }
    pub fn next_int(&mut self) -> i32 {
        self.next_long() as i32
    }
    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        self.bounded(bound)
    }
    pub fn next_int_bounded(&mut self, bound: i32) -> i32 {
        self.bounded(bound)
    }
    pub fn next(&mut self, bits: u32) -> i32 {
        assert!((1..=32).contains(&bits));
        ((self.next_long() as u64) >> (64 - bits)) as i32
    }
    pub fn bounded(&mut self, bound: i32) -> i32 {
        assert!(bound > 0);
        let b = bound as u32;
        let threshold = b.wrapping_neg() % b;
        loop {
            let p = (self.next_int() as u32 as u64) * (b as u64);
            if (p as u32) >= threshold {
                return (p >> 32) as i32;
            }
        }
    }
    pub fn next_boolean(&mut self) -> bool {
        self.next_long() & 1 != 0
    }
    pub fn next_float(&mut self) -> f32 {
        (self.next(24) as f32) / (1u32 << 24) as f32
    }
    pub fn next_double(&mut self) -> f64 {
        ((self.next_long() as u64) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
    pub fn next_gaussian(&mut self) -> f64 {
        if let Some(g) = self.gaussian.take() {
            return g;
        }
        loop {
            let x = 2. * self.next_double() - 1.;
            let y = 2. * self.next_double() - 1.;
            let s = x * x + y * y;
            if s >= 1. || s == 0. {
                continue;
            }
            let m = (-2. * s.ln() / s).sqrt();
            self.gaussian = Some(y * m);
            return x * m;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_reference_stream() {
        let mut r = JavaRandom::new(0);
        assert_eq!(r.next_int(), -1155484576);
        assert_eq!(r.next_int(), -723955400);
    }
    #[test]
    fn xoroshiro_seed_stream_is_repeatable() {
        let mut a = Xoroshiro::new(-1234567);
        let mut b = Xoroshiro::new(-1234567);
        for _ in 0..100 {
            assert_eq!(a.next_long(), b.next_long());
        }
    }

    #[test]
    fn xoroshiro_matches_minecraft_1_20_fixture() {
        // Captured from the official Minecraft 1.20.1 XoroshiroRandomSource fixture.
        let mut r = Xoroshiro::new(0);
        assert_eq!(r.next_long(), 3_038_984_756_725_240_190);
        assert_eq!(r.next_long(), -3_694_039_286_755_638_414);
    }
    #[test]
    fn bounded_values_are_valid() {
        let mut r = Xoroshiro::new(0);
        for _ in 0..1000 {
            let n = r.next_int_bound(17);
            assert!((0..17).contains(&n));
        }
    }
}
