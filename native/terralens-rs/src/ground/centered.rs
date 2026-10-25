//! Exposed basalt on TFG's centered cinder cones and tuyas.
use crate::{
    biome::ids::id,
    noise::{OpenSimplex2D, TfgCellular2D},
    world::World,
};
use lru::LruCache;
use std::{cell::RefCell, num::NonZeroUsize};

struct Samplers {
    cinder: TfgCellular2D,
    tuya: TfgCellular2D,
    tuff: TfgCellular2D,
    height: OpenSimplex2D,
}

impl Samplers {
    fn new(seed: i64) -> Self {
        let mut cinder = TfgCellular2D::new(seed);
        cinder.spread(f64::from(0.009_f32));
        let mut tuya = TfgCellular2D::configured(seed, 0.21, 1);
        tuya.spread(f64::from(0.0033_f32));
        let mut tuff = TfgCellular2D::configured(seed, 0.2, 1);
        tuff.spread(f64::from(0.003_f32));
        let mut random = crate::random::Xoroshiro::new(seed);
        let mut height = OpenSimplex2D::new(random.next_long());
        height.octaves(2).spread(f64::from(0.1_f32)).scaled(-4., 4.);
        Self {
            cinder,
            tuya,
            tuff,
            height,
        }
    }
}

thread_local! {
    static NOISE: RefCell<LruCache<i64, Samplers>> =
        RefCell::new(LruCache::new(NonZeroUsize::new(8).unwrap()));
}

/// Surface wrappers use unjittered, float-precision easing. Their height sampler
/// uses a jittered field, so a terrain-height threshold alone cannot identify rock.
pub(super) fn sample(
    world: &World,
    x: i32,
    y: i32,
    z: i32,
    rain: f64,
    temp: f64,
) -> Option<String> {
    NOISE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains(&world.seed) {
            cache.put(world.seed, Samplers::new(world.seed));
        }
        let noise = cache.get(&world.seed).unwrap();
        let c = noise.tuya.cell(f64::from(x), f64::from(z));
        let center = noise
            .tuya
            .cell(f64::from((x & !15) + 8), f64::from((z & !15) + 8));
        let biome = world
            .region
            .biome_at_quart((center.x as i32) >> 2, (center.y as i32) >> 2);
        let rarity = match biome {
            id::TUYAS => 2,
            id::ICE_SHEET_TUYAS | id::ICE_SHEET_TUYAS_EDGE => 3,
            _ => 0,
        };
        if 1. - easing(c, rarity) < 0.16_f32 {
            // The tuya wrapper returns without invoking its parent even below
            // its basalt height, leaving the local raw stone exposed there.
            return Some(
                if y > 57 + noise.height.sample(f64::from(x), f64::from(z)) as i32 {
                    "tfc:rock/raw/basalt".into()
                } else {
                    format!(
                        "tfc:rock/raw/{}",
                        world.rock_at(x, y, z).unwrap_or_else(|| "granite".into())
                    )
                },
            );
        }
        let c = noise.tuff.cell(f64::from(x), f64::from(z));
        let center = noise
            .tuff
            .cell(f64::from((x & !15) + 8), f64::from((z & !15) + 8));
        let biome = world
            .region
            .biome_at_quart((center.x as i32) >> 2, (center.y as i32) >> 2);
        let rarity = match biome {
            id::DORMANT_SHIELD_VOLCANO | id::EXTINCT_SHIELD_VOLCANO | id::SUNKEN_SHIELD_VOLCANO => {
                2
            }
            id::ANCIENT_SHIELD_VOLCANO | id::OLD_SHIELD_VOLCANO_SHORE => 3,
            _ => 0,
        };
        if easing(c, rarity) > 0.6_f32 {
            let dry = if super::position_hash(x, y, z, 728_275_914) & 127 > 48 {
                "tfg:tuff_gravel"
            } else {
                "tfc:sand/black"
            };
            return Some(if y < 62 {
                "tfg:tuff_gravel".into()
            } else if y + 1 < 66 {
                if rain > 420. {
                    "tfc:sand/green".into()
                } else {
                    "tfc:sand/black".into()
                }
            } else {
                super::builders::volcanic_top(x, y, z, rain, temp, Some(dry))
            });
        }
        let c = noise.cinder.cell(f64::from(x), f64::from(z));
        let center = noise
            .cinder
            .cell(f64::from((x & !15) + 8), f64::from((z & !15) + 8));
        let biome = world
            .region
            .biome_at_quart((center.x as i32) >> 2, (center.y as i32) >> 2);
        let config = match biome {
            id::CANYONS | id::DOLINE_CANYONS => Some((6, 91)),
            id::VOLCANIC_MOUNTAINS | id::VOLCANIC_MOUNTAIN_LAKE => Some((4, 103)),
            id::VOLCANIC_OCEANIC_MOUNTAINS | id::VOLCANIC_OCEANIC_MOUNTAIN_LAKE => Some((2, 83)),
            id::ACTIVE_SHIELD_VOLCANO => Some((4, 91)),
            _ => None,
        };
        if let Some((rarity, rock_y)) = config
            && easing(c, rarity) > 0.6_f32
            && f64::from(y + 1)
                > f64::from(rock_y) + noise.height.sample(f64::from(x), f64::from(z))
        {
            return Some("tfc:rock/raw/basalt".into());
        }
        None
    })
}

fn easing(cell: crate::noise::Cell, rarity: i32) -> f32 {
    if rarity == 0 || cell.noise.abs() > 1. / f64::from(rarity) {
        return 0.;
    }
    (1. - cell.f1 as f32 / 0.23_f32).clamp(0., 1.)
}
