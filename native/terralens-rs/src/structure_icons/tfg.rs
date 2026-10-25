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

pub(super) fn volcano_cinder_cone() -> RgbaImage {
    let mut im = canvas();
    let basalt_shadow = [35, 42, 43, 255];
    let basalt = [65, 72, 70, 255];
    let basalt_light = [108, 106, 93, 255];
    let ash = [143, 130, 108, 255];
    let ember_dark = [206, 65, 34, 255];
    let lava = [246, 115, 38, 255];
    let hot = [255, 200, 72, 255];
    let smoke = [113, 121, 117, 255];
    let smoke_light = [169, 171, 158, 255];

    // Angular cone with a broad, chipped foot and a clearly open summit.
    poly(
        &mut im,
        &[
            (3, 27),
            (7, 25),
            (10, 19),
            (13, 14),
            (15, 10),
            (18, 10),
            (20, 15),
            (23, 20),
            (26, 25),
            (29, 27),
            (27, 30),
            (6, 30),
        ],
        basalt_shadow,
    );
    poly(
        &mut im,
        &[
            (5, 27),
            (9, 24),
            (12, 18),
            (15, 12),
            (18, 12),
            (21, 18),
            (24, 24),
            (27, 27),
            (25, 29),
            (8, 29),
        ],
        basalt,
    );
    poly(
        &mut im,
        &[
            (8, 25),
            (11, 22),
            (13, 17),
            (16, 13),
            (17, 14),
            (15, 19),
            (12, 25),
            (9, 27),
        ],
        basalt_light,
    );
    // Basalt blocks and sparse ash chips keep the slope legible at map scale.
    line(&mut im, (7, 27), (12, 27), basalt_shadow, 1);
    line(&mut im, (19, 25), (25, 27), basalt_shadow, 1);
    put(&mut im, 10, 23, ash);
    put(&mut im, 14, 17, basalt_light);
    put(&mut im, 20, 21, ash);
    put(&mut im, 23, 24, basalt_light);
    // Molten light is confined to the crater, so the icon reads as a volcanic vent.
    poly(
        &mut im,
        &[(13, 12), (15, 9), (18, 9), (21, 12), (19, 14), (15, 14)],
        ember_dark,
    );
    poly(
        &mut im,
        &[(14, 12), (16, 10), (18, 10), (20, 12), (18, 13), (15, 13)],
        lava,
    );
    line(&mut im, (16, 11), (18, 11), hot, 0);

    // Two small rising puffs sit above the crater and remain separate from the cone.
    fill(&mut im, 15, 5, 2, 2, smoke);
    put(&mut im, 14, 5, smoke_light);
    fill(&mut im, 17, 2, 2, 2, smoke);
    put(&mut im, 16, 3, smoke_light);
    put(&mut im, 18, 1, [124, 130, 125, 205]);
    im
}

pub(super) fn volcano_dormant_cone() -> RgbaImage {
    let mut im = canvas();
    let shadow = [49, 56, 55, 255];
    let basalt = [84, 90, 84, 255];
    let face = [120, 119, 103, 255];
    let light = [162, 154, 128, 255];
    let muted_crater = [46, 50, 48, 255];
    let soil = [99, 103, 72, 255];
    let grass = [124, 139, 77, 255];

    poly(
        &mut im,
        &[
            (3, 28),
            (6, 25),
            (9, 21),
            (12, 16),
            (15, 11),
            (18, 11),
            (21, 16),
            (24, 21),
            (27, 25),
            (29, 28),
            (27, 30),
            (5, 30),
        ],
        shadow,
    );
    poly(
        &mut im,
        &[
            (5, 27),
            (9, 23),
            (12, 18),
            (15, 13),
            (18, 13),
            (21, 18),
            (24, 23),
            (27, 27),
            (25, 29),
            (8, 29),
        ],
        basalt,
    );
    poly(
        &mut im,
        &[
            (7, 26),
            (11, 22),
            (13, 18),
            (16, 14),
            (16, 19),
            (13, 24),
            (10, 27),
        ],
        face,
    );
    line(&mut im, (8, 27), (12, 27), light, 0);
    line(&mut im, (20, 25), (25, 27), shadow, 0);
    // The gray crater has no fire or smoke; sparse grass only reaches its lower skirt.
    poly(
        &mut im,
        &[(13, 13), (15, 10), (18, 10), (21, 13), (19, 15), (15, 15)],
        face,
    );
    poly(
        &mut im,
        &[(15, 13), (16, 12), (18, 12), (19, 13), (18, 14), (16, 14)],
        muted_crater,
    );
    put(&mut im, 7, 28, soil);
    put(&mut im, 8, 27, grass);
    put(&mut im, 24, 28, soil);
    put(&mut im, 25, 27, grass);
    im
}

