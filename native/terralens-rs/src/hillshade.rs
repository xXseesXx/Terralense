//! Small, deterministic Lambert hillshade helper for elevation overlays.
//!
//! Inputs are cardinal-neighbour heights and their horizontal spacing. Callers should
//! preserve ocean colors and skip the effect when any height is unavailable.

/// Apply NW-lit hillshade to one RGB pixel using west/east/north/south heights.
///
/// Flat terrain maps exactly to its input color. Invalid samples, non-positive spacing,
/// or zero strength also return the original color. The slope is normalized by `step`,
/// so equivalent slopes at different raster resolutions shade consistently.
pub fn shade_rgb(
    rgb: [u8; 3],
    west: f64,
    east: f64,
    north: f64,
    south: f64,
    step: f64,
    strength: f32,
) -> [u8; 3] {
    if ![west, east, north, south, step]
        .into_iter()
        .all(f64::is_finite)
        || step <= 0.0
        || !strength.is_finite()
    {
        return rgb;
    }
    let strength = strength.clamp(0.0, 1.0) as f64;
    if strength <= f64::EPSILON {
        return rgb;
    }

    // World Z increases southward. A positive east/south slope tilts the surface
    // normal toward the northwest source of light.
    let dz_dx = (east - west) / (2.0 * step);
    let dz_dz = (south - north) / (2.0 * step);
    let inv_len = (dz_dx * dz_dx + dz_dz * dz_dz + 1.0).sqrt().recip();
    let normal = (-dz_dx * inv_len, inv_len, -dz_dz * inv_len);

    // Northwest azimuth, 45 degree altitude. Normalize illumination against a flat
    // surface so flat RGB remains exact and slopes only modulate local contrast.
    let horizontal = std::f64::consts::FRAC_1_SQRT_2 * std::f64::consts::FRAC_1_SQRT_2;
    let flat_light = std::f64::consts::FRAC_1_SQRT_2;
    let lit = (normal.0 * -horizontal + normal.1 * flat_light + normal.2 * -horizontal).max(0.0);
    let illumination = (lit / flat_light).clamp(0.0, 1.5);
    let multiplier = ((1.0 - strength) + strength * illumination).clamp(0.5, 1.45);
    rgb.map(|channel| (channel as f64 * multiplier).round().clamp(0.0, 255.0) as u8)
}

/// Shade a pixel with Xaero World Map 1.47's default Overworld terrain style.
///
/// Xaero's `terrain_slopes=2` uses the height difference from the preceding
/// northern and north-western map columns, quantizes its direct-light cosine to
/// tenths, then combines that with 0.2 colored ambient + 0.5 white ambient. Its
/// `terrain_depth=true` additionally multiplies by `clamp(y / 63, 0.9, 1.0)`.
/// `spacing` is accepted to make the raster sampling contract explicit, but the
/// source slope rule uses height differences directly rather than dividing by
/// map scale. Heights should be the final visible map block's effective Y. In
/// Xaero, transparent water is an overlay on the final terrain block, so water
/// columns retain the seabed's height for these calculations.
///
/// `strength` linearly blends Xaero's resulting multiplier with unshaded RGB.
/// Values use f32 math and Java's truncation points to preserve the source
/// renderer's quantization behavior.
pub fn shade_rgb_xaero(
    rgb: [u8; 3],
    center: f64,
    north: f64,
    northwest: f64,
    spacing: f64,
    strength: f32,
) -> [u8; 3] {
    if ![center, north, northwest, spacing]
        .into_iter()
        .all(f64::is_finite)
        || spacing <= 0.0
        || !strength.is_finite()
    {
        return rgb;
    }

    let strength = strength.clamp(0.0, 1.0);
    if strength <= f32::EPSILON {
        return rgb;
    }

    // MapBlock.fixHeightType stores each difference in a signed byte.
    let vertical = ((center - north) as i32).clamp(i8::MIN as i32, i8::MAX as i32) as f32;
    let diagonal = ((center - northwest) as i32).clamp(i8::MIN as i32, i8::MAX as i32) as f32;
    let cross_z = -vertical;
    let mut cosine = 0.0_f32;
    if cross_z < 1.0 {
        if vertical == 1.0 && diagonal == 1.0 {
            cosine = 1.0;
        } else {
            let cross_x = vertical - diagonal;
            let cast = 1.0 - cross_z;
            let magnitude = (cross_x * cross_x + 1.0 + cross_z * cross_z).sqrt();
            // MapPixel promotes the already-f32 quotient to double for division
            // by Math.sqrt(2.0), then narrows the result back to float.
            cosine = ((cast / magnitude) as f64 / 2.0_f64.sqrt()) as f32;
        }
    }

    let direct = if cosine == 1.0 {
        0.6666667_f32
    } else if cosine > 0.0 {
        ((cosine * 10.0).ceil() / 10.0) * 0.6666667_f32 * 0.88388_f32
    } else {
        0.0
    };
    let white_light = 0.5_f32 + direct;
    let slope_multiplier = 1.0_f32 * 0.2_f32 + white_light;
    let visible_y = center as i32 as f32;
    let depth_multiplier = (visible_y / 63.0_f32).clamp(0.9, 1.0);
    let target = slope_multiplier * depth_multiplier;
    let multiplier = 1.0_f32 + (target - 1.0_f32) * strength;

    rgb.map(|channel| ((channel as f32 * multiplier) as i32).clamp(0, 255) as u8)
}

