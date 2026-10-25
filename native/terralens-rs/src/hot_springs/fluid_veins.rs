//! TFG's `tfg:spring_water` bedrock-fluid vein gate.
//!
//! Sources: installed TFG Core-Modern 0.9.25 `TFGBedrockFluids`,
//! `TFGBedrockFluidClimates`,
//! `TFGBedrockFluidSavedDataMixin`; GTCEu 7.5.3 `GTBedrockFluids` and
//! `BedrockFluidVeinSavedData`; installed TFG 0.13.8 feature data. This is
//! only the fluid-vein placement predicate. The placed-feature rarity and
//! in-square draws are performed by the caller.

use crate::{biome::ids::id, dimension::Dimension, random::Xoroshiro, world::World};

// GT's `GTRegistry` iterates its HashMap-backed keyToValue map. The order below
// is the resulting 1.20.1 ResourceLocation hash-bucket order after TFG removes
// GT's eight built-in deposits, registers its 41 declarations, then KubeJS adds
// nine dimension-filtered planetary veins. The final registry's HashMap has a
// 128-slot table. This order was independently reproduced with Java
// HashMap/ResourceLocation.hashCode. Entries with zero weight for a volcanic
// biome are omitted by weights_for, which preserves cumulative selection.
const EARTH_REGISTRY_WEIGHT_ORDER: &[Vein] = &[
    Vein::GasSurface,
    Vein::LightOilCommon,
    Vein::LightOilShore,
    Vein::RawOilSandstone,
    Vein::MuddyWater,
    Vein::LavaVolcanic,
    Vein::GasPermafrost,
    Vein::RawOilOcean,
    Vein::OilCommon,
    Vein::GasOcean,
    Vein::GasVolcanic,
    Vein::HeavyOilCommon,
    Vein::SpringWater,
    Vein::LightOilHot,
    Vein::OilNormal,
    Vein::SeaWater,
    Vein::LightOilOcean,
    Vein::HeavyOilDry,
    Vein::WaterCommon,
    Vein::OilOcean,
    Vein::GasSwamp,
    Vein::RawOilHot,
    Vein::HeavyOilHot,
    Vein::RawOilCommon,
    Vein::OilHot,
    Vein::LavaMountain,
    Vein::GasCommon,
];

#[derive(Clone, Copy)]
enum Vein {
    GasSurface,
    SpringWater,
    LightOilHot,
    LightOilCommon,
    OilNormal,
    LightOilShore,
    SeaWater,
    WaterCommon,
    MuddyWater,
    GasSwamp,
    LavaVolcanic,
    GasPermafrost,
    OilCommon,
    RawOilCommon,
    GasOcean,
    LavaMountain,
    GasVolcanic,
    GasCommon,
    HeavyOilCommon,
    LightOilOcean,
    OilHot,
    OilOcean,
    RawOilHot,
    RawOilOcean,
    RawOilSandstone,
    HeavyOilHot,
    HeavyOilDry,
}

pub(super) fn is_spring_water(world: &World, chunk_x: i32, chunk_z: i32) -> bool {
    if world.dimension != Dimension::Overworld {
        return false;
    }

    // TFG's `EarthIsVolcanic` biome tag. All other Overworld biomes have zero
    // spring-water weight, so avoid climate sampling and RNG work for them.
    let block_x = chunk_x.wrapping_mul(16);
    let block_z = chunk_z.wrapping_mul(16);
    let biome = world.biome_id(block_x, block_z);
    if !is_volcanic_biome(biome) {
        return false;
    }
    // TFG's ClimateWeightModifier queries ChunkData at the chunk origin.
    let climate = world.region.climate(block_x, block_z);
    // TFC ChunkData exposes both climate axes as Java `float` values.
    selects_spring_water(
        world.seed,
        chunk_x,
        chunk_z,
        biome,
        climate.temperature as f32,
        climate.rainfall as f32,
    )
}