pub(super) fn volcano_tuya() -> RgbaImage {
    let mut im = canvas();
    let shadow = [42, 54, 57, 255];
    let basalt = [72, 85, 83, 255];
    let face = [106, 112, 101, 255];
    let light_face = [146, 144, 124, 255];
    let plateau = [80, 93, 84, 255];
    let plateau_light = [125, 142, 116, 255];

    // A tuya's flattened volcanic plateau sits on steep, blocky subglacial sides.
    poly(
        &mut im,
        &[
            (4, 28),
            (7, 25),
            (10, 20),
            (11, 14),
            (13, 12),
            (20, 12),
            (22, 14),
            (23, 20),
            (26, 25),
            (29, 28),
            (27, 30),
            (6, 30),
        ],
        shadow,
    );
    poly(
        &mut im,
        &[
            (6, 27),
            (9, 24),
            (12, 19),
            (13, 14),
            (20, 14),
            (21, 19),
            (24, 24),
            (27, 27),
            (25, 29),
            (8, 29),
        ],
        basalt,
    );
    poly(
        &mut im,
        &[
            (7, 26),
            (11, 22),
            (13, 15),
            (15, 15),
            (14, 21),
            (12, 26),
            (9, 28),
        ],
        face,
    );
    line(&mut im, (8, 27), (12, 27), light_face, 0);
    line(&mut im, (20, 22), (25, 27), shadow, 0);
    // The crater lip is level and broken; the middle stays dark stone without lava.
    poly(
        &mut im,
        &[
            (10, 12),
            (12, 8),
            (15, 7),
            (18, 7),
            (21, 8),
            (24, 12),
            (22, 15),
            (12, 15),
        ],
        shadow,
    );
    poly(
        &mut im,
        &[
            (11, 11),
            (13, 9),
            (15, 9),
            (16, 11),
            (18, 9),
            (21, 10),
            (23, 12),
            (21, 14),
            (13, 14),
        ],
        plateau,
    );
    put(&mut im, 13, 10, plateau_light);
    put(&mut im, 20, 11, plateau_light);
    line(&mut im, (14, 13), (19, 13), shadow, 0);
    put(&mut im, 10, 17, face);
    put(&mut im, 22, 17, face);
    im
}

pub(super) fn volcano_tuff_ring() -> RgbaImage {
    let mut im = canvas();
    let shadow = [55, 61, 58, 255];
    let stone = [108, 111, 101, 255];
    let light = [157, 153, 132, 255];
    let ash = [128, 121, 104, 255];
    let inside = [75, 80, 72, 255];

    // Low circular tuff rim, shown in broken segments around a visible crater.
    poly(
        &mut im,
        &[
            (4, 18),
            (7, 11),
            (13, 7),
            (20, 7),
            (26, 12),
            (29, 19),
            (26, 25),
            (20, 29),
            (12, 29),
            (6, 25),
            (3, 20),
        ],
        shadow,
    );
    poly(
        &mut im,
        &[
            (6, 18),
            (9, 12),
            (14, 9),
            (20, 9),
            (25, 13),
            (27, 19),
            (24, 24),
            (19, 27),
            (13, 27),
            (8, 24),
            (6, 20),
        ],
        stone,
    );
    // Remove the center to preserve the open rim silhouette at all render sizes.
    for y in 14..23 {
        for x in 11..23 {
            let dx = x - 17;
            let dy = y - 18;
            if dx * dx + dy * dy <= 24 {
                put(&mut im, x, y, CLEAR);
            }
        }
    }
    // Rim highlights, ash-colored ejecta, and a gap at the near edge.
    line(&mut im, (11, 11), (16, 9), light, 0);
    line(&mut im, (20, 10), (24, 14), light, 0);
    line(&mut im, (26, 19), (23, 23), ash, 0);
    line(&mut im, (13, 26), (10, 23), ash, 0);
    put(&mut im, 5, 18, CLEAR);
    put(&mut im, 6, 20, CLEAR);
    put(&mut im, 25, 25, inside);
    put(&mut im, 8, 13, inside);
    im
}