#[cfg(test)]
mod tests {
    use super::{shade_rgb, shade_rgb_xaero};

    #[test]
    fn flat_height_keeps_rgb_exact() {
        let color = [37, 128, 241];
        assert_eq!(shade_rgb(color, 12., 12., 12., 12., 1., 0.8), color);
    }

    #[test]
    fn opposing_slopes_follow_northwest_light() {
        let color = [120, 120, 120];
        let lit = shade_rgb(color, 0., 20., 10., 10., 10., 1.0);
        let dark = shade_rgb(color, 20., 0., 10., 10., 10., 1.0);
        assert!(lit[0] > color[0]);
        assert!(dark[0] < color[0]);
    }

    #[test]
    fn normalized_slope_is_resolution_independent() {
        let color = [100, 140, 200];
        let coarse = shade_rgb(color, 0., 20., 10., 10., 10., 0.7);
        let fine = shade_rgb(color, 0., 2., 1., 1., 1., 0.7);
        assert_eq!(coarse, fine);
    }

    #[test]
    fn invalid_or_missing_heights_leave_color_unchanged() {
        let color = [1, 128, 255];
        assert_eq!(shade_rgb(color, f64::NAN, 1., 0., 0., 1., 0.8), color);
        assert_eq!(shade_rgb(color, 0., 1., 0., 0., 0., 0.8), color);
        assert_eq!(shade_rgb(color, 0., 1., 0., 0., 1., f32::NAN), color);
    }

    #[test]
    fn xaero_default_3d_slopes_match_independent_reference_values() {
        let rgb = [120, 120, 120];
        // Reference values are independently evaluated from MapPixel's default
        // Overworld constants (ambient .2/.5, max direct 2/3, factor .88388).
        assert_eq!(shade_rgb_xaero(rgb, 63., 63., 63., 1., 1.), [140, 140, 140]);
        assert_eq!(shade_rgb_xaero(rgb, 63., 62., 62., 1., 1.), [164, 164, 164]);
        assert_eq!(shade_rgb_xaero(rgb, 63., 64., 63., 1., 1.), [84, 84, 84]);
        assert_eq!(shade_rgb_xaero(rgb, 50., 50., 50., 1., 1.), [126, 126, 126]);
        assert_eq!(shade_rgb_xaero(rgb, 80., 80., 80., 8., 1.), [140, 140, 140]);
    }

    #[test]
    fn xaero_shading_strength_and_invalid_inputs_are_safe() {
        let rgb = [100, 160, 220];
        assert_eq!(shade_rgb_xaero(rgb, 70., 69., 69., 1., 0.), rgb);
        assert_eq!(
            shade_rgb_xaero(rgb, 70., 69., 69., 1., 0.5),
            [118, 189, 255]
        );
        assert_eq!(shade_rgb_xaero(rgb, f64::NAN, 69., 69., 1., 1.), rgb);
        assert_eq!(shade_rgb_xaero(rgb, 70., 69., 69., 0., 1.), rgb);
    }

    #[test]
    fn xaero_default_matches_independent_java_sweep() {
        let heights = [-32, 0, 56, 57, 62, 63, 64, 100, 256, 319];
        let mut hash = 0xcbf29ce484222325_u64;
        let mut samples = 0_u64;
        for y in heights {
            for vertical in -160..=160 {
                for diagonal in -160..=160 {
                    let color = shade_rgb_xaero(
                        [83, 149, 221],
                        y as f64,
                        (y - vertical) as f64,
                        (y - diagonal) as f64,
                        1.0,
                        1.0,
                    );
                    for channel in color {
                        hash = (hash ^ u64::from(channel)).wrapping_mul(0x100000001b3);
                    }
                    samples += 1;
                }
            }
        }
        // Generated independently by tools/reference/XaeroSlopeReference.java.
        assert_eq!(samples, 1_030_410);
        assert_eq!(hash, 0xa50d47e4446be96a);
    }
}
