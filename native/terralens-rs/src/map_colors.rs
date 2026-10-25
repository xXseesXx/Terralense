//! TFC's climate based grass and foliage tint colors.
//!
//! The bundled colormaps and coordinate conversion match TFC 3.2.25's
//! `TFCColors.getClimateColor`. Seasonal APIs reproduce TFC's climatological
//! monthly grass tint and seasonal foliage rules without daily weather noise.

use image::RgbImage;
use std::sync::OnceLock;

static GRASS: OnceLock<RgbImage> = OnceLock::new();
static FOLIAGE: OnceLock<RgbImage> = OnceLock::new();
static FOLIAGE_FALL: OnceLock<RgbImage> = OnceLock::new();
static FOLIAGE_WINTER: OnceLock<RgbImage> = OnceLock::new();
static WATER: OnceLock<RgbImage> = OnceLock::new();
static FOLIAGE_JACARANDA: OnceLock<RgbImage> = OnceLock::new();
static FOLIAGE_YELLOW: OnceLock<RgbImage> = OnceLock::new();
static FOLIAGE_ORANGE: OnceLock<RgbImage> = OnceLock::new();
static FOLIAGE_RED: OnceLock<RgbImage> = OnceLock::new();

/// Sample the installed TFC grass colormap using annual mean climate.
///
/// Temperature is adjusted for elevation above Y=63 using TFG's installed
/// average-temperature lapse rate. Rainfall is in millimeters per year.
#[inline]
pub fn grass(temperature: f64, rainfall: f64, y: i32) -> [u8; 3] {
    sample(
        &GRASS,
        include_bytes!("../assets/map-colors/grass.png"),
        temperature,
        rainfall,
        y,
    )
}

/// Sample the installed TFC default foliage colormap using annual mean climate.
///
/// This is the default foliage tint, not the calendar-dependent fall or winter
/// tint. Temperature is adjusted for elevation above Y=63 as in TFG.
#[inline]
pub fn foliage(temperature: f64, rainfall: f64, y: i32) -> [u8; 3] {
    sample(
        &FOLIAGE,
        include_bytes!("../assets/map-colors/foliage.png"),
        temperature,
        rainfall,
        y,
    )
}

/// Sample TFC grass tint at a simulated date in the TFC calendar.
///
/// `season` is the fraction of a year since January 1 (`None` retains the
/// existing annual-mean color exactly). Monthly temperature is modulated by
/// TFG's monthly temperature curve using the world's actual `temperature_scale`.
/// Daily weather and time-of-day variation are omitted, so every location has
/// a stable climatological color for a given date.
#[inline]
pub fn grass_at(
    temperature: f64,
    rainfall: f64,
    _x: i32,
    y: i32,
    z: i32,
    temperature_scale: f64,
    season: Option<f32>,
) -> [u8; 3] {
    let Some(date) = seasonal_date(season) else {
        return grass(temperature, rainfall, y);
    };
    let seasonal_at_y =
        averaged_monthly_temperature(temperature as f32, y, z, temperature_scale as f32, date);
    sample_temperature(
        &GRASS,
        include_bytes!("../assets/map-colors/grass.png"),
        seasonal_at_y,
        rainfall as f32,
    )
}

/// Sample TFC foliage tint at a simulated date in the TFC calendar.
///
/// The species name may be a TFC wood name (`oak`) or a generated feature id
/// such as `tfc:tree/oak_dead`. TFC's seasonal-leaves tag controls whether
/// fall/winter palettes apply; ordinary conifer and unknown-addon foliage uses
/// the monthly green palette. `None` retains the exact annual-mean foliage tint
/// produced by [`foliage`].
#[inline]
pub fn foliage_at(
    temperature: f64,
    rainfall: f64,
    x: i32,
    y: i32,
    z: i32,
    species: &str,
    temperature_scale: f64,
    season: Option<f32>,
) -> [u8; 3] {
    let Some(date) = seasonal_date(season) else {
        return foliage(temperature, rainfall, y);
    };
    if let Some(blossom) = afc_blossom(species, x, y, z, date) {
        return blossom;
    }
    if is_palm(species) {
        return palm_foliage(temperature as f32, rainfall as f32, x, y, z, species, date);
    }
    seasonal_foliage_at_date(
        temperature,
        rainfall,
        x,
        y,
        z,
        species,
        temperature_scale,
        date,
    )
}

