//! Original crisp pixel-art symbols for common TFC and TFG landmarks.
//! Each sprite is a transparent 32×32 image with no shared badge or backdrop.
use image::{Rgba, RgbaImage};

type C = [u8; 4];
const CLEAR: C = [0, 0, 0, 0];

pub(super) fn campfire() -> RgbaImage {
    let mut im = canvas();
    let stone = [119, 119, 105, 255];
    let light = [178, 171, 146, 255];
    let shade = [76, 78, 71, 255];
    let wood = [120, 71, 43, 255];
    let wood_light = [178, 111, 59, 255];
    // Scattered low firepit stones leave a broken, natural outline.
    for (x, y, w, h) in [
        (4, 19, 6, 4),
        (10, 23, 6, 4),
        (18, 23, 6, 4),
        (24, 19, 5, 4),
        (3, 14, 5, 4),
        (24, 13, 5, 4),
        (7, 11, 5, 3),
        (19, 10, 6, 4),
    ] {
        fill(&mut im, x, y, w, h, stone);
        put(&mut im, x + 1, y, light);
        put(&mut im, x + w - 1, y + h - 1, shade);
    }
    // Split logs laid across the shallow stone bed.
    line(&mut im, (8, 21), (23, 15), wood, 4);
    line(&mut im, (8, 20), (23, 14), wood_light, 1);
    line(&mut im, (9, 14), (23, 21), wood, 4);
    line(&mut im, (9, 13), (23, 20), wood_light, 1);
    // Chunky, irregular TFC flame with a yellow core and ember pixels.
    poly(
        &mut im,
        &[
            (11, 17),
            (10, 14),
            (12, 12),
            (13, 7),
            (16, 10),
            (18, 5),
            (21, 11),
            (23, 14),
            (21, 18),
        ],
        [229, 92, 34, 255],
    );
    poly(
        &mut im,
        &[
            (13, 17),
            (13, 14),
            (15, 12),
            (16, 9),
            (18, 12),
            (20, 11),
            (21, 15),
            (19, 18),
        ],
        [255, 172, 56, 255],
    );
    poly(
        &mut im,
        &[(15, 17), (15, 14), (17, 12), (19, 14), (19, 17)],
        [255, 225, 121, 255],
    );
    fill(&mut im, 7, 26, 5, 2, shade);
    fill(&mut im, 19, 26, 6, 2, shade);
    im
}

pub(super) fn village_head() -> RgbaImage {
    let mut im = canvas();
    let skin = [173, 139, 101, 255];
    let skin_light = [204, 168, 124, 255];
    let skin_shade = [118, 91, 68, 255];
    let hair = [83, 68, 48, 255];
    let moss = [76, 104, 57, 255];
    let moss_light = [115, 139, 72, 255];
    // Primitive square head, broken hairline, and mossy cap rather than a copied game face.
    poly(
        &mut im,
        &[
            (7, 9),
            (10, 6),
            (21, 6),
            (25, 10),
            (24, 23),
            (21, 27),
            (10, 26),
            (7, 22),
        ],
        skin,
    );
    fill(&mut im, 8, 10, 4, 7, skin_light);
    fill(&mut im, 20, 10, 4, 11, skin_shade);
    poly(
        &mut im,
        &[
            (7, 10),
            (9, 7),
            (11, 8),
            (14, 5),
            (17, 7),
            (20, 5),
            (23, 8),
            (26, 11),
            (23, 12),
            (21, 10),
            (18, 11),
            (15, 9),
            (12, 11),
            (9, 10),
        ],
        hair,
    );
    poly(
        &mut im,
        &[
            (8, 8),
            (11, 5),
            (14, 6),
            (17, 4),
            (19, 7),
            (23, 6),
            (25, 9),
            (22, 10),
            (19, 9),
            (16, 10),
            (12, 9),
        ],
        moss,
    );
    fill(&mut im, 11, 5, 4, 2, moss_light);
    fill(&mut im, 19, 6, 3, 2, moss_light);
    // Heavy brows, inset eyes and broad nose form a readable primitive face at 32px.
    fill(&mut im, 10, 13, 5, 2, hair);
    fill(&mut im, 18, 13, 5, 2, hair);
    fill(&mut im, 11, 15, 2, 2, [224, 216, 188, 255]);
    fill(&mut im, 19, 15, 2, 2, [224, 216, 188, 255]);
    put(&mut im, 12, 15, [44, 45, 37, 255]);
    put(&mut im, 20, 15, [44, 45, 37, 255]);
    fill(&mut im, 15, 15, 3, 6, skin_light);
    fill(&mut im, 14, 20, 6, 3, skin);
    fill(&mut im, 11, 24, 10, 2, skin_shade);
    fill(&mut im, 10, 23, 12, 1, hair);
    im
}

