//! Deterministic terrain-height contours for sampled map rasters.
//!
//! Contours follow a triangle-interpolated height surface, so flat regions remain clear
//! instead of being painted with periodic color bands. The sample neighborhood is fixed,
//! and each pixel traces at most one minor level and its corresponding major style.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContourPrefs {
    pub enabled: bool,
    /// Height difference between neighboring contour lines, in terrain blocks.
    pub interval: u32,
    /// Draw every Nth contour more strongly. Zero disables major contours.
    pub major_every: u32,
    /// Maximum opacity of a fully covered contour line.
    pub opacity: f32,
    /// Line width in sampled raster pixels, excluding the antialias fringe.
    pub width: f32,
    /// RGB color composited over the selected map layer.
    pub color: [u8; 3],
    /// Whether contours may be drawn over water and the ocean floor.
    pub underwater: bool,
}

impl Default for ContourPrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: 10,
            major_every: 5,
            opacity: 0.55,
            width: 0.8,
            color: [50, 40, 28],
            underwater: false,
        }
    }
}

impl ContourPrefs {
    /// Ignore dormant contour style when choosing whether cached color tiles can be reused.
    pub fn same_raster(&self, other: &Self) -> bool {
        let active = self.is_active();
        let other_active = other.is_active();
        if !active && !other_active {
            return true;
        }
        active == other_active
            && self.interval == other.interval
            && self.major_every == other.major_every
            && self.opacity == other.opacity
            && self.width == other.width
            && self.color == other.color
            && self.underwater == other.underwater
    }

    pub fn is_active(&self) -> bool {
        self.enabled
            && self.interval > 0
            && self.opacity.is_finite()
            && self.opacity > 0.0
            && self.width.is_finite()
            && self.width > 0.0
    }

