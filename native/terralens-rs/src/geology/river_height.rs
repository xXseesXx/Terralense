//! TFG river surface-height profiles. The declaration order is seed-significant.
use crate::{
    noise::{self, Noise2D, OpenSimplex2D},
    random::Xoroshiro,
    rivers::RiverInfo,
};
use serde::Deserialize;
use std::{collections::HashMap, rc::Rc, sync::OnceLock};

const RIVER_DATA: &str = include_str!("../../assets/data/tfg/biome-rivers.json");
const SEA: f64 = 63.0;
type Sample = Rc<dyn Fn(&RiverInfo, f64, f64, f64, f64, f64) -> f64>;

#[derive(Deserialize)]
struct RiverData {
    order: Vec<String>,
    biomes: HashMap<String, String>,
}
static DATA: OnceLock<RiverData> = OnceLock::new();
fn data() -> &'static RiverData {
    DATA.get_or_init(|| serde_json::from_str(RIVER_DATA).expect("bundled river biome types"))
}

pub struct RiverHeightSamplers {
    by_biome: HashMap<String, usize>,
    samplers: Vec<Option<Sample>>,
    cave: usize,
    none: usize,
}
impl RiverHeightSamplers {
    pub fn new(world_seed: i64) -> Self {
        let mut seed = Xoroshiro::new(world_seed);
        let mut samplers = Vec::new();
        for ty in &data().order {
            samplers.push(make_sampler(ty, &mut seed));
        }
        let index = |name: &str| data().order.iter().position(|x| x == name).unwrap_or(0);
        Self {
            by_biome: data()
                .biomes
                .iter()
                .map(|(b, t)| (b.clone(), index(t)))
                .collect(),
            samplers,
            cave: index("CAVE"),
            none: index("NONE"),
        }
    }
    pub fn type_for(&self, biome: &str) -> usize {
        *self.by_biome.get(biome).unwrap_or(&self.none)
    }
    pub fn count(&self) -> usize {
        self.samplers.len()
    }
    pub fn cave_index(&self) -> usize {
        self.cave
    }
    pub fn none_index(&self) -> usize {
        self.none
    }
    pub fn sample(
        &self,
        ty: usize,
        info: &RiverInfo,
        x: f64,
        z: f64,
        height: f64,
        cave_weight: f64,
        weight: f64,
    ) -> f64 {
        self.samplers
            .get(ty)
            .and_then(Option::as_ref)
            .map_or(height, |f| f(info, x, z, height, cave_weight, weight))
    }
}

fn base(seed: i64, octaves: Option<u32>, spread: f64) -> Noise2D {
    let mut n = OpenSimplex2D::new(seed);
    if let Some(o) = octaves {
        n.octaves(o);
    }
    n.spread(spread as f32 as f64);
    n.as_noise()
}
fn scaled(n: Noise2D, old_min: f64, old_max: f64, min: f64, max: f64) -> Noise2D {
    let k = (max - min) / (old_max - old_min);
    let b = min - old_min * k;
    Rc::new(move |x, z| n(x, z) * k + b)
}
fn scaled2(n: Noise2D, min: f64, max: f64) -> Noise2D {
    scaled(n, -1., 1., min, max)
}
fn abs(n: Noise2D) -> Noise2D {
    noise::abs(n)
}
fn clamp(v: f64, min: f64, max: f64) -> f64 {
    v.clamp(min, max)
}
fn clamped(n: Noise2D, min: f64, max: f64) -> Noise2D {
    noise::clamped(n, min, max)
}
fn clamped_map(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    c + ((v - a) / (b - a)).clamp(0., 1.) * (d - c)
}
fn map_range(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    c + ((v - a) / (b - a)) * (d - c)
}
fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + (b - a) * t
}
fn norm(info: &RiverInfo) -> f64 {
    info.dist_sq_blocks / info.width_sq
}
fn profile_noise(seed: &mut Xoroshiro, octaves: Option<u32>, spread: f64) -> Noise2D {
    base(seed.next_long(), octaves, spread)
}