pub(super) fn cracked_limestone_brick() -> RgbaImage {
    let mut im = canvas();
    let face = [173, 166, 143, 255];
    let light = [217, 207, 178, 255];
    let shade = [112, 111, 101, 255];
    let crack = [82, 83, 76, 255];
    // Chunky, slightly irregular masonry slab with a chipped upper corner.
    poly(
        &mut im,
        &[
            (4, 9),
            (9, 5),
            (27, 6),
            (29, 10),
            (28, 25),
            (23, 28),
            (5, 27),
            (3, 23),
        ],
        face,
    );
    line(&mut im, (5, 9), (26, 9), light, 2);
    line(&mut im, (5, 10), (5, 24), light, 1);
    line(&mut im, (5, 25), (24, 25), shade, 2);
    line(&mut im, (27, 11), (26, 23), shade, 1);
    // Shallow mortar seams and a jagged split, with tiny missing chips.
    line(&mut im, (6, 17), (27, 17), [139, 135, 120, 255], 1);
    line(&mut im, (14, 10), (14, 16), [139, 135, 120, 255], 1);
    line(&mut im, (20, 18), (20, 24), [139, 135, 120, 255], 1);
    line(&mut im, (18, 10), (17, 14), crack, 1);
    line(&mut im, (17, 14), (20, 16), crack, 1);
    line(&mut im, (20, 16), (18, 20), crack, 1);
    line(&mut im, (18, 20), (21, 23), crack, 1);
    put(&mut im, 10, 12, light);
    put(&mut im, 25, 20, light);
    put(&mut im, 6, 8, CLEAR);
    put(&mut im, 28, 8, CLEAR);
    put(&mut im, 4, 25, CLEAR);
    im
}

pub(super) fn church_cross() -> RgbaImage {
    let mut im = canvas();
    let wood = [117, 78, 48, 255];
    let light = [174, 127, 78, 255];
    let shade = [76, 57, 41, 255];
    // A tall weathered beam and short cross-piece; split edges avoid a UI-glyph look.
    poly(
        &mut im,
        &[
            (14, 4),
            (18, 4),
            (18, 13),
            (25, 13),
            (25, 17),
            (18, 17),
            (18, 28),
            (13, 28),
            (13, 17),
            (7, 17),
            (7, 13),
            (13, 13),
            (13, 5),
        ],
        wood,
    );
    line(&mut im, (14, 5), (14, 26), light, 1);
    line(&mut im, (8, 14), (23, 14), light, 1);
    line(&mut im, (17, 7), (17, 26), shade, 1);
    // Faded grain and a broken foot.
    line(&mut im, (14, 20), (17, 22), shade, 1);
    fill(&mut im, 12, 28, 8, 2, shade);
    put(&mut im, 9, 16, [200, 153, 93, 255]);
    im
}

