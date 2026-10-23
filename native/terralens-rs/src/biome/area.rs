//! TFC `Area`, zoom, smooth, and source layers. Each layer owns its own coordinate-seeded RNG.
use crate::random::Xoroshiro;
use std::{cell::RefCell, rc::Rc};

pub type Area = Rc<dyn Fn(i32, i32) -> i32>;
const CACHE_SIZE: usize = 8192;

pub fn cached_area<F: Fn(i32, i32) -> i32 + 'static>(source: F) -> Area {
    let cache = RefCell::new(vec![None::<(i32, i32, i32)>; CACHE_SIZE]);
    Rc::new(move |x, z| {
        let slot = (x.wrapping_mul(501125321) ^ z.wrapping_mul(1136930381)) as u32 as usize
            & (CACHE_SIZE - 1);
        if let Some((cx, cz, value)) = cache.borrow()[slot] {
            if cx == x && cz == z {
                return value;
            }
        }
        let value = source(x, z);
        cache.borrow_mut()[slot] = Some((x, z, value));
        value
    })
}

fn murmur_hash3(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^ (x >> 33)
}

/// TFC AreaContext: Java's `((long)x * 501125321 ^ (long)z * 1136930381 ^ seed) * 0x27d4eb2d`.
pub struct AreaContext {
    seed: u64,
    pending: RefCell<Option<(i32, i32)>>,
    rng: RefCell<Xoroshiro>,
}
impl AreaContext {
    pub fn new(seed: i64) -> Self {
        Self {
            seed: murmur_hash3(seed as u64),
            pending: RefCell::new(None),
            rng: RefCell::new(Xoroshiro::new(0)),
        }
    }
    pub fn set_seed(&self, x: i32, z: i32) {
        *self.pending.borrow_mut() = Some((x, z));
    }
    fn reseed(&self) {
        if let Some((x, z)) = self.pending.borrow_mut().take() {
            let h = ((x as i64).wrapping_mul(501125321)
                ^ (z as i64).wrapping_mul(1136930381)
                ^ self.seed as i64)
                .wrapping_mul(0x27d4eb2d);
            self.rng.borrow_mut().set_seed(h);
        }
    }
    pub fn int(&self) -> i32 {
        self.reseed();
        self.rng.borrow_mut().next_int()
    }
    pub fn bounded(&self, n: i32) -> i32 {
        self.reseed();
        self.rng.borrow_mut().next_int_bound(n)
    }
    pub fn choose(&self, a: i32, b: i32) -> i32 {
        if a == b {
            a
        } else if self.boolean() {
            a
        } else {
            b
        }
    }
    pub fn boolean(&self) -> bool {
        self.reseed();
        self.rng.borrow_mut().next_boolean()
    }
}

pub fn uniform_area(random: &mut Xoroshiro, zoom_levels: usize) -> Area {
    let ctx = Rc::new(AreaContext::new(random.next_long()));
    let c = ctx.clone();
    let mut area = cached_area(move |x, z| {
        c.set_seed(x, z);
        c.int()
    });
    for _ in 0..zoom_levels {
        let s = random.next_long();
        area = zoom_area(s, area);
        let s = random.next_long();
        area = smooth_area(s, area);
    }
    area
}

pub fn zoom_area(seed: i64, parent: Area) -> Area {
    let ctx = Rc::new(AreaContext::new(seed));
    cached_area(move |x, z| {
        let px = x >> 1;
        let pz = z >> 1;
        let a = parent(px, pz);
        ctx.set_seed(px, pz);
        if x & 1 == 0 && z & 1 == 0 {
            return a;
        }
        if x & 1 == 0 {
            return ctx.choose(a, parent(px, pz + 1));
        }
        if z & 1 == 0 {
            return ctx.choose(a, parent(px + 1, pz));
        }
        let b = parent(px, pz + 1);
        let c = parent(px + 1, pz);
        let d = parent(px + 1, pz + 1);
        if a == b {
            return if a == c || c != d {
                a
            } else {
                ctx.choose(a, c)
            };
        }
        if a == c {
            return if b != d { a } else { ctx.choose(a, b) };
        }
        if a == d {
            return if b != c { a } else { ctx.choose(a, b) };
        }
        if b == c || b == d {
            return b;
        }
        if c == d {
            return c;
        }
        match ctx.bounded(4) {
            0 => a,
            1 => b,
            2 => c,
            _ => d,
        }
    })
}