#[inline]
fn seasonal_foliage_at_date(
    temperature: f64,
    rainfall: f64,
    x: i32,
    y: i32,
    z: i32,
    species: &str,
    temperature_scale: f64,
    date: f32,
) -> [u8; 3] {
    let temp = adjusted_average_temperature(temperature as f32, y);
    let seasonal_temp =
        averaged_monthly_temperature(temperature as f32, y, z, temperature_scale as f32, date);
    let tree_species = normalized_species(species);
    let seasonal_form = tree_species
        .strip_suffix("_krummholz")
        .or_else(|| tree_species.strip_suffix(" krummholz"))
        .unwrap_or(tree_species);
    let Some((conifer, _)) = wood_color_info(seasonal_form) else {
        return sample_temperature(
            &FOLIAGE,
            include_bytes!("../assets/map-colors/foliage.png"),
            seasonal_temp,
            rainfall as f32,
        );
    };
    if conifer && !is_krummholz(species) {
        return sample_temperature(
            &FOLIAGE,
            include_bytes!("../assets/map-colors/foliage.png"),
            seasonal_temp,
            rainfall as f32,
        );
    }
    let temp_clamped = if temp > 12.0 { 12.0 } else { temp.max(-20.0) };
    // Match Java's `(float) Math.pow(floatTerm, exponent)`, not Rust's f32
    // `powi`, which rounds differently from the double-precision Java call.
    let cubed_power = ((temp_clamped + 3.0) as f64).powi(3) as f32;
    let squared_power = ((temp_clamped + 3.0) as f64).powi(2) as f32;
    let cubed = 1.5 * cubed_power / 4913.0;
    let squared = 0.5 * squared_power / 289.0;
    let autumn_start = (cubed + squared + 8.5) / 12.0;
    let autumn_end = if temp > 12.0 {
        autumn_start
    } else {
        (cubed - squared + 10.5) / 12.0
    };
    let spring_start = 1.0 - autumn_end;

    if date > autumn_end || date <= spring_start {
        sample_temperature(
            &FOLIAGE_WINTER,
            include_bytes!("../assets/map-colors/foliage_winter.png"),
            temp,
            rainfall as f32,
        )
    } else if date > autumn_start {
        let position_delta = (tfc_hash(x, y, z) & 127) - 63;
        let progress = (255.0 * (date - autumn_start) / (autumn_end - autumn_start)
            + position_delta as f32)
            .clamp(0.0, 255.0) as i32;
        sample_pixel(
            &FOLIAGE_FALL,
            include_bytes!("../assets/map-colors/foliage_fall.png"),
            progress,
            autumn_index(species),
        )
    } else {
        sample_temperature(
            &FOLIAGE,
            include_bytes!("../assets/map-colors/foliage.png"),
            seasonal_temp,
            rainfall as f32,
        )
    }
}

/// TFC's registered color for `wood/fallen_leaves/<species>` blocks. Conifers
/// use their fixed orange tint; deciduous species use the seasonal foliage
/// palette, including the species-specific autumn index.
#[inline]
pub fn fallen_leaves_at(
    temperature: f64,
    rainfall: f64,
    x: i32,
    y: i32,
    z: i32,
    species: &str,
    temperature_scale: f64,
    season: Option<f32>,
) -> [u8; 3] {
    if wood_color_info(species).is_some_and(|(conifer, _)| conifer) {
        [0xcf, 0x7d, 0x13]
    } else if let Some(date) = seasonal_date(season) {
        // Fallen leaves use the registered seasonal tint, without living-leaf
        // blossom or palm lifecycle color handlers.
        seasonal_foliage_at_date(
            temperature,
            rainfall,
            x,
            y,
            z,
            species,
            temperature_scale,
            date,
        )
    } else {
        foliage(temperature, rainfall, y)
    }
}

/// Sample the TFC water colormap at the fixed sea-level position used by
/// `TFCColors.waterColorResolver`, with averaged TFG monthly temperature.
/// `None` samples annual mean temperature; callers can omit this layer when
/// they need to retain a fixed map water color.
#[inline]
pub fn water_at(
    temperature: f64,
    rainfall: f64,
    z: i32,
    temperature_scale: f64,
    season: Option<f32>,
) -> [u8; 3] {
    let temp = match seasonal_date(season) {
        Some(date) => {
            averaged_monthly_temperature(temperature as f32, 63, z, temperature_scale as f32, date)
        }
        None => temperature as f32,
    };
    sample_temperature(
        &WATER,
        include_bytes!("../assets/map-colors/water.png"),
        temp,
        rainfall as f32,
    )
}

#[inline]
fn sample(
    cache: &'static OnceLock<RgbImage>,
    encoded: &'static [u8],
    temperature: f64,
    rainfall: f64,
    y: i32,
) -> [u8; 3] {
    // TFC's climate API and constants are f32. Keep each operation in f32 so
    // index boundaries match Java rather than calculating in f64 then casting.
    let temperature = adjusted_average_temperature(temperature as f32, y);
    sample_temperature(cache, encoded, temperature, rainfall as f32)
}