    fn effective_opacity(&self) -> f32 {
        if self.opacity.is_finite() {
            self.opacity.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn effective_width(&self) -> f64 {
        if self.width.is_finite() {
            f64::from(self.width.clamp(0.1, 4.0))
        } else {
            0.0
        }
    }
}

/// Blend one map pixel with contours inferred from a row-major 3×3 height neighborhood.
///
/// Heights are sampled one output pixel apart and ordered NW, N, NE, W, center, E, SW, S,
/// SE. Missing or non-finite values leave the pixel unchanged. The fixed local mesh and
/// nearest world-anchored contour level keep this bounded and seam-stable at every elevation.
pub fn shade_rgb(rgb: [u8; 3], heights: [Option<f64>; 9], prefs: &ContourPrefs) -> [u8; 3] {
    if !prefs.is_active() {
        return rgb;
    }
    let Some(heights) = finite_heights(heights) else {
        return rgb;
    };

    let interval = f64::from(prefs.interval);
    let variation = local_height_variation(&heights);
    let minor_index = (heights[4] / interval).round();
    let minor_level = minor_index * interval;
    if !minor_level.is_finite() {
        return rgb;
    }

    // Trace the nearest minor and major levels independently. A bold major contour may
    // be on the far side of a raster sample from its nearest minor line.
    let major_interval = interval * f64::from(prefs.major_every.max(1));
    let minor_is_major =
        prefs.major_every > 0 && minor_index.rem_euclid(f64::from(prefs.major_every)) == 0.0;
    let mut coverage = level_coverage(
        &heights,
        variation,
        minor_level,
        if minor_is_major {
            major_interval
        } else {
            interval
        },
        prefs.effective_width() * if minor_is_major { 1.75 } else { 1.0 },
        prefs,
    );
    if prefs.major_every > 0 {
        let major_index = (heights[4] / major_interval).round();
        let major_level = major_index * major_interval;
        if major_level.is_finite() && major_level != minor_level {
            coverage = coverage.max(level_coverage(
                &heights,
                variation,
                major_level,
                major_interval,
                prefs.effective_width() * 1.75,
                prefs,
            ));
        }
    }
    if coverage <= 0.0 {
        return rgb;
    }
    let mut output = rgb;
    for channel in 0..3 {
        output[channel] = (f32::from(rgb[channel]) * (1.0 - coverage)
            + f32::from(prefs.color[channel]) * coverage)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    output
}

fn level_coverage(
    heights: &[f64; 9],
    variation: f64,
    level: f64,
    spacing: f64,
    width: f64,
    prefs: &ContourPrefs,
) -> f32 {
    // Fade closely packed levels when terrain changes by more than their spacing per
    // sampled pixel. The fixed local mesh means even large height spans take constant work.
    let density = if variation > spacing {
        (spacing / variation).clamp(0.0, 1.0)
    } else {
        1.0
    };
    if density <= 0.0 {
        return 0.0;
    }

    let mut distance = f64::INFINITY;
    for cell in CELLS {
        for triangle in cell {
            if let Some(candidate) = triangle_distance(heights, triangle, level) {
                distance = distance.min(candidate);
            }
        }
    }
    if !distance.is_finite() {
        return 0.0;
    }

    // A half-pixel antialias ramp is added outside the configured stroke width.
    ((width * 0.5 + 0.5 - distance) as f32).clamp(0.0, 1.0)
        * prefs.effective_opacity()
        * density as f32
}

fn finite_heights(heights: [Option<f64>; 9]) -> Option<[f64; 9]> {
    let mut finite = [0.0; 9];
    for (index, height) in heights.into_iter().enumerate() {
        let height = height.filter(|value| value.is_finite())?;
        finite[index] = height;
    }
    Some(finite)
}

fn local_height_variation(heights: &[f64; 9]) -> f64 {
    const EDGES: [(usize, usize, f64); 20] = [
        (0, 1, 1.0),
        (1, 2, 1.0),
        (3, 4, 1.0),
        (4, 5, 1.0),
        (6, 7, 1.0),
        (7, 8, 1.0),
        (0, 3, 1.0),
        (3, 6, 1.0),
        (1, 4, 1.0),
        (4, 7, 1.0),
        (2, 5, 1.0),
        (5, 8, 1.0),
        (0, 4, std::f64::consts::SQRT_2),
        (1, 3, std::f64::consts::SQRT_2),
        (1, 5, std::f64::consts::SQRT_2),
        (2, 4, std::f64::consts::SQRT_2),
        (3, 7, std::f64::consts::SQRT_2),
        (4, 6, std::f64::consts::SQRT_2),
        (4, 8, std::f64::consts::SQRT_2),
        (5, 7, std::f64::consts::SQRT_2),
    ];
    EDGES
        .into_iter()
        .map(|(a, b, distance)| (heights[a] - heights[b]).abs() / distance)
        .fold(0.0, f64::max)
}

#[derive(Clone, Copy)]
struct Vertex {
    x: f64,
    y: f64,
    h: f64,
}

// Four grid cells surrounding the sampled pixel, each split along its NW-SE diagonal.
const CELLS: [[[usize; 3]; 2]; 4] = [
    [[0, 1, 4], [0, 4, 3]],
    [[1, 2, 5], [1, 5, 4]],
    [[3, 4, 7], [3, 7, 6]],
    [[4, 5, 8], [4, 8, 7]],
];

fn triangle_distance(heights: &[f64; 9], indices: [usize; 3], level: f64) -> Option<f64> {
    const OFFSETS: [[f64; 2]; 9] = [
        [-1.0, -1.0],
        [0.0, -1.0],
        [1.0, -1.0],
        [-1.0, 0.0],
        [0.0, 0.0],
        [1.0, 0.0],
        [-1.0, 1.0],
        [0.0, 1.0],
        [1.0, 1.0],
    ];
    let mut vertices = [Vertex {
        x: 0.0,
        y: 0.0,
        h: 0.0,
    }; 3];
    for (slot, index) in indices.into_iter().enumerate() {
        vertices[slot] = Vertex {
            x: OFFSETS[index][0],
            y: OFFSETS[index][1],
            h: heights[index],
        };
    }
    if vertices.iter().all(|vertex| vertex.h == vertices[0].h) {
        return None;
    }

    let mut intersections = [[0.0; 2]; 3];
    let mut count = 0;
    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
        let first = vertices[a];
        let second = vertices[b];
        let first_above = first.h >= level;
        let second_above = second.h >= level;
        if first_above == second_above {
            continue;
        }
        let span = second.h - first.h;
        if span == 0.0 {
            continue;
        }
        let t = ((level - first.h) / span).clamp(0.0, 1.0);
        let point = [
            first.x + t * (second.x - first.x),
            first.y + t * (second.y - first.y),
        ];
        if intersections[..count]
            .iter()
            .any(|old| (old[0] - point[0]).hypot(old[1] - point[1]) < 1e-9)
        {
            continue;
        }
        intersections[count] = point;
        count += 1;
    }
    if count < 2 {
        return None;
    }

    // Degenerate equal-height cases can report three crossings; keep the longest pair,
    // which is the stable outer edge of a plateau level set.
    let mut best = 0.0;
    let mut first = [0.0; 2];
    let mut second = [0.0; 2];
    for i in 0..count {
        for j in i + 1..count {
            let length = (intersections[i][0] - intersections[j][0])
                .hypot(intersections[i][1] - intersections[j][1]);
            if length > best {
                best = length;
                first = intersections[i];
                second = intersections[j];
            }
        }
    }
    if best <= 1e-9 {
        return None;
    }
    Some(point_segment_distance([0.0, 0.0], first, second))
}

fn point_segment_distance(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> f64 {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
        return (point[0] - start[0]).hypot(point[1] - start[1]);
    }
    let t = (((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / length_squared)
        .clamp(0.0, 1.0);
    (point[0] - (start[0] + t * dx)).hypot(point[1] - (start[1] + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active() -> ContourPrefs {
        ContourPrefs {
            enabled: true,
            ..Default::default()
        }
    }

    fn neighborhood(values: [f64; 9]) -> [Option<f64>; 9] {
        values.map(Some)
    }

    #[test]
    fn flat_terrain_stays_clear_even_when_it_matches_a_contour_level() {
        let prefs = active();
        let heights = neighborhood([80.; 9]);
        assert_eq!(shade_rgb([90, 120, 150], heights, &prefs), [90, 120, 150]);
    }

    #[test]
    fn negative_heights_use_world_anchored_levels() {
        let prefs = active();
        let heights = neighborhood([-11., -10., -9., -11., -10., -9., -11., -10., -9.]);
        assert_ne!(shade_rgb([220; 3], heights, &prefs), [220; 3]);
    }

    #[test]
    fn major_interval_changes_only_the_line_width() {
        let prefs = active();
        let heights = neighborhood([49., 50., 51., 49., 50., 51., 49., 50., 51.]);
        let major = shade_rgb([220; 3], heights, &prefs);
        let mut minor_prefs = prefs.clone();
        minor_prefs.major_every = 0;
        let minor = shade_rgb([220; 3], heights, &minor_prefs);
        assert!(major[0] < minor[0]);
    }

    #[test]
    fn bold_major_line_is_traced_even_when_nearest_minor_level_differs() {
        let prefs = active();
        let heights = neighborhood([36., 44., 52., 36., 44., 52., 36., 44., 52.]);
        let major_enabled = shade_rgb([220; 3], heights, &prefs);
        let mut minor_only = prefs.clone();
        minor_only.major_every = 0;
        let minor_result = shade_rgb([220; 3], heights, &minor_only);
        assert!(major_enabled[0] < minor_result[0]);

        let dense = neighborhood([-6., 44., 94., -6., 44., 94., -6., 44., 94.]);
        let dense_major = shade_rgb([220; 3], dense, &prefs);
        let dense_minor = shade_rgb([220; 3], dense, &minor_only);
        assert!(dense_major[0] + 12 < dense_minor[0]);
    }

    #[test]
    fn unavailable_or_non_finite_heights_leave_pixels_unchanged() {
        let prefs = active();
        let color = [180, 190, 200];
        let mut missing = neighborhood([49., 50., 51., 49., 50., 51., 49., 50., 51.]);
        missing[0] = None;
        assert_eq!(shade_rgb(color, missing, &prefs), color);
        let mut invalid = neighborhood([49., 50., 51., 49., 50., 51., 49., 50., 51.]);
        invalid[0] = Some(f64::NAN);
        assert_eq!(shade_rgb(color, invalid, &prefs), color);
    }

    #[test]
    fn steep_terrain_fades_line_density() {
        let prefs = active();
        let low = neighborhood([49., 50., 51., 49., 50., 51., 49., 50., 51.]);
        let steep = neighborhood([0., 20., 40., 0., 20., 40., 0., 20., 40.]);
        let low_result = shade_rgb([220; 3], low, &prefs);
        let steep_result = shade_rgb([220; 3], steep, &prefs);
        assert!(low_result[0] < steep_result[0]);
    }

    #[test]
    fn dormant_style_does_not_invalidate_raster_and_active_style_does() {
        let base = ContourPrefs::default();
        let mut dormant = base.clone();
        dormant.interval = 20;
        dormant.color = [255, 0, 0];
        assert!(base.same_raster(&dormant));
        dormant.enabled = true;
        assert!(!base.same_raster(&dormant));
    }

    #[test]
    fn invalid_contour_settings_are_inert() {
        let color = [100, 140, 180];
        let heights = neighborhood([49., 50., 51., 49., 50., 51., 49., 50., 51.]);
        let mut prefs = active();
        prefs.interval = 0;
        assert_eq!(shade_rgb(color, heights, &prefs), color);
        prefs.interval = 10;
        prefs.opacity = f32::NAN;
        assert_eq!(shade_rgb(color, heights, &prefs), color);
        prefs.opacity = 0.5;
        prefs.width = f32::NAN;
        assert_eq!(shade_rgb(color, heights, &prefs), color);
    }
}