pub fn fuzzy_zoom_area(seed: i64, parent: Area) -> Area {
    let ctx = Rc::new(AreaContext::new(seed));
    cached_area(move |x, z| {
        let px = x >> 1;
        let pz = z >> 1;
        let a = parent(px, pz);
        ctx.set_seed(px, pz);
        if x & 1 == 0 && z & 1 == 0 {
            return a;
        }
        if x & 1 == 0 {
            return ctx.choose(a, parent(px, pz + 1));
        }
        if z & 1 == 0 {
            return ctx.choose(a, parent(px + 1, pz));
        }
        let b = parent(px, pz + 1);
        let c = parent(px + 1, pz);
        let d = parent(px + 1, pz + 1);
        match ctx.bounded(4) {
            0 => a,
            1 => b,
            2 => c,
            _ => d,
        }
    })
}

pub fn smooth_area(seed: i64, parent: Area) -> Area {
    let ctx = Rc::new(AreaContext::new(seed));
    cached_area(move |x, z| {
        ctx.set_seed(x, z);
        let n = parent(x, z - 1);
        let e = parent(x + 1, z);
        let s = parent(x, z + 1);
        let w = parent(x - 1, z);
        let c = parent(x, z);
        let eqx = e == w;
        let eqz = n == s;
        if eqx == eqz {
            if eqx { ctx.choose(w, n) } else { c }
        } else if eqx {
            w
        } else {
            n
        }
    })
}

pub fn source_area<F: Fn(i32, i32, &AreaContext) -> i32 + 'static>(seed: i64, f: F) -> Area {
    let ctx = Rc::new(AreaContext::new(seed));
    cached_area(move |x, z| {
        ctx.set_seed(x, z);
        f(x, z, &ctx)
    })
}

pub fn center_transform(
    seed: i64,
    parent: Area,
    f: impl Fn(i32, &AreaContext) -> i32 + 'static,
) -> Area {
    let ctx = Rc::new(AreaContext::new(seed));
    cached_area(move |x, z| {
        ctx.set_seed(x, z);
        f(parent(x, z), &ctx)
    })
}

pub fn adjacent_transform(
    parent: Area,
    f: impl Fn(i32, i32, i32, i32, i32) -> i32 + 'static,
) -> Area {
    cached_area(move |x, z| {
        f(
            parent(x, z - 1),
            parent(x + 1, z),
            parent(x, z + 1),
            parent(x - 1, z),
            parent(x, z),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_preserves_uniform_even_points() {
        let a: Area = Rc::new(|x, z| x.wrapping_mul(31) ^ z);
        let b = zoom_area(12, a.clone());
        for z in -8..8 {
            for x in -8..8 {
                assert_eq!(b(x * 2, z * 2), a(x, z));
            }
        }
    }
    #[test]
    fn cache_is_coordinate_exact() {
        let a = cached_area(|x, z| x.wrapping_add(z));
        assert_eq!(a(-3, 8), 5);
        assert_eq!(a(4, -7), -3);
        assert_eq!(a(-3, 8), 5);
    }

    #[test]
    fn uniform_area_matches_typescript_capture() {
        let mut rng = Xoroshiro::new(9_001_001_001);
        let area = uniform_area(&mut rng, 2);
        let points = [(-9, -8), (-1, 0), (0, 0), (3, 7), (22, -18), (100, 63)];
        assert_eq!(
            points.map(|(x, z)| area(x, z)),
            [
                1_627_604_745,
                -1_845_946_208,
                -541_493_984,
                -1_712_262_595,
                -759_151_553,
                97_729_379
            ]
        );
    }
}