#[inline]
fn sample_temperature(
    cache: &'static OnceLock<RgbImage>,
    encoded: &'static [u8],
    temperature: f32,
    rainfall: f32,
) -> [u8; 3] {
    let temperature_index = 255 - java_clamp_index((temperature + 20.0) * 255.0 / 50.0);
    let rainfall_index = 255 - java_clamp_index(rainfall * 255.0 / 500.0);
    sample_pixel(cache, encoded, temperature_index, rainfall_index)
}

#[inline]
fn sample_pixel(
    cache: &'static OnceLock<RgbImage>,
    encoded: &'static [u8],
    x: i32,
    y: i32,
) -> [u8; 3] {
    let image = cache.get_or_init(|| {
        image::load_from_memory(encoded)
            .expect("bundled TFC colormap PNG must decode")
            .to_rgb8()
    });
    let pixel = image.get_pixel(x.clamp(0, 255) as u32, y.clamp(0, 255) as u32);
    [pixel[0], pixel[1], pixel[2]]
}

#[inline]
fn seasonal_date(date: Option<f32>) -> Option<f32> {
    date.filter(|value| value.is_finite())
        .map(|value| value.rem_euclid(1.0))
}

#[inline]
fn month_temperature_modifier(date: f32) -> f32 {
    // TFC Month enum values; calendar fraction is Jan 1 = 0 and each month is
    // one twelfth of a year. Interpolate between month-start modifiers exactly
    // as OverworldClimateModel.getTemperature does.
    const MONTHS: [f32; 12] = [
        -1.0, -0.866, -0.5, 0.0, 0.5, 0.866, 1.0, 0.866, 0.5, 0.0, -0.5, -0.866,
    ];
    let year_month = date * 12.0;
    let month = year_month.floor() as usize % 12;
    let delta = year_month - year_month.floor();
    MONTHS[month] + (MONTHS[(month + 1) % 12] - MONTHS[month]) * delta
}

#[inline]
fn averaged_monthly_temperature(
    average: f32,
    y: i32,
    z: i32,
    temperature_scale: f32,
    date: f32,
) -> f32 {
    let month_factor = month_temperature_modifier(date);
    let monthly = if temperature_scale == 0.0 {
        0.0
    } else {
        // TFG OverworldClimateModelMixin.calculateMonthlyTemperature:
        // triangle(-18, 0, 1/(4*scale), z - scale/2) * monthFactor.
        let frequency = 1.0 / (4.0 * temperature_scale);
        let latitude = z as f32 - temperature_scale / 2.0;
        month_factor * tfc_triangle(-18.0, 0.0, frequency, latitude)
    };
    if y > 63 {
        adjusted_average_temperature(average, y) + monthly
    } else if y > 0 {
        average + monthly * (y as f32 / 63.0)
    } else {
        // OverworldClimateModel.adjustTemperatureByElevation blends deep
        // temperatures toward the fixed 15 C lava-level climate at Y=-64.
        let depth_influence = (y as f32 - (-64.0)) / (0.0 - (-64.0));
        15.0 + (average - 15.0) * depth_influence
    }
}

#[inline]
fn tfc_triangle(amplitude: f32, midpoint: f32, frequency: f32, value: f32) -> f32 {
    // Helpers.triangle from the pinned TFC source. Java floor semantics are
    // important for negative Z coordinates.
    let phase = 4.0 * frequency * value + 1.0 - 4.0 * (frequency * value + 0.75).floor();
    midpoint + amplitude * (phase.abs() - 1.0)
}

#[inline]
fn tfc_hash(x: i32, y: i32, z: i32) -> i32 {
    tfc_hash_with_salt(836_494_186_029_734_123, x, y, z)
}

#[inline]
fn tfc_hash_with_salt(salt: i64, x: i32, y: i32, z: i32) -> i32 {
    // Helpers.hash(salt, x, y, z), with Java long overflow.
    const PRIME_X: i64 = 501_125_321;
    const PRIME_Y: i64 = 1_136_930_381;
    let mut hash =
        salt ^ (x as i64).wrapping_mul(PRIME_X) ^ (y as i64).wrapping_mul(PRIME_Y) ^ z as i64;
    hash = hash.wrapping_mul(0x27d4eb2d);
    hash as i32
}

#[inline]
fn autumn_index(species: &str) -> i32 {
    let species = normalized_species(species);
    let species = species
        .strip_suffix("_krummholz")
        .or_else(|| species.strip_suffix(" krummholz"))
        .unwrap_or(species);
    wood_color_info(species).map_or(0, |(_, index)| index)
}