fn selects_spring_water(
    world_seed: i64,
    chunk_x: i32,
    chunk_z: i32,
    biome: u16,
    temperature: f32,
    rainfall: f32,
) -> bool {
    if !is_volcanic_biome(biome) {
        return false;
    }
    let weights: [i32; 27] = std::array::from_fn(|index| {
        weight_for(
            EARTH_REGISTRY_WEIGHT_ORDER[index],
            biome,
            temperature,
            rainfall,
        )
    });
    let total: i32 = weights.iter().sum();
    if total <= 0 {
        return false;
    }

    // GT: getVeinCoord(c) is floorDiv(c, 8); ChunkPos.asLong packs the signed
    // x and z cell coordinates as two's-complement 32-bit halves.
    let vein_x = chunk_x.div_euclid(8) as u32 as u64;
    let vein_z = chunk_z.div_euclid(8) as u32 as u64;
    let packed = vein_x | (vein_z << 32);
    let mut random = Xoroshiro::new((world_seed as u64 ^ packed) as i64);
    let query = random.next_int();
    // Java Math.abs(query % totalWeight), including MIN_VALUE overflow.
    let mut selected_weight = (query % total).wrapping_abs();

    for (&vein, &weight) in EARTH_REGISTRY_WEIGHT_ORDER.iter().zip(&weights) {
        selected_weight -= weight;
        if selected_weight < 0 {
            return matches!(vein, Vein::SpringWater);
        }
    }
    false
}

fn is_volcanic_biome(biome: u16) -> bool {
    matches!(
        biome,
        id::CANYONS
            | id::VOLCANIC_MOUNTAINS
            | id::VOLCANIC_OCEANIC_MOUNTAINS
            | id::ACTIVE_SHIELD_VOLCANO
            | id::DORMANT_SHIELD_VOLCANO
            | id::EXTINCT_SHIELD_VOLCANO
            | id::ANCIENT_SHIELD_VOLCANO
            | id::SUNKEN_SHIELD_VOLCANO
            | id::SHIELD_VOLCANO_SHORE
            | id::OLD_SHIELD_VOLCANO_SHORE
            | id::ICE_SHEET_SHIELD_VOLCANO
            | id::ICE_SHEET_TUYAS
            | id::ICE_SHEET_TUYAS_EDGE
            | id::GLACIATED_SHIELD_VOLCANO
            | id::TUYAS
    )
}

fn weight_for(vein: Vein, biome: u16, temperature: f32, rainfall: f32) -> i32 {
    use id::*;
    let is = |ids: &[u16]| ids.contains(&biome);
    let volcanic = is_volcanic_biome(biome);
    let mountain = is(&[VOLCANIC_MOUNTAINS, VOLCANIC_OCEANIC_MOUNTAINS]);
    let cold = is(&[
        ICE_SHEET_SHIELD_VOLCANO,
        ICE_SHEET_TUYAS,
        ICE_SHEET_TUYAS_EDGE,
        GLACIATED_SHIELD_VOLCANO,
        TUYAS,
    ]);
    let shore = is(&[SHIELD_VOLCANO_SHORE, OLD_SHIELD_VOLCANO_SHORE]);
    let true_ocean = biome == SUNKEN_SHIELD_VOLCANO;
    let sandstone = biome == CANYONS;
    let range = |value: f32, min: f32, max: f32| value >= min && value <= max;
    let climate = |tmin: f32, tmax: f32, rmin: f32, rmax: f32| {
        range(temperature, tmin, tmax) && range(rainfall, rmin, rmax)
    };

    match vein {
        Vein::SpringWater => weight_if(volcanic, 20), // EarthIsVolcanic
        Vein::GasVolcanic => weight_if(climate(-20., 30., 0., 300.), 5),
        Vein::LavaVolcanic => 20, // EarthIsVolcanic
        Vein::WaterCommon => weight_if(mountain || cold, 10),
        Vein::MuddyWater => {
            (weight_if(volcanic, 1) + weight_if(mountain, 1) + weight_if(cold, 1)) * 5
        }
        Vein::LavaMountain => weight_if(mountain, 10),
        Vein::LightOilShore => weight_if(shore, 5),
        Vein::SeaWater => weight_if(true_ocean, 100),
        Vein::GasSurface => weight_if(climate(-20., 0., 300., 500.), 50),
        Vein::GasPermafrost => weight_if(cold && climate(-20., 30., 250., 500.), 30),
        Vein::GasOcean => weight_if(true_ocean && climate(-20., 10., 200., 500.), 20),
        Vein::GasSwamp => 0,
        Vein::LightOilHot => weight_if(climate(20., 30., 0., 50.), 50),
        Vein::OilHot => weight_if(climate(20., 30., 0., 50.), 30),
        Vein::RawOilHot => weight_if(climate(20., 30., 0., 50.), 30),
        Vein::HeavyOilHot => weight_if(climate(20., 30., 0., 50.), 20),
        Vein::LightOilOcean => weight_if(true_ocean && climate(15., 30., 0., 100.), 50),
        Vein::OilOcean => weight_if(true_ocean && climate(15., 30., 0., 100.), 30),
        Vein::RawOilOcean => weight_if(true_ocean && climate(15., 30., 0., 100.), 30),
        Vein::RawOilSandstone => weight_if(sandstone && climate(10., 30., 0., 100.), 50),
        Vein::HeavyOilDry => 0,
        Vein::LightOilCommon => 10,
        Vein::OilCommon => 5,
        Vein::RawOilCommon => 5,
        Vein::GasCommon => 1,
        Vein::HeavyOilCommon => 1,
        Vein::OilNormal => 0,
    }
}