pub(super) fn volcano_ice_cone() -> RgbaImage {
    let mut im = canvas();
    let shadow = [43, 58, 63, 255];
    let basalt = [78, 99, 102, 255];
    let blue_ice = [110, 165, 179, 255];
    let ice = [175, 212, 213, 255];
    let snow = [227, 239, 229, 255];
    let crater = [62, 83, 91, 255];

    poly(
        &mut im,
        &[
            (3, 28),
            (7, 25),
            (10, 20),
            (13, 15),
            (15, 10),
            (18, 10),
            (21, 15),
            (24, 20),
            (27, 25),
            (29, 28),
            (27, 30),
            (5, 30),
        ],
        shadow,
    );
    poly(
        &mut im,
        &[
            (5, 27),
            (9, 24),
            (12, 19),
            (15, 12),
            (18, 12),
            (21, 19),
            (24, 24),
            (27, 27),
            (25, 29),
            (8, 29),
        ],
        basalt,
    );
    // Glaciated tongues follow the flanks, with a snow cap around the crater.
    poly(
        &mut im,
        &[
            (10, 22),
            (13, 17),
            (16, 12),
            (16, 18),
            (14, 23),
            (12, 27),
            (9, 28),
        ],
        blue_ice,
    );
    poly(
        &mut im,
        &[(20, 21), (22, 21), (25, 26), (26, 28), (23, 27)],
        ice,
    );
    poly(
        &mut im,
        &[(13, 13), (15, 9), (18, 9), (21, 13), (18, 15), (15, 14)],
        snow,
    );
    poly(
        &mut im,
        &[(15, 12), (16, 11), (18, 11), (19, 12), (18, 13), (16, 13)],
        crater,
    );
    put(&mut im, 15, 8, snow);
    put(&mut im, 20, 15, ice);
    put(&mut im, 11, 25, ice);
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

/// Ruined building with a visible recovered chest, retaining the ruin's visual topic.
pub(super) fn ruined_building_with_chest(topic: super::RuinTopic) -> RgbaImage {
    let mut im = match topic {
        super::RuinTopic::Masonry => cracked_limestone_brick(),
        super::RuinTopic::Church => ruined_church(),
        super::RuinTopic::Tower => ruined_tower(),
        super::RuinTopic::Monument => ancient_monument_ruin(),
    };
    let outline = [54, 47, 39, 255];
    let wood = [111, 69, 39, 255];
    let wood_light = [163, 105, 55, 255];
    let wood_shadow = [77, 53, 37, 255];
    let iron = [170, 166, 148, 255];
    let iron_light = [215, 207, 177, 255];
    let lock = [225, 182, 76, 255];
    // An open, irregular chest silhouette sits in front of the stonework.
    poly(
        &mut im,
        &[(17, 20), (19, 17), (27, 17), (30, 20), (28, 23), (19, 23)],
        outline,
    );
    poly(
        &mut im,
        &[(19, 20), (20, 18), (26, 18), (28, 20), (27, 22), (20, 22)],
        wood_light,
    );
    line(&mut im, (20, 20), (27, 20), iron, 1);
    // Low plank body, with a narrow iron strap and a warm latch highlight.
    poly(
        &mut im,
        &[(18, 21), (28, 21), (28, 27), (26, 29), (19, 28), (17, 25)],
        outline,
    );
    poly(
        &mut im,
        &[(19, 22), (27, 22), (27, 26), (25, 27), (20, 26), (19, 25)],
        wood,
    );
    line(&mut im, (21, 22), (21, 26), wood_light, 1);
    line(&mut im, (25, 22), (25, 26), wood_shadow, 1);
    fill(&mut im, 18, 24, 10, 2, iron);
    put(&mut im, 18, 24, iron_light);
    put(&mut im, 23, 24, lock);
    put(&mut im, 23, 25, [117, 83, 42, 255]);
    put(&mut im, 27, 25, iron_light);
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

pub(super) fn moon_dome() -> RgbaImage {
    let mut im = canvas();
    let outline = [48, 55, 61, 255];
    let shell = [105, 113, 119, 255];
    let shell_light = [151, 159, 163, 255];
    let shell_shadow = [75, 84, 91, 255];
    let seam = [57, 67, 73, 255];
    let glass = [92, 161, 169, 255];
    let glass_light = [151, 204, 205, 255];
    // Low modular habitat: broad foundation under a faceted pressure dome.
    poly(
        &mut im,
        &[
            (5, 18),
            (6, 12),
            (9, 8),
            (13, 5),
            (18, 4),
            (22, 6),
            (26, 10),
            (28, 17),
            (27, 22),
            (23, 25),
            (9, 25),
            (5, 22),
        ],
        outline,
    );
    poly(
        &mut im,
        &[
            (7, 18),
            (8, 13),
            (11, 9),
            (15, 7),
            (18, 7),
            (22, 9),
            (25, 13),
            (26, 18),
            (24, 21),
            (10, 21),
        ],
        shell,
    );
    // Alternating roof panels read as joined metal plates, not a glass bubble.
    poly(
        &mut im,
        &[(9, 13), (12, 9), (15, 8), (15, 20), (9, 20)],
        shell_light,
    );
    poly(
        &mut im,
        &[(17, 8), (20, 9), (24, 13), (25, 19), (17, 20)],
        shell_shadow,
    );
    line(&mut im, (16, 7), (16, 21), seam, 1);
    line(&mut im, (10, 13), (24, 13), seam, 1);
    line(&mut im, (8, 18), (26, 18), outline, 1);
    // Small muted-cyan portholes stay subordinate to the silhouette.
    fill(&mut im, 11, 14, 3, 3, glass);
    put(&mut im, 11, 14, glass_light);
    fill(&mut im, 19, 14, 3, 3, glass);
    put(&mut im, 19, 14, glass_light);
    // Separate lower foundation and a compact side airlock with a dark hatch.
    fill(&mut im, 7, 22, 19, 3, [65, 73, 79, 255]);
    line(&mut im, (9, 25), (24, 25), [179, 182, 176, 255], 1);
    fill(&mut im, 25, 18, 5, 7, outline);
    fill(&mut im, 26, 19, 4, 5, shell);
    fill(&mut im, 27, 20, 2, 3, [55, 65, 71, 255]);
    put(&mut im, 28, 21, glass);
    fill(&mut im, 9, 26, 3, 2, shell_shadow);
    fill(&mut im, 21, 26, 3, 2, shell_shadow);
    im
}

pub(super) fn moon_rabbit_house() -> RgbaImage {
    let mut im = canvas();
    let dark = [48, 51, 53, 255];
    let wood = [119, 104, 85, 255];
    let wood_light = [162, 146, 119, 255];
    let wood_shadow = [81, 77, 70, 255];
    let straw = [191, 171, 112, 255];
    let straw_light = [222, 204, 145, 255];
    let fur = [218, 220, 216, 255];
    let fur_light = [241, 241, 232, 255];
    let fur_shadow = [143, 151, 151, 255];
    let pink = [190, 112, 126, 255];
    let pink_light = [226, 156, 164, 255];
    // Tall ears set the silhouette first; small inset pixels keep them legible.
    poly(
        &mut im,
        &[
            (10, 15),
            (9, 8),
            (10, 3),
            (13, 2),
            (16, 9),
            (18, 8),
            (20, 2),
            (23, 3),
            (24, 9),
            (22, 16),
        ],
        dark,
    );
    poly(
        &mut im,
        &[(11, 12), (11, 6), (12, 4), (14, 10), (15, 14)],
        pink,
    );
    poly(
        &mut im,
        &[(19, 13), (21, 5), (22, 5), (22, 10), (21, 14)],
        pink_light,
    );
    // Boxy nesting shelter with a shaded opening and a broad straw lip.
    poly(
        &mut im,
        &[
            (4, 16),
            (8, 13),
            (24, 13),
            (28, 16),
            (28, 27),
            (25, 30),
            (7, 30),
            (4, 27),
        ],
        dark,
    );
    poly(
        &mut im,
        &[
            (6, 17),
            (9, 15),
            (23, 15),
            (26, 17),
            (26, 26),
            (24, 28),
            (8, 28),
            (6, 26),
        ],
        wood,
    );
    fill(&mut im, 8, 17, 4, 2, wood_light);
    fill(&mut im, 21, 17, 4, 2, wood_shadow);
    line(&mut im, (7, 22), (25, 22), wood_shadow, 1);
    line(&mut im, (9, 25), (23, 25), wood_light, 1);
    // Straw spills across the front edge around the rabbit's little nest.
    line(&mut im, (5, 21), (10, 19), straw, 2);
    line(&mut im, (8, 22), (14, 19), straw_light, 1);
    line(&mut im, (19, 20), (26, 22), straw, 2);
    line(&mut im, (17, 22), (24, 24), straw_light, 1);
    // The head and cheeks overlap the box opening so the animal is unmistakable.
    poly(
        &mut im,
        &[
            (10, 14),
            (13, 12),
            (19, 12),
            (23, 15),
            (24, 20),
            (21, 24),
            (14, 24),
            (10, 21),
            (9, 17),
        ],
        fur_shadow,
    );
    poly(
        &mut im,
        &[
            (11, 15),
            (14, 13),
            (19, 13),
            (22, 16),
            (22, 20),
            (19, 22),
            (14, 22),
            (11, 20),
        ],
        fur,
    );
    fill(&mut im, 12, 15, 4, 3, fur_light);
    put(&mut im, 14, 18, dark);
    put(&mut im, 20, 18, dark);
    put(&mut im, 17, 20, [104, 104, 102, 255]);
    put(&mut im, 16, 21, pink);
    // A few straw pixels frame the lower silhouette without adding a badge.
    put(&mut im, 6, 25, straw_light);
    put(&mut im, 25, 26, straw);
    im
}

pub(super) fn cheese_wedge() -> RgbaImage {
    let mut im = canvas();
    let rind = [191, 123, 37, 255];
    let rind_light = [235, 176, 65, 255];
    let cheese = [255, 215, 87, 255];
    let cheese_light = [255, 235, 132, 255];
    let cheese_shadow = [226, 159, 48, 255];
    let hole = [180, 119, 42, 255];
    // A chunky triangular wedge, with the rind forming its darker lower crust.
    poly(
        &mut im,
        &[
            (4, 24),
            (7, 20),
            (10, 16),
            (14, 12),
            (19, 8),
            (25, 5),
            (27, 9),
            (27, 23),
            (23, 27),
            (8, 27),
        ],
        rind,
    );
    poly(
        &mut im,
        &[
            (6, 22),
            (10, 17),
            (14, 13),
            (19, 9),
            (24, 7),
            (25, 10),
            (25, 21),
            (22, 24),
            (9, 24),
        ],
        cheese,
    );
    line(&mut im, (8, 21), (23, 9), cheese_light, 2);
    line(&mut im, (9, 25), (23, 25), rind_light, 1);
    fill(&mut im, 25, 12, 2, 8, cheese_shadow);
    // Uneven round holes are dark, inset, and varied in size for a readable cheese face.
    fill(&mut im, 13, 15, 3, 3, hole);
    put(&mut im, 13, 16, cheese_shadow);
    fill(&mut im, 19, 12, 2, 2, hole);
    put(&mut im, 19, 12, cheese_shadow);
    fill(&mut im, 18, 19, 4, 3, hole);
    put(&mut im, 18, 20, cheese_shadow);
    fill(&mut im, 10, 21, 2, 2, hole);
    put(&mut im, 10, 21, cheese_shadow);
    im
}

pub(super) fn ruined_church() -> RgbaImage {
    let mut im = canvas();
    let stone = [135, 131, 117, 255];
    let stone_light = [190, 180, 153, 255];
    let stone_shade = [87, 88, 79, 255];
    let mortar = [112, 110, 99, 255];
    let moss = [75, 111, 64, 255];
    let moss_light = [119, 145, 76, 255];
    let dark = [49, 54, 49, 255];

    // Broken chapel front: the chipped roofline rises toward a small surviving cross.
    poly(
        &mut im,
        &[
            (3, 13),
            (7, 11),
            (10, 7),
            (13, 10),
            (17, 4),
            (20, 8),
            (23, 7),
            (26, 12),
            (29, 13),
            (29, 28),
            (26, 30),
            (5, 30),
            (3, 27),
        ],
        stone,
    );
    poly(
        &mut im,
        &[
            (4, 14),
            (8, 12),
            (11, 9),
            (13, 12),
            (17, 6),
            (19, 10),
            (23, 9),
            (27, 14),
            (27, 27),
            (24, 28),
            (6, 28),
        ],
        stone_light,
    );
    line(&mut im, (5, 15), (26, 15), stone_shade, 1);
    // Outer buttresses are deliberately separate from the arched opening.
    poly(
        &mut im,
        &[(4, 17), (10, 17), (10, 27), (8, 30), (4, 29)],
        stone,
    );
    poly(
        &mut im,
        &[(22, 17), (28, 17), (28, 29), (24, 30), (22, 27)],
        stone,
    );
    fill(&mut im, 5, 18, 2, 9, stone_light);
    fill(&mut im, 25, 19, 2, 8, stone_shade);
    // Recessed pointed arch: empty pixels show the map through the ruin.
    poly(
        &mut im,
        &[
            (11, 28),
            (11, 19),
            (12, 15),
            (15, 12),
            (17, 11),
            (20, 14),
            (21, 18),
            (21, 28),
        ],
        dark,
    );
    poly(
        &mut im,
        &[
            (13, 28),
            (13, 19),
            (14, 16),
            (16, 14),
            (18, 15),
            (19, 18),
            (19, 28),
        ],
        CLEAR,
    );
    // Split mortar courses, missing stones and trailing moss break up the regular wall.
    line(&mut im, (5, 21), (10, 21), mortar, 1);
    line(&mut im, (22, 22), (27, 22), mortar, 1);
    line(&mut im, (8, 15), (9, 19), stone_shade, 1);
    line(&mut im, (24, 15), (23, 19), stone_shade, 1);
    put(&mut im, 7, 13, CLEAR);
    put(&mut im, 26, 11, CLEAR);
    put(&mut im, 3, 16, CLEAR);
    fill(&mut im, 5, 25, 3, 2, moss);
    put(&mut im, 7, 24, moss_light);
    fill(&mut im, 24, 26, 3, 2, moss);
    put(&mut im, 25, 25, moss_light);
    // Weathered cross fragment above the gable.
    fill(&mut im, 16, 1, 2, 6, stone_shade);
    fill(&mut im, 13, 3, 8, 2, stone_shade);
    put(&mut im, 16, 1, stone_light);
    im
}

pub(super) fn ruined_tower() -> RgbaImage {
    let mut im = canvas();
    let stone = [133, 133, 125, 255];
    let light = [192, 185, 162, 255];
    let shade = [78, 83, 79, 255];
    let mortar = [105, 108, 101, 255];
    let moss = [69, 105, 62, 255];
    let moss_light = [118, 143, 75, 255];
    let dark = [42, 48, 46, 255];

    // Narrow medieval keep with a missing upper corner and an irregular crown of masonry.
    poly(
        &mut im,
        &[
            (7, 9),
            (11, 7),
            (13, 9),
            (16, 5),
            (19, 8),
            (22, 6),
            (25, 10),
            (27, 28),
            (24, 31),
            (7, 30),
            (5, 27),
        ],
        shade,
    );
    poly(
        &mut im,
        &[
            (8, 11),
            (12, 9),
            (14, 11),
            (17, 7),
            (19, 10),
            (23, 8),
            (24, 11),
            (25, 27),
            (22, 29),
            (8, 28),
            (7, 25),
        ],
        stone,
    );
    // Highlighted broken blocks make the silhouette read as masonry rather than a rectangle.
    fill(&mut im, 9, 12, 4, 2, light);
    fill(&mut im, 16, 10, 4, 2, light);
    fill(&mut im, 20, 12, 3, 2, light);
    fill(&mut im, 8, 17, 3, 6, light);
    fill(&mut im, 22, 19, 2, 5, shade);
    line(&mut im, (9, 16), (23, 16), mortar, 1);
    line(&mut im, (13, 13), (13, 16), mortar, 1);
    line(&mut im, (18, 17), (18, 21), mortar, 1);
    line(&mut im, (11, 23), (22, 23), mortar, 1);
    // Deep open arrow slit / window leaves transparent holes, not a painted dark plaque.
    fill(&mut im, 14, 14, 5, 11, dark);
    fill(&mut im, 15, 13, 3, 2, dark);
    fill(&mut im, 16, 12, 1, 1, CLEAR);
    fill(&mut im, 15, 15, 3, 10, CLEAR);
    // A crack reaches the base and moss threads into a few missing stones.
    line(&mut im, (11, 18), (13, 21), shade, 1);
    line(&mut im, (13, 21), (11, 25), shade, 1);
    put(&mut im, 6, 10, CLEAR);
    put(&mut im, 26, 8, CLEAR);
    put(&mut im, 25, 12, CLEAR);
    fill(&mut im, 8, 26, 4, 2, moss);
    fill(&mut im, 19, 27, 4, 2, moss);
    put(&mut im, 10, 25, moss_light);
    put(&mut im, 21, 26, moss_light);
    im
}

pub(super) fn ancient_monument_ruin() -> RgbaImage {
    let mut im = canvas();
    let stone = [139, 133, 111, 255];
    let light = [199, 184, 145, 255];
    let shade = [84, 85, 72, 255];
    let seam = [109, 107, 87, 255];
    let moss = [73, 110, 63, 255];
    let moss_light = [120, 147, 75, 255];

    // Fractured plinths sit apart as an archaeological ruin, not a complete building face.
    poly(
        &mut im,
        &[(3, 26), (6, 23), (12, 24), (15, 27), (14, 30), (4, 30)],
        shade,
    );
    poly(
        &mut im,
        &[(4, 25), (7, 23), (12, 24), (13, 27), (5, 28)],
        stone,
    );
    poly(
        &mut im,
        &[(18, 27), (21, 23), (28, 24), (30, 27), (28, 30), (19, 30)],
        shade,
    );
    poly(
        &mut im,
        &[(19, 26), (22, 24), (27, 25), (28, 27), (21, 28)],
        stone,
    );
    line(&mut im, (7, 25), (12, 26), light, 1);
    line(&mut im, (22, 26), (27, 26), light, 1);

    // One tall standing stone, a broken center pillar and a shorter leaning marker.
    poly(
        &mut im,
        &[
            (5, 23),
            (6, 8),
            (9, 5),
            (12, 7),
            (13, 23),
            (11, 25),
            (7, 24),
        ],
        shade,
    );
    poly(
        &mut im,
        &[(7, 22), (7, 9), (9, 7), (10, 9), (11, 22), (9, 24)],
        stone,
    );
    line(&mut im, (8, 10), (8, 20), light, 1);
    line(&mut im, (10, 12), (9, 17), seam, 1);

    poly(
        &mut im,
        &[
            (14, 26),
            (15, 13),
            (18, 10),
            (21, 12),
            (22, 27),
            (19, 29),
            (15, 28),
        ],
        shade,
    );
    poly(
        &mut im,
        &[(16, 25), (16, 14), (18, 12), (19, 14), (20, 25), (18, 27)],
        stone,
    );
    line(&mut im, (17, 15), (18, 22), light, 1);
    // Naturally eroded notch and a carved, off-center mark distinguish the old monolith.
    put(&mut im, 20, 14, CLEAR);
    put(&mut im, 17, 18, shade);
    line(&mut im, (17, 18), (19, 20), seam, 1);

    poly(
        &mut im,
        &[
            (22, 24),
            (23, 12),
            (25, 10),
            (28, 13),
            (29, 25),
            (27, 28),
            (23, 27),
        ],
        shade,
    );
    poly(
        &mut im,
        &[(24, 23), (24, 14), (26, 12), (27, 14), (27, 24), (25, 26)],
        stone,
    );
    line(&mut im, (25, 15), (25, 21), light, 1);
    put(&mut im, 28, 12, CLEAR);
    // Moss gathers in lower cracks without hiding the weathered stone shapes.
    fill(&mut im, 6, 22, 3, 2, moss);
    fill(&mut im, 18, 25, 3, 2, moss);
    fill(&mut im, 25, 23, 3, 2, moss);
    put(&mut im, 7, 21, moss_light);
    put(&mut im, 19, 24, moss_light);
    put(&mut im, 26, 22, moss_light);
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
        for pair in xs.as_chunks::<2>().0 {
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
            ruined_building_with_chest(super::super::RuinTopic::Masonry),
            church_cross(),
            mineshaft_arch(),
            moon_dome(),
            moon_rabbit_house(),
            cheese_wedge(),
            ruined_church(),
            ruined_tower(),
            ancient_monument_ruin(),
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

        let church = ruined_church();
        assert_eq!(church.get_pixel(16, 20)[3], 0, "chapel arch remains open");
        let tower = ruined_tower();
        assert_eq!(tower.get_pixel(16, 18)[3], 0, "tower window remains open");
        let monument = ancient_monument_ruin();
        assert_eq!(
            monument.get_pixel(13, 14)[3],
            0,
            "broken pillars remain separate"
        );
        assert_ne!(church.as_raw(), tower.as_raw());
        assert_ne!(tower.as_raw(), monument.as_raw());
    }
}