#[inline]
fn is_krummholz(species: &str) -> bool {
    let species = normalized_species(species);
    species.ends_with("_krummholz") || species.ends_with(" krummholz")
}

#[inline]
fn normalized_species(species: &str) -> &str {
    let id = species.rsplit('/').next().unwrap_or(species);
    let id = id.strip_suffix("_entry").unwrap_or(id);
    let id = id.strip_suffix("_dead").unwrap_or(id);
    let id = id.strip_suffix("_large").unwrap_or(id);
    let id = id
        .rsplit_once('_')
        .filter(|(_, age)| !age.is_empty() && age.bytes().all(|b| b.is_ascii_digit()))
        .map_or(id, |(name, _)| name);
    // Saved canopy labels use spaces; seed features use registry underscores.
    // Match interned aliases without allocating a String for every leaf pixel.
    match id {
        "acacia koa" => "acacia_koa",
        "atlas cedar" => "atlas_cedar",
        "bald cypress" => "bald_cypress",
        "balsam fir" => "balsam_fir",
        "bigleaf maple" => "bigleaf_maple",
        "black oak" => "black_oak",
        "black spruce" => "black_spruce",
        "coast redwood" => "coast_redwood",
        "coast spruce" => "coast_spruce",
        "douglas fir" => "douglas_fir",
        "flame of the forest" => "flame_of_the_forest",
        "giant rosewood" => "giant_rosewood",
        "gum arabic" => "gum_arabic",
        "horsetail ironwood" => "horsetail_ironwood",
        "iroko teak" => "iroko_teak",
        "jaggery palm" => "jaggery_palm",
        "lebombo ironwood" => "lebombo_ironwood",
        "live oak" => "live_oak",
        "mountain ash" => "mountain_ash",
        "mountain fir" => "mountain_fir",
        "mpingo blackwood" => "mpingo_blackwood",
        "oil palm" => "oil_palm",
        "rainbow eucalyptus" => "rainbow_eucalyptus",
        "red pine" => "red_pine",
        "red silk cotton" => "red_silk_cotton",
        "rubber fig" => "rubber_fig",
        "sapele mahogany" => "sapele_mahogany",
        "scrub hickory" => "scrub_hickory",
        "sitka spruce" => "sitka_spruce",
        "small leaf mahogany" => "small_leaf_mahogany",
        "stone pine" => "stone_pine",
        "weeping cypress" => "weeping_cypress",
        "weeping maple" => "weeping_maple",
        "weeping willow" => "weeping_willow",
        "white cedar" => "white_cedar",
        _ => id,
    }
}

#[inline]
fn wood_color_info(species: &str) -> Option<(bool, i32)> {
    // TFC Wood enum + AFC 1.0.22 registered AFCWood/TreeSpecies tables.
    // The matching source values are retained in afc-species-indices.json.
    let id = normalized_species(species);
    Some(match id {
        "acacia" => (false, 210),
        "ash" => (false, 10),
        "aspen" => (false, 250),
        "birch" => (false, 145),
        "blackwood" => (false, 80),
        "chestnut" => (false, 40),
        "douglas_fir" => (true, 0),
        "hickory" => (false, 230),
        "kapok" => (false, 30),
        "mangrove" => (false, 100),
        "maple" => (false, 0),
        "oak" => (false, 120),
        "palm" => (false, 255),
        "pine" => (true, 0),
        "rosewood" => (false, 170),
        "sequoia" => (true, 0),
        "spruce" => (true, 0),
        "white_cedar" => (true, 0),
        "sycamore" => (false, 200),
        "willow" => (false, 225),
        // AFC wood species (bytecode enum-constructor oracle).
        "baobab" => (false, 212),
        "eucalyptus" => (false, 150),
        "mahogany" => (false, 10),
        "hevea" => (false, 130),
        "tualang" => (false, 226),
        "teak" => (false, 240),
        "cypress" => (true, 0),
        "fig" => (false, 250),
        "ironwood" => (false, 200),
        "ipe" => (false, 254),
        // AFC tree species.
        "gum_arabic" => (false, 196),
        "acacia_koa" => (false, 180),
        "poplar" => (false, 250),
        "mpingo_blackwood" => (false, 200),
        "mountain_fir" | "balsam_fir" => (true, 0),
        "scrub_hickory" => (false, 220),
        "red_silk_cotton" => (false, 150),
        "bigleaf_maple" => (false, 215),
        "weeping_maple" => (true, 0),
        "black_oak" => (false, 180),
        "live_oak" => (false, 155),
        "jaggery_palm" => (false, 249),
        "stone_pine" | "red_pine" => (true, 0),
        "tamarack" => (false, 254),
        "giant_rosewood" => (false, 190),
        "coast_redwood" | "coast_spruce" | "sitka_spruce" | "black_spruce" | "atlas_cedar"
        | "redcedar" | "weeping_cypress" => (true, 0),
        "weeping_willow" => (false, 240),
        "rainbow_eucalyptus" => (false, 30),
        "mountain_ash" => (false, 150),
        "rubber_fig" => (false, 80),
        "bald_cypress" => (false, 130),
        "sapele_mahogany" => (false, 170),
        "small_leaf_mahogany" => (false, 240),
        "iroko_teak" => (false, 140),
        "flame_of_the_forest" => (false, 0),
        "lebombo_ironwood" => (false, 230),
        "horsetail_ironwood" => (false, 220),
        _ => return None,
    })
}