pub(super) fn mineshaft_arch() -> RgbaImage {
    let mut im = canvas();
    let wood = [126, 82, 46, 255];
    let wood_light = [184, 128, 73, 255];
    let wood_dark = [75, 53, 38, 255];
    let brace = [150, 100, 55, 255];
    // Squared timber portal. Its central opening remains transparent.
    fill(&mut im, 5, 8, 5, 21, wood);
    fill(&mut im, 22, 8, 5, 21, wood);
    fill(&mut im, 8, 5, 16, 5, wood);
    fill(&mut im, 11, 3, 10, 3, wood);
    fill(&mut im, 5, 8, 4, 3, wood_light);
    fill(&mut im, 23, 8, 4, 3, wood_light);
    fill(&mut im, 9, 5, 14, 2, wood_light);
    // Dark inner edges define the tunnel without painting a solid hole/background.
    fill(&mut im, 9, 11, 2, 16, wood_dark);
    fill(&mut im, 21, 11, 2, 16, wood_dark);
    fill(&mut im, 10, 8, 3, 2, wood_dark);
    fill(&mut im, 19, 8, 3, 2, wood_dark);
    // Short braces reinforce the lintel's corners without crossing the tunnel.
    line(&mut im, (10, 13), (13, 10), brace, 1);
    line(&mut im, (21, 13), (18, 10), brace, 1);
    // Narrow-gauge rails recede toward the opening, with two short sleepers.
    line(&mut im, (11, 29), (14, 23), wood_light, 1);
    line(&mut im, (20, 29), (18, 23), wood_light, 1);
    line(&mut im, (12, 28), (19, 28), wood, 1);
    line(&mut im, (13, 25), (18, 25), wood, 1);
    fill(&mut im, 3, 28, 9, 2, wood_dark);
    fill(&mut im, 20, 28, 9, 2, wood_dark);
    // Nail heads and timber grain.
    put(&mut im, 7, 12, [214, 184, 125, 255]);
    put(&mut im, 24, 12, [214, 184, 125, 255]);
    line(&mut im, (7, 16), (7, 24), [148, 101, 61, 255], 1);
    line(&mut im, (24, 16), (24, 24), [148, 101, 61, 255], 1);
    im
}

fn canvas() -> RgbaImage {
    RgbaImage::from_pixel(32, 32, Rgba(CLEAR))
}
fn put(im: &mut RgbaImage, x: i32, y: i32, c: C) {
    if (0..32).contains(&x) && (0..32).contains(&y) {
        im.put_pixel(x as u32, y as u32, Rgba(c));
    }
}
fn fill(im: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: C) {
    for py in y..y + h {
        for px in x..x + w {
            put(im, px, py, c);
        }
    }
}
fn line(im: &mut RgbaImage, (mut x0, mut y0): (i32, i32), (x1, y1): (i32, i32), c: C, width: i32) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        for oy in -width..=width {
            for ox in -width..=width {
                if ox * ox + oy * oy <= width * width + 1 {
                    put(im, x0 + ox, y0 + oy, c);
                }
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e = 2 * err;
        if e >= dy {
            err += dy;
            x0 += sx;
        }
        if e <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
fn poly(im: &mut RgbaImage, points: &[(i32, i32)], c: C) {
    let min_y = points.iter().map(|p| p.1).min().unwrap_or(0);
    let max_y = points.iter().map(|p| p.1).max().unwrap_or(-1);
    for y in min_y..=max_y {
        let mut xs = Vec::new();
        for i in 0..points.len() {
            let (x0, y0) = points[i];
            let (x1, y1) = points[(i + 1) % points.len()];
            if (y0 <= y && y < y1) || (y1 <= y && y < y0) {
                xs.push(x0 + (y - y0) * (x1 - x0) / (y1 - y0));
            }
        }
        xs.sort_unstable();
        for pair in xs.chunks_exact(2) {
            for x in pair[0]..=pair[1] {
                put(im, x, y, c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn landmark_sprites_are_transparent_crisp_32px_art() {
        for im in [
            campfire(),
            village_head(),
            cracked_limestone_brick(),
            church_cross(),
            mineshaft_arch(),
        ] {
            assert_eq!(im.dimensions(), (32, 32));
            assert!(im.pixels().any(|p| p[3] == 0));
            assert!(im.pixels().any(|p| p[3] == 255));
            assert!(im.pixels().any(|p| p[3] != 0));
        }
        let mine = mineshaft_arch();
        assert_eq!(
            mine.get_pixel(16, 16)[3],
            0,
            "tunnel opening remains transparent"
        );
        assert_eq!(mine.get_pixel(16, 21)[3], 0, "center remains unobstructed");
    }
}
