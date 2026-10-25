//! Minecraft 1.20.1's fuzzy block-biome lookup (BiomeManager).
//!
//! The stored generator fields remain quart-resolution. This is the vanilla
//! position lookup that chooses one of the surrounding eight quart samples.

/// Pick the quart coordinate selected by vanilla's `FuzzyOffsetBiomeZoomer`.
/// `seed` must be `BiomeManager.obfuscateSeed(world_seed)`.
pub fn nearest_quart(seed: i64, x: i32, y: i32, z: i32) -> (i32, i32, i32) {
    // BiomeManager.getBiome(BlockPos) intentionally samples at pos - (2,2,2).
    let (x, y, z) = (x.wrapping_sub(2), y.wrapping_sub(2), z.wrapping_sub(2));
    let (qx, qy, qz) = (x >> 2, y >> 2, z >> 2);
    let (fx, fy, fz) = (
        (x & 3) as f64 / 4.0,
        (y & 3) as f64 / 4.0,
        (z & 3) as f64 / 4.0,
    );
    let mut best = f64::INFINITY;
    let mut selected = (qx, qy, qz);
    // Iteration order and strict comparison match the 1.20.1 bytecode.
    for i in 0..8 {
        let ox = i & 4 == 0;
        let oy = i & 2 == 0;
        let oz = i & 1 == 0;
        let cx = qx + i32::from(!ox);
        let cy = qy + i32::from(!oy);
        let cz = qz + i32::from(!oz);
        let d = fiddled_distance(
            seed,
            cx,
            cy,
            cz,
            fx - if ox { 0.0 } else { 1.0 },
            fy - if oy { 0.0 } else { 1.0 },
            fz - if oz { 0.0 } else { 1.0 },
        );
        if d < best {
            best = d;
            selected = (cx, cy, cz);
        }
    }
    selected
}

fn lcg_next(seed: i64, salt: i64) -> i64 {
    seed.wrapping_mul(
        seed.wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407),
    )
    .wrapping_add(salt)
}

fn fiddle(seed: i64) -> f64 {
    let n = ((seed >> 24).rem_euclid(1024)) as f64 / 1024.0;
    (n - 0.5) * 0.9
}

fn fiddled_distance(seed: i64, x: i32, y: i32, z: i32, dx: f64, dy: f64, dz: f64) -> f64 {
    let mut s = seed;
    for salt in [x, y, z, x, y, z] {
        s = lcg_next(s, i64::from(salt));
    }
    let jx = fiddle(s);
    s = lcg_next(s, seed);
    let jy = fiddle(s);
    s = lcg_next(s, seed);
    let jz = fiddle(s);
    let a = dz + jz;
    let b = dy + jy;
    let c = dx + jx;
    a * a + b * b + c * c
}

/// Vanilla's Guava `Hashing.sha256().hashLong(seed).asLong()` byte semantics.
pub fn obfuscate_seed(seed: i64) -> i64 {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(seed.to_le_bytes());
    i64::from_le_bytes(digest[..8].try_into().expect("SHA-256 digest prefix"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minecraft_obfuscated_seed_matches_sha256_hashlong() {
        assert_eq!(obfuscate_seed(0), 8_794_265_229_978_523_055);
        assert_eq!(obfuscate_seed(1), -6_467_378_160_175_308_932);
        assert_eq!(obfuscate_seed(-1), 6_759_447_113_877_070_610);
        assert_eq!(obfuscate_seed(123_456_789), -5_458_710_421_630_621_949);
    }

    #[test]
    fn fuzzy_quart_lookup_is_stable_across_signed_chunk_edges() {
        let seed = obfuscate_seed(-1_234_567_890);
        assert_eq!(seed, 1_459_567_974_964_968_691);
        assert_eq!(nearest_quart(seed, -17, 64, 31), (-4, 16, 7));
        assert_eq!(nearest_quart(seed, -16, 64, 31), (-4, 16, 7));
        assert_eq!(nearest_quart(seed, -1, 64, -1), (-1, 15, 0));
        assert_eq!(nearest_quart(seed, 0, 64, 0), (-1, 15, 0));
        assert_eq!(nearest_quart(seed, 15, 64, -17), (3, 16, -5));
        assert_eq!(nearest_quart(seed, 16, 64, -17), (3, 16, -5));
    }
}