#[inline]
fn is_palm(species: &str) -> bool {
    matches!(
        normalized_species(species),
        "coconut" | "oil_palm" | "date" | "acai"
    )
}

#[inline]
fn palm_foliage(
    average_temperature: f32,
    rainfall: f32,
    x: i32,
    y: i32,
    z: i32,
    species: &str,
    date: f32,
) -> [u8; 3] {
    let month = (date * 12.0).floor() as usize % 12;
    let (mut row, min_temp, max_temp, min_hydration, max_hydration, dormant) =
        match normalized_species(species) {
            "coconut" => (
                150,
                15.0,
                45.0,
                35,
                100,
                [0, 1, 2, 3, 7, 8].contains(&month),
            ),
            "oil_palm" => (200, 19.0, 62.0, 10, 100, [1, 2, 7, 8, 9].contains(&month)),
            "date" => (255, 20.0, 50.0, 40, 100, month <= 7),
            _ => (255, 24.0, 70.0, 10, 100, month >= 4), // acai
        };
    let hydration = (rainfall / 5.0) as i32;
    if average_temperature >= min_temp
        && average_temperature <= max_temp
        && hydration >= min_hydration
        && hydration <= max_hydration
    {
        if dormant {
            row += 50;
        }
    } else {
        row += 100;
    }
    let in_range = average_temperature >= min_temp
        && average_temperature <= max_temp
        && hydration >= min_hydration
        && hydration <= max_hydration;
    let color_temperature = if in_range {
        average_temperature + if dormant { 50.0 } else { 0.0 }
    } else {
        average_temperature + 100.0
    };
    let temperature_index = 255 - java_clamp_index((color_temperature + 20.0) * 255.0 / 50.0);
    let jitter = (tfc_hash_with_salt(836_494_186_029_734_123, x, y, z) & 7) - 3;
    sample_pixel(
        &FOLIAGE,
        include_bytes!("../assets/map-colors/foliage.png"),
        temperature_index + jitter,
        row,
    )
}

#[inline]
fn afc_blossom(species: &str, x: i32, y: i32, z: i32, date: f32) -> Option<[u8; 3]> {
    let id = normalized_species(species);
    let palette = match id {
        "ipe" => Some(("yellow", [2, 3])),
        "giant_rosewood" => Some(("jacaranda", [2, 3])),
        "red_silk_cotton" => Some(("red", [3, 4])),
        "flame_of_the_forest" => Some(("orange", [2, 3])),
        _ => None,
    }?;
    let year_month = date * 12.0;
    let month = year_month.floor() as i32;
    let month_fraction = year_month - month as f32;
    let delta = month_fraction - 0.5;
    let switch_chance = 16.0 * delta.powi(4);
    let month_hash = (tfc_hash_with_salt(836_494_186_029_734_123, x, y, z) & 255) as f32;
    // AFC 1.0.22 bytecode uses `delta < .5` for the previous-month branch.
    // Since delta is always < .5 in a month, this historical behavior is kept.
    let adjusted_month = if month_hash < 256.0 * switch_chance {
        (month - 1).rem_euclid(12)
    } else {
        month
    };
    if !palette.1.contains(&adjusted_month) {
        return None;
    }
    let hash = tfc_hash_with_salt(91_273_491_823_412_341, x, y, z) as u32;
    let image = match palette.0 {
        "yellow" => (
            &FOLIAGE_YELLOW,
            include_bytes!("../assets/map-colors/foliage_yellow.png").as_slice(),
        ),
        "jacaranda" => (
            &FOLIAGE_JACARANDA,
            include_bytes!("../assets/map-colors/foliage_jacaranda.png").as_slice(),
        ),
        "red" => (
            &FOLIAGE_RED,
            include_bytes!("../assets/map-colors/foliage_red.png").as_slice(),
        ),
        _ => (
            &FOLIAGE_ORANGE,
            include_bytes!("../assets/map-colors/foliage_orange.png").as_slice(),
        ),
    };
    Some(sample_pixel(
        image.0,
        image.1,
        (hash & 255) as i32,
        ((hash >> 8) & 255) as i32,
    ))
}