fn make_sampler(kind: &str, seed: &mut Xoroshiro) -> Option<Sample> {
    let out: Sample = match kind {
        "NONE" => return None,
        "BANKED" => {
            let dist = scaled2(profile_noise(seed, Some(3), 0.05), -0.2, 0.2);
            let bank = scaled(
                abs(profile_noise(seed, Some(3), 0.025)),
                0.,
                1.,
                SEA - 4.,
                SEA + 30.,
            );
            Rc::new(move |info, x, z, height, _, _| {
                let d = norm(info) * 0.8f32 as f64 + dist(x, z);
                let river = 57. + if d < 1. { d * 6. } else { 6. };
                let hw = clamp(d - 1., 0., 2.);
                let rw = 2. - hw;
                ((height * hw + river * rw) / 2.).min(bank(x, z))
            })
        }
        "TALL_BANKED" => {
            let dist = scaled2(profile_noise(seed, Some(3), 0.05), -0.2, 0.2);
            let surf = scaled2(profile_noise(seed, Some(4), 0.07), -2., 2.);
            Rc::new(move |info, x, z, height, _, _| {
                let d = norm(info) * 0.8f32 as f64 + dist(x, z);
                let river = 57. + if d < 1. { d * 9. } else { 9. } + surf(x, z);
                let hw = clamp(d - 1., 0., 2.);
                let rw = 2. - hw;
                (height * hw + river * rw) / 2.
            })
        }
        "FLOODPLAIN" => {
            let dist = scaled2(profile_noise(seed, Some(4), 0.05), -0.2, 0.2);
            Rc::new(move |info, x, z, height, _, _| {
                let d = norm(info) * 0.8f32 as f64 + dist(x, z);
                let river = if d < 1. {
                    58.5 + d * 3.
                } else if d < 2. {
                    61.5
                } else {
                    let w = clamp(2. * d - 4., 0., 1.);
                    61.5 * (1. - w) + height * w
                };
                river.min(height)
            })
        }
        "WIDE" | "WIDE_DEEP" => {
            let floor = if kind == "WIDE" { 58. } else { 55. };
            let base = scaled2(profile_noise(seed, Some(4), 0.05), -2.5, 1.5);
            let dist = scaled2(profile_noise(seed, Some(4), 0.05), -0.15, 0.15);
            Rc::new(move |info, x, z, height, _, _| {
                (floor + norm(info) * 0.8f32 as f64 * 7. + dist(x, z) * 7. + base(x, z)).min(height)
            })
        }
        "CANYON" => {
            let base = scaled2(profile_noise(seed, Some(4), 0.05), -7., 3.);
            let dist = scaled2(profile_noise(seed, Some(4), 0.05), -0.3, 0.2);
            let low = clamped(profile_noise(seed, None, 0.0007), 0., 1.);
            Rc::new(move |info, x, z, height, _, _| {
                let d = norm(info) * 1.3 + dist(x, z);
                let adj = if d > 0.6 { d * 0.4 + 0.8 } else { d };
                (55. + lerp(low(x, z), d, adj) * 16. + base(x, z)).min(height)
            })
        }
        "TALL_CANYON" => {
            let base = scaled2(profile_noise(seed, Some(4), 0.05), -7., 3.);
            let dist = scaled2(profile_noise(seed, Some(4), 0.05), -0.3, 0.2);
            let _discarded_3d_seed = seed.next_long();
            Rc::new(move |info, x, z, height, _, _| {
                let d = norm(info) * 1.3 + dist(x, z);
                let adj = if d > 0.32 { d * 0.2 + 1.6 } else { d };
                (55. + adj * 16. + base(x, z)).min(height)
            })
        }
        "TALUS" => {
            let base = scaled2(profile_noise(seed, Some(4), 0.05), -2.5, 1.5);
            let cliff = scaled2(profile_noise(seed, Some(2), 0.1), 3., 8.);
            let dist = scaled2(profile_noise(seed, Some(4), 0.05), -0.15, 0.15);
            Rc::new(move |info, x, z, height, _, this_weight| {
                let d = norm(info);
                let df = d.sqrt() + dist(x, z);
                let talus = 55. + df * 12. + base(x, z) + if df > 1.5 { cliff(x, z) } else { 0. };
                let canyon = 55. + d * 1.3 * 16.;
                clamped_map(this_weight, 0.9, 1., canyon, talus).min(height)
            })
        }
        "TERRACES" => {
            let base = scaled2(profile_noise(seed, Some(4), 0.05), -2.5, 1.5);
            let cliff = scaled2(profile_noise(seed, Some(2), 0.1), 4., 8.);
            let cliff_base = scaled2(profile_noise(seed, Some(2), 0.06), SEA - 2., SEA + 4.);
            let dist = scaled2(profile_noise(seed, Some(4), 0.05), -0.15, 0.15);
            Rc::new(move |info, x, z, height, _, this_weight| {
                let d = norm(info);
                let sloped = 54. + (d.sqrt() * 0.85 + dist(x, z)) * 12. + base(x, z);
                let cb = cliff_base(x, z);
                let c = cliff(x, z);
                let lower = if sloped > cb {
                    clamped_map(sloped, cb, cb + 1.1, 0., c)
                } else {
                    0.
                };
                let upper = if sloped > cb + c {
                    clamped_map(sloped, cb + c, cb + c + 1.4, 0., c + 4.)
                } else {
                    0.
                };
                let terrace = sloped + lower + upper;
                let canyon = 55. + d * 1.3 * 16.;
                clamped_map(this_weight, 0.9, 1., canyon, terrace).min(height)
            })
        }
        "CAVE" => {
            let center = scaled2(profile_noise(seed, Some(2), 0.02), SEA - 3., SEA + 3.);
            let h = scaled2(profile_noise(seed, Some(4), 0.15), 8., 14.);
            Rc::new(move |info, x, z, height, cave_weight, _| {
                if cave_weight > 0.75 {
                    return height;
                }
                let d = norm(info);
                let canyon = (55. + d * 1.3 * 16.).min(height);
                if cave_weight <= 0.5 {
                    return canyon;
                }
                let max = (center(x, z) + h(x, z)).min(height);
                let interior = map_range(cave_weight, 0.5, 0.75, max, height);
                let exterior = map_range(cave_weight, 0.5, 0.75, canyon, height);
                lerp(clamp(d * 1.3 - 0.1, 0., 1.), interior, exterior)
            })
        }
        _ => return None,
    };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volcanic_river_profiles_match_pinned_core_biome_wrappers() {
        let samplers = RiverHeightSamplers::new(0);
        for (biome, expected) in [
            ("dormant_shield_volcano", "CAVE"),
            ("extinct_shield_volcano", "CAVE"),
            ("ancient_shield_volcano", "CAVE"),
            ("sunken_shield_volcano", "CAVE"),
            ("old_shield_volcano_shore", "TALL_CANYON"),
            ("ice_sheet_tuyas", "CAVE"),
            ("ice_sheet_tuyas_edge", "TALL_CANYON"),
            ("tuyas", "CANYON"),
        ] {
            assert_eq!(
                samplers.type_for(biome),
                data().order.iter().position(|ty| ty == expected).unwrap(),
                "TFGBiomes river wrapper for {biome}"
            );
        }
    }

    #[test]
    fn seed_zero_reported_bank_uses_the_floodplain_shelf_beside_a_lower_riverbed() {
        let region = crate::region::Generator::new(0, crate::model::Settings::default());
        let samplers = RiverHeightSamplers::new(0);
        let profile = samplers.type_for("plains");
        let sample = |x, z| {
            let edges = region.river_edges_at(x, z);
            let info = crate::rivers::sample_river_edge(&edges, x as f64, z as f64)
                .expect("observed bank has a nearby river");
            samplers.sample(profile, &info, x as f64, z as f64, 80., 0., 1.)
        };
        assert_eq!(
            sample(8833, 11107),
            61.5,
            "the river shapes its surrounding bank even outside the river biome"
        );
        assert!(
            sample(8827, 11107) < 61.,
            "the adjacent wet riverbed must remain lower than its dry shelf"
        );
    }
}

/// Sequential noise seeds consumed by the final CAVE profile, from the same
/// builder path as all preceding river profiles.
pub(crate) fn cave_noise_seeds(world_seed: i64) -> (i64, i64) {
    let mut r = Xoroshiro::new(world_seed);
    for kind in &data().order {
        if kind == "CAVE" {
            return (r.next_long(), r.next_long());
        }
        let _ = make_sampler(kind, &mut r);
    }
    unreachable!("installed profile includes CAVE")
}

pub(crate) fn tall_canyon_noise_seeds(world_seed: i64) -> (i64, i64) {
    let mut r = Xoroshiro::new(world_seed);
    for kind in &data().order {
        if kind == "TALL_CANYON" {
            r.next_long();
            return (r.next_long(), r.next_long());
        }
        let _ = make_sampler(kind, &mut r);
    }
    unreachable!("installed profile includes TALL_CANYON")
}