fn weight_if(condition: bool, weight: i32) -> i32 {
    if condition { weight } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volcanic_biome_tag_matches_tfg_0925_source_membership() {
        let tagged = [
            id::CANYONS,
            id::VOLCANIC_MOUNTAINS,
            id::VOLCANIC_OCEANIC_MOUNTAINS,
            id::ACTIVE_SHIELD_VOLCANO,
            id::DORMANT_SHIELD_VOLCANO,
            id::EXTINCT_SHIELD_VOLCANO,
            id::ANCIENT_SHIELD_VOLCANO,
            id::SUNKEN_SHIELD_VOLCANO,
            id::SHIELD_VOLCANO_SHORE,
            id::OLD_SHIELD_VOLCANO_SHORE,
            id::ICE_SHEET_SHIELD_VOLCANO,
            id::ICE_SHEET_TUYAS,
            id::ICE_SHEET_TUYAS_EDGE,
            id::GLACIATED_SHIELD_VOLCANO,
            id::TUYAS,
        ];
        assert_eq!(tagged.len(), 15);
        for biome in tagged {
            assert!(is_volcanic_biome(biome));
        }
        assert!(!is_volcanic_biome(id::PLAINS));
    }

    #[test]
    fn matches_independent_java_gt_vein_selection_reference() {
        // Frozen weights for these biome/climate combinations were resolved
        // from the installed TFG definition and climate tags. Goldens come
        // from tools/reference/fluid_vein_selection.java, which independently
        // ports MC 1.20.1 RandomSupport/XoroshiroRandomSource and GT's weighted
        // pick, including negative chunk-cell floorDiv.
        let cases = [
            (0, 0, 0, id::ACTIVE_SHIELD_VOLCANO, 10.0, 250.0, false),
            (0, 15, 31, id::VOLCANIC_MOUNTAINS, 25.0, 40.0, false),
            (
                123_456_789,
                64,
                -8,
                id::SUNKEN_SHIELD_VOLCANO,
                5.0,
                250.0,
                true,
            ),
            (0, -9, -17, id::VOLCANIC_MOUNTAINS, 25.0, 40.0, true),
        ];
        for (seed, cx, cz, biome, temperature, rainfall, expected) in cases {
            assert_eq!(
                selects_spring_water(seed, cx, cz, biome, temperature, rainfall),
                expected,
                "seed={seed}, chunk=({cx},{cz}), biome={biome}, temp={temperature}, rain={rainfall}"
            );
        }
    }
}