#[inline]
fn java_clamp_index(value: f32) -> i32 {
    // Rust's float-to-int cast has the same relevant behavior as Java's: it
    // truncates toward zero, saturates infinities, and converts NaN to zero.
    (value as i32).clamp(0, 255)
}

#[inline]
fn adjusted_average_temperature(average: f32, y: i32) -> f32 {
    if y <= 63 {
        return average;
    }
    let cooling = ((y as f32 - 63.0) * 0.16225).clamp(0.0, 17.822);
    average - cooling
}

#[cfg(test)]
mod tests {
    use super::{
        adjusted_average_temperature, fallen_leaves_at, foliage, foliage_at, grass, grass_at,
        month_temperature_modifier, seasonal_date, water_at,
    };

    #[test]
    fn fallen_leaf_tint_uses_fixed_conifer_or_species_seasonal_color() {
        assert_eq!(
            fallen_leaves_at(10.0, 250.0, -50, 70, 120, "pine", 20_000.0, Some(0.5)),
            [0xcf, 0x7d, 0x13]
        );
        assert_eq!(
            fallen_leaves_at(10.0, 250.0, -50, 70, 120, "oak", 20_000.0, Some(0.875)),
            foliage_at(10.0, 250.0, -50, 70, 120, "oak", 20_000.0, Some(0.875))
        );
    }

    #[test]
    fn saved_species_labels_feature_ids_and_palm_ages_share_seasonal_rules() {
        for (saved, generated) in [
            ("white cedar", "tfc:tree/white_cedar"),
            ("red silk cotton", "afc:tree/red_silk_cotton"),
            ("giant rosewood", "afc:tree/giant_rosewood"),
            ("flame of the forest", "afc:tree/flame_of_the_forest"),
            ("oil palm", "tfg:palm_tree/oil_palm_7"),
            ("aspen krummholz", "tfc:tree/aspen_krummholz"),
        ] {
            for month in [0., 2.75, 7., 10.] {
                let tint = |species| {
                    foliage_at(
                        8.,
                        250.,
                        120,
                        63,
                        -16000,
                        species,
                        20000.,
                        Some(month / 12.),
                    )
                };
                assert_eq!(tint(saved), tint(generated), "{saved} in month {month}");
            }
        }
    }

    #[test]
    fn bundled_tfc_colormap_pixels_are_sampled_at_discrete_java_indices() {
        // These RGB values were captured directly from the unmodified source
        // PNGs at the expected Java indices (not produced by this sampler).
        assert_eq!(grass(10.0, 250.0, 63), [93, 155, 82]);
        assert_eq!(foliage(10.0, 250.0, 63), [98, 131, 57]);
        assert_eq!(grass(0.0, 0.0, 63), [249, 209, 90]);
        assert_eq!(foliage(0.0, 0.0, 63), [151, 147, 46]);
        assert_eq!(grass(-20.0, 500.0, 63), [109, 152, 121]);
        assert_eq!(foliage(-20.0, 500.0, 63), [87, 119, 109]);
    }

    #[test]
    fn elevation_adjustment_and_clamped_edges_match_tfg() {
        assert_eq!(adjusted_average_temperature(18.0, 63), 18.0);
        assert_eq!(adjusted_average_temperature(18.0, 40), 18.0);
        assert!((adjusted_average_temperature(18.0, 100) - 11.996_75).abs() < 1e-5);
        assert!((adjusted_average_temperature(18.0, 200) - 0.178).abs() < 1e-5);
        assert!((adjusted_average_temperature(18.0, i32::MAX) - 0.178).abs() < 1e-5);

        // NaN follows Java's `(int) NaN == 0`; extreme values clamp to the
        // source image edges, so all non-finite user inputs remain safe.
        assert_eq!(grass(f64::NAN, f64::NAN, 63), [202, 190, 100]);
        assert_eq!(grass(f64::INFINITY, f64::INFINITY, 63), [33, 124, 62]);
        assert_eq!(
            grass(f64::NEG_INFINITY, f64::NEG_INFINITY, 63),
            [202, 190, 100]
        );
    }

    #[test]
    fn absent_or_invalid_date_preserves_annual_mean_colors() {
        for season in [None, Some(f32::NAN), Some(f32::INFINITY)] {
            assert_eq!(
                grass_at(10.0, 250.0, -1234, 100, 2345, 20_000.0, season),
                grass(10.0, 250.0, 100)
            );
            assert_eq!(
                foliage_at(
                    10.0,
                    250.0,
                    -1234,
                    100,
                    2345,
                    "tfc:tree/oak",
                    20_000.0,
                    season
                ),
                foliage(10.0, 250.0, 100)
            );
        }
    }

    #[test]
    fn calendar_date_is_january_based_and_wraps_like_tfc_calendar() {
        assert_eq!(seasonal_date(Some(0.0)), Some(0.0));
        assert_eq!(seasonal_date(Some(1.0)), Some(0.0));
        assert_eq!(seasonal_date(Some(-0.25)), Some(0.75));
        assert!((month_temperature_modifier(0.0) + 1.0).abs() < 1e-6);
        assert!((month_temperature_modifier(0.5) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn seasonal_grass_uses_world_scale_and_averaged_monthly_temperature() {
        let january = grass_at(10.0, 250.0, 0, 63, 0, 20_000.0, Some(0.0));
        let july = grass_at(10.0, 250.0, 0, 63, 0, 20_000.0, Some(0.5));
        assert_ne!(january, july);

        // A zero temperature scale disables seasonal temperature just as in
        // OverworldClimateModel.calculateMonthlyTemperature.
        assert_eq!(
            grass_at(10.0, 250.0, 0, 63, 0, 0.0, Some(0.5)),
            grass(10.0, 250.0, 63)
        );
        // The source climate model suppresses monthly temperature at and below
        // Y=0; the annual tint is used unchanged there.
        assert_eq!(
            grass_at(10.0, 250.0, 0, 0, 0, 20_000.0, Some(0.5)),
            grass(10.0, 250.0, 0)
        );
    }

    #[test]
    fn seasonal_foliage_uses_installed_fall_and_winter_palettes() {
        let summer = foliage_at(10.0, 250.0, 120, 63, -50, "oak", 20_000.0, Some(0.5));
        let winter = foliage_at(10.0, 250.0, 120, 63, -50, "oak", 20_000.0, Some(0.0));
        let autumn = foliage_at(10.0, 250.0, 120, 63, -50, "oak", 20_000.0, Some(0.875));
        assert_eq!(summer, [91, 132, 49]); // independent TFG monthly-color sample
        assert_ne!(winter, summer);
        assert_ne!(autumn, summer);
        assert_ne!(autumn, winter);
    }

    #[test]
    fn species_controls_tfc_fall_palette_row() {
        let oak = foliage_at(
            10.0,
            250.0,
            120,
            63,
            -50,
            "tfc:tree/oak_dead",
            20_000.0,
            Some(0.875),
        );
        let aspen = foliage_at(10.0, 250.0, 120, 63, -50, "aspen", 20_000.0, Some(0.875));
        assert_ne!(oak, aspen);
    }

    #[test]
    fn monthly_temperature_matches_pinned_tfg_triangle_oracle_values() {
        use super::averaged_monthly_temperature as tfg_average_month;

        // Independent evaluations of the installed Java expression:
        // monthFactor * Helpers.triangle(-18, 0, 1/(4*scale), z-scale/2).
        let january = [
            (0, 1.0),
            (10_000, 10.0),
            (20_000, 19.0),
            (30_000, 28.0),
            (40_000, 19.0),
            (50_000, 10.0),
            (60_000, 1.0),
            (70_000, -8.0),
        ];
        for (z, expected) in january {
            assert_eq!(tfg_average_month(10.0, 63, z, 20_000.0, 0.0), expected);
        }
        assert_eq!(tfg_average_month(10.0, 63, 0, 20_000.0, 0.5), 19.0);
        assert_eq!(tfg_average_month(10.0, 63, 30_000, 20_000.0, 0.5), -8.0);
        assert_eq!(tfg_average_month(10.0, 63, 0, 0.0, 0.0), 10.0);
        assert_eq!(tfg_average_month(10.0, 0, 0, 20_000.0, 0.5), 10.0);

        // TFGNoiseHelpers.adjustAverageTemperatureByElevation(y, average, 63)
        // uses this lapse rate and cap.
        assert!((tfg_average_month(10.0, 64, 0, 0.0, 0.0) - 9.83775).abs() < 1e-5);
        assert!((tfg_average_month(10.0, 200, 0, 0.0, 0.0) - (-7.822)).abs() < 1e-5);
    }

    #[test]
    fn ordinary_conifers_and_unknown_addon_foliage_use_monthly_green_palette() {
        let january = foliage_at(10.0, 250.0, 120, 63, 0, "pine", 20_000.0, Some(0.0));
        let july = foliage_at(10.0, 250.0, 120, 63, 0, "pine", 20_000.0, Some(0.5));
        assert_ne!(january, july);
        let unknown = foliage_at(
            10.0,
            250.0,
            120,
            63,
            0,
            "addon:unregistered_tree",
            20_000.0,
            Some(0.0),
        );
        assert_ne!(unknown, foliage(10.0, 250.0, 63));
    }

    #[test]
    fn water_colormap_uses_tfg_monthly_climate_at_sea_level() {
        let january = water_at(10.0, 250.0, 0, 20_000.0, Some(0.0));
        let july = water_at(10.0, 250.0, 0, 20_000.0, Some(0.5));
        assert_ne!(january, july);
        assert_eq!(
            water_at(10.0, 250.0, 0, 20_000.0, None),
            water_at(10.0, 250.0, 0, 20_000.0, Some(f32::NAN))
        );
    }

    #[test]
    fn source_registered_afc_species_select_conifer_and_autumn_colors() {
        assert_eq!(super::wood_color_info("afc:tree/ipe"), Some((false, 254)));
        assert_eq!(super::wood_color_info("afc:tree/cypress"), Some((true, 0)));
        assert_eq!(
            super::wood_color_info("tfc:tree/bigleaf_maple_dead"),
            Some((false, 215))
        );
        assert!(!super::wood_color_info("tfc:tree/bigleaf_maple").unwrap().0);
        let evergreen = foliage_at(
            10.0,
            250.0,
            120,
            63,
            0,
            "afc:tree/cypress",
            20_000.0,
            Some(0.0),
        );
        let deciduous = foliage_at(
            10.0,
            250.0,
            120,
            63,
            0,
            "afc:tree/bigleaf_maple",
            20_000.0,
            Some(0.0),
        );
        assert_ne!(evergreen, deciduous);
    }

    #[test]
    fn afc_blossom_colormaps_activate_in_their_registered_months() {
        let march = foliage_at(10.0, 250.0, 120, 63, 0, "ipe", 20_000.0, Some(2.5 / 12.0));
        let january = foliage_at(10.0, 250.0, 120, 63, 0, "ipe", 20_000.0, Some(0.0));
        assert_ne!(march, january);
        assert_ne!(
            march,
            foliage_at(10.0, 250.0, 120, 63, 0, "oak", 20_000.0, Some(2.5 / 12.0))
        );
    }

    #[test]
    fn palms_follow_tfg_climate_ranges_dormancy_and_base_leaf_palette() {
        let coconut_january = foliage_at(
            25.0,
            250.0,
            120,
            63,
            0,
            "tfg:palm_tree/coconut",
            20_000.0,
            Some(0.0),
        );
        let coconut_june = foliage_at(
            25.0,
            250.0,
            120,
            63,
            0,
            "tfg:palm_tree/coconut",
            20_000.0,
            Some(5.0 / 12.0),
        );
        assert_ne!(coconut_january, coconut_june);
        let out_of_range = foliage_at(5.0, 10.0, 120, 63, 0, "coconut", 20_000.0, Some(5.0 / 12.0));
        assert_ne!(out_of_range, coconut_june);
    }

    #[test]
    fn rust_colors_match_independent_java_source_and_png_oracle() {
        #[derive(Debug, serde::Deserialize)]
        struct Fixture {
            kind: String,
            temperature: f64,
            rainfall: f64,
            x: i32,
            y: i32,
            z: i32,
            temperature_scale: f64,
            season: Option<f32>,
            species: String,
            expected_rgb: [u8; 3],
        }
        #[derive(serde::Deserialize)]
        struct FixtureFile {
            samples: Vec<Fixture>,
        }
        let fixture: FixtureFile =
            serde_json::from_str(include_str!("../tests/fixtures/season-colors-java.json"))
                .expect("source-backed Java seasonal color fixture parses");
        assert_eq!(fixture.samples.len(), 385);
        for (index, row) in fixture.samples.into_iter().enumerate() {
            let actual = match row.kind.as_str() {
                "grass" => grass_at(
                    row.temperature,
                    row.rainfall,
                    row.x,
                    row.y,
                    row.z,
                    row.temperature_scale,
                    row.season,
                ),
                "water" => water_at(
                    row.temperature,
                    row.rainfall,
                    row.z,
                    row.temperature_scale,
                    row.season,
                ),
                "foliage" => foliage_at(
                    row.temperature,
                    row.rainfall,
                    row.x,
                    row.y,
                    row.z,
                    &row.species,
                    row.temperature_scale,
                    row.season,
                ),
                kind => panic!("unknown Java oracle kind {kind}"),
            };
            assert_eq!(
                actual, row.expected_rgb,
                "Java oracle case {index}: {row:?}"
            );
        }
    }
}
