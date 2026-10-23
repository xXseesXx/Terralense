//! Transparent Cubiomes Viewer sprites and original TFG symbols shared by the map and exports.
//! Upstream images retain GPL-3.0 licensing; see assets/structure-icons/README.md.
use image::{ImageBuffer, Rgba, RgbaImage, imageops};
mod tfg;

pub const MAP_ICON_SIZE: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuinTopic {
    Masonry,
    Church,
    Tower,
    Monument,
}

/// One ruin category across mods, with the original building topic retained for its sprite.
pub fn ruin_topic(id: &str) -> Option<RuinTopic> {
    let id = id.split('@').next().unwrap_or(id).to_ascii_lowercase();
    let (namespace, path) = id.split_once(':').unwrap_or(("", id.as_str()));
    let words: Vec<_> = path.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let has = |word| words.contains(&word);
    if has("living") || has("lived") {
        return None;
    }
    let monument = has("monument") || has("monuments");
    let ruined = matches!(namespace, "tfc_ruins" | "tfc_ruined_world")
        || id == "tfg:aqueduct/aqueduct"
        || matches!(
            id.as_str(),
            "tfg:ocean/ocean_moai" | "tfg:ocean/ocean_moai_0"
        )
        || ["ruin", "ruins", "ruined", "abandoned"]
            .into_iter()
            .any(has)
        || has("ancient") && monument;
    if !ruined {
        return None;
    }
    Some(if has("church") || has("churches") || has("chapel") {
        RuinTopic::Church
    } else if [
        "tower",
        "towers",
        "towerhouse",
        "towerhouses",
        "castle",
        "castles",
        "fortress",
    ]
    .into_iter()
    .any(has)
    {
        RuinTopic::Tower
    } else if monument || has("temple") || has("temples") || has("aqueduct") || has("moai") {
        RuinTopic::Monument
    } else {
        RuinTopic::Masonry
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    WorldSpawn,
    Player,
    Settlement,
    Camp,
    Church,
    Vessel,
    Temple,
    Tower,
    Fortress,
    Ruin,
    DigSite,
    Nest,
    Dungeon,
    Portal,
    Mine,
    Monument,
    Well,
    Impact,
    Fossil,
    Treasure,
    Igloo,
    Outpost,
    Landmark,
}

/// Map namespaced and vanilla IDs to a semantic silhouette. Unknown structures deliberately
/// receive a generic landmark pin so new pack content never disappears from the map.
pub fn family(id: &str) -> Family {
    let id = id.to_ascii_lowercase();
    let id = id.rsplit_once(':').map_or(id.as_str(), |(_, path)| path);
    // These point annotations are deliberately kept out of structure families.
    if id == "world_spawn" {
        Family::WorldSpawn
    } else if id.starts_with("player/") {
        Family::Player
    } else if id.contains("church") {
        Family::Church
    } else if id.contains("camp") {
        Family::Camp
    } else if id.contains("portal") || id.contains("gateway") {
        Family::Portal
    } else if id.contains("fossil") {
        Family::Fossil
    } else if id.contains("treasure") {
        Family::Treasure
    } else if id.contains("igloo") {
        Family::Igloo
    } else if id.contains("outpost") {
        Family::Outpost
    } else if ["shipwreck", "ship", "rowboat", "sloop", "boat", "vessel"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::Vessel
    } else if ["village", "settlement", "camp", "coop", "house"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::Settlement
    } else if id.contains("temple") {
        Family::Temple
    } else if id.contains("tower") {
        Family::Tower
    } else if ["fortress", "bastion", "stronghold", "castle", "mansion"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::Fortress
    } else if ["ruin", "ruined", "abandoned"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::Ruin
    } else if ["digsite", "dig_site", "paleontology"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::DigSite
    } else if ["nest", "wraptor", "arthropod"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::Nest
    } else if ["dungeon", "chamber", "crypt", "ancient_city"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::Dungeon
    } else if ["portal", "gateway"].iter().any(|word| id.contains(word)) {
        Family::Portal
    } else if ["mineshaft", "mine"].iter().any(|word| id.contains(word)) {
        Family::Mine
    } else if ["monument", "aqueduct", "moai", "coral"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::Monument
    } else if ["well", "oil_well"].iter().any(|word| id.contains(word)) {
        Family::Well
    } else if ["meteorite", "meteor", "impact"]
        .iter()
        .any(|word| id.contains(word))
    {
        Family::Impact
    } else {
        Family::Landmark
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Sprite {
    Cubiomes(&'static str),
    Tfg(Family),
    MoonCheese,
    MoonBase,
    MoonRabbitHouse,
    Ruin(RuinTopic),
    Glyph(Family),
}

fn sprite(id: &str) -> Sprite {
    let id = id.split('@').next().unwrap_or(id).to_ascii_lowercase();
    match id.as_str() {
        "tfg:moon/cheese_ores" => return Sprite::MoonCheese,
        "tfg:moon/moonbase" => return Sprite::MoonBase,
        "tfg:moon/moon_rabbit_houses" => return Sprite::MoonRabbitHouse,
        _ => {}
    }
    let kind = family(&id);
    let vanilla = id.strip_prefix("minecraft:").and_then(|path| match path {
        "world_spawn" => Some("spawn"),
        "fortress" | "nether_fortress" | "nether_complexes" => Some("fortress"),
        "bastion_remnant" | "bastion" => Some("bastion"),
        "stronghold" | "strongholds" => Some("stronghold"),
        "mansion" | "woodland_mansion" => Some("mansion"),
        "village" => Some("village"),
        path if path.starts_with("village_") => Some("village"),
        "desert_pyramid" | "desert_temple" => Some("desert"),
        "jungle_pyramid" | "jungle_temple" => Some("jungle"),
        "swamp_hut" | "witch_hut" => Some("hut"),
        "shipwreck" | "shipwreck_beached" => Some("shipwreck"),
        "ocean_ruin" | "ocean_ruin_cold" | "ocean_ruin_warm" | "ocean_ruins" => Some("ruins"),
        "monument" | "ocean_monument" => Some("monument"),
        "ruined_portal" => Some("portal"),
        path if path.starts_with("ruined_portal_") => Some("portal"),
        "end_city" | "endcity" => Some("endcity"),
        "end_gateway" => Some("gateway"),
        "pillager_outpost" | "outpost" => Some("outpost"),
        "ancient_city" => Some("ancient_city"),
        "trial_chambers" => Some("chambers"),
        "trail_ruins" => Some("trails"),
        "buried_treasure" => Some("treasure"),
        "igloo" => Some("igloo"),
        "desert_well" => Some("well"),
        "amethyst_geode" => Some("geode"),
        _ => None,
    });
    if let Some(name) = vanilla {
        return Sprite::Cubiomes(name);
    }
    if let Some(topic) = ruin_topic(&id) {
        return Sprite::Ruin(topic);
    }
    // Intact TFG churches, camps and timber mine openings retain their own symbols.
    if matches!(kind, Family::Mine | Family::Camp | Family::Church) {
        return Sprite::Tfg(kind);
    }
    match kind {
        Family::Settlement | Family::Ruin => Sprite::Tfg(kind),
        Family::Vessel => Sprite::Cubiomes("shipwreck"),
        Family::Temple => Sprite::Cubiomes("desert"),
        Family::Tower | Family::Outpost => Sprite::Cubiomes("outpost"),
        Family::Fortress => Sprite::Cubiomes("stronghold"),
        Family::Portal => Sprite::Cubiomes("portal"),
        Family::Dungeon => Sprite::Cubiomes("chambers"),
        Family::Treasure => Sprite::Cubiomes("treasure"),
        Family::Igloo => Sprite::Cubiomes("igloo"),
        Family::Well => Sprite::Cubiomes("well"),
        Family::WorldSpawn => Sprite::Cubiomes("spawn"),
        _ => Sprite::Glyph(kind),
    }
}

fn cubiomes_png(name: &str) -> &'static [u8] {
    match name {
        "village" => include_bytes!("../assets/structure-icons/cubiomes/village.png"),
        "fortress" => include_bytes!("../assets/structure-icons/cubiomes/fortress.png"),
        "portal" => include_bytes!("../assets/structure-icons/cubiomes/portal.png"),
        "monument" => include_bytes!("../assets/structure-icons/cubiomes/monument.png"),
        "shipwreck" => include_bytes!("../assets/structure-icons/cubiomes/shipwreck.png"),
        "stronghold" => include_bytes!("../assets/structure-icons/cubiomes/stronghold.png"),
        "spawn" => include_bytes!("../assets/structure-icons/cubiomes/spawn.png"),
        "bastion" => include_bytes!("../assets/structure-icons/cubiomes/bastion.png"),
        "outpost" => include_bytes!("../assets/structure-icons/cubiomes/outpost.png"),
        "endcity" => include_bytes!("../assets/structure-icons/cubiomes/endcity.png"),
        "mansion" => include_bytes!("../assets/structure-icons/cubiomes/mansion.png"),
        "hut" => include_bytes!("../assets/structure-icons/cubiomes/hut.png"),
        "igloo" => include_bytes!("../assets/structure-icons/cubiomes/igloo.png"),
        "ruins" => include_bytes!("../assets/structure-icons/cubiomes/ruins.png"),
        "treasure" => include_bytes!("../assets/structure-icons/cubiomes/treasure.png"),
        "ancient_city" => include_bytes!("../assets/structure-icons/cubiomes/ancient_city.png"),
        "geode" => include_bytes!("../assets/structure-icons/cubiomes/geode.png"),
        "well" => include_bytes!("../assets/structure-icons/cubiomes/well.png"),
        "trails" => include_bytes!("../assets/structure-icons/cubiomes/trails.png"),
        "chambers" => include_bytes!("../assets/structure-icons/cubiomes/chambers.png"),
        "gateway" => include_bytes!("../assets/structure-icons/cubiomes/gateway.png"),
        "desert" => include_bytes!("../assets/structure-icons/cubiomes/desert.png"),
        "jungle" => include_bytes!("../assets/structure-icons/cubiomes/jungle.png"),
        _ => unreachable!("known embedded Cubiomes icon"),
    }
}

fn cubiomes_image(name: &str) -> RgbaImage {
    let mut image = image::load_from_memory(cubiomes_png(name))
        .expect("embedded Cubiomes PNG")
        .to_rgba8();
    for pixel in image.pixels_mut() {
        if pixel[3] < 16 {
            pixel[3] = 0;
        }
    }
    // Some upstream block symbols have a solid two-pixel black frame. Keep the
    // original files intact, but omit that frame when drawing background-free markers.
    let frame = image.width() > 4
        && image.height() > 4
        && image
            .enumerate_pixels()
            .filter(|(x, y, _)| {
                *x < 2 || *y < 2 || *x >= image.width() - 2 || *y >= image.height() - 2
            })
            .all(|(_, _, p)| p[3] == 255 && p[0] < 12 && p[1] < 12 && p[2] < 12);
    if frame {
        imageops::crop_imm(&image, 2, 2, image.width() - 4, image.height() - 4).to_image()
    } else {
        image
    }
}

/// Draw each object's silhouette on alpha, preserving the original pixel-art edges.
pub fn raster(id: &str, size: u32) -> RgbaImage {
    let image = match sprite(id) {
        Sprite::Cubiomes(name) => cubiomes_image(name),
        Sprite::MoonCheese => moon_cheese(),
        Sprite::MoonBase => tfg::moon_dome(),
        Sprite::MoonRabbitHouse => tfg::moon_rabbit_house(),
        Sprite::Ruin(RuinTopic::Masonry) => tfg::cracked_limestone_brick(),
        Sprite::Ruin(RuinTopic::Church) => tfg::ruined_church(),
        Sprite::Ruin(RuinTopic::Tower) => tfg::ruined_tower(),
        Sprite::Ruin(RuinTopic::Monument) => tfg::ancient_monument_ruin(),
        Sprite::Tfg(Family::Camp) => tfg::campfire(),
        Sprite::Tfg(Family::Settlement) => tfg::village_head(),
        Sprite::Tfg(Family::Ruin) => tfg::cracked_limestone_brick(),
        Sprite::Tfg(Family::Church) => tfg::church_cross(),
        Sprite::Tfg(Family::Mine) => tfg::mineshaft_arch(),
        Sprite::Tfg(_) => unreachable!("known TFG icon"),
        Sprite::Glyph(kind) => glyph(kind),
    };
    fit_silhouette(&image, size.clamp(8, 128))
}

fn moon_cheese() -> RgbaImage {
    use std::sync::OnceLock;
    const LOCAL_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ad-astra-cheese.png"));
    static IMAGE: OnceLock<RgbaImage> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            if LOCAL_PNG.is_empty() {
                tfg::cheese_wedge()
            } else {
                image::load_from_memory(LOCAL_PNG)
                    .expect("local Ad Astra cheese PNG")
                    .to_rgba8()
            }
        })
        .clone()
}

fn fit_silhouette(image: &RgbaImage, size: u32) -> RgbaImage {
    let mut bounds = (image.width(), image.height(), 0, 0);
    for (x, y, _) in image.enumerate_pixels().filter(|(_, _, p)| p[3] != 0) {
        bounds = (
            bounds.0.min(x),
            bounds.1.min(y),
            bounds.2.max(x + 1),
            bounds.3.max(y + 1),
        );
    }
    let mut canvas = ImageBuffer::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    if bounds.2 <= bounds.0 || bounds.3 <= bounds.1 {
        return canvas;
    }
    let cropped = imageops::crop_imm(
        image,
        bounds.0,
        bounds.1,
        bounds.2 - bounds.0,
        bounds.3 - bounds.1,
    )
    .to_image();
    let scale = (size - 2) as f64 / cropped.width().max(cropped.height()) as f64;
    let width = (cropped.width() as f64 * scale).round().max(1.) as u32;
    let height = (cropped.height() as f64 * scale).round().max(1.) as u32;
    let scaled = imageops::resize(&cropped, width, height, imageops::FilterType::Nearest);
    imageops::overlay(
        &mut canvas,
        &scaled,
        ((size - width) / 2) as i64,
        ((size - height) / 2) as i64,
    );
    canvas
}

fn glyph(kind: Family) -> RgbaImage {
    let mut im = ImageBuffer::from_pixel(32, 32, Rgba([0, 0, 0, 0]));
    let accent = color(kind);
    match kind {
        Family::DigSite => digsite(&mut im, accent),
        Family::Nest => nest(&mut im, accent),
        Family::Monument => monument(&mut im, accent),
        Family::Impact => impact(&mut im, accent),
        Family::Fossil => fossil(&mut im, accent),
        Family::Player => player(&mut im, accent),
        Family::Landmark => landmark(&mut im, accent),
        _ => unreachable!("family uses embedded or custom sprite"),
    }
    im
}

fn color(f: Family) -> [u8; 4] {
    match f {
        Family::WorldSpawn => [255, 220, 112, 255],
        Family::Player => [137, 207, 255, 255],
        Family::Settlement | Family::Camp | Family::Church => [229, 183, 111, 255],
        Family::Vessel => [100, 190, 211, 255],
        Family::Temple => [238, 202, 126, 255],
        Family::Tower => [191, 210, 222, 255],
        Family::Fortress => [213, 150, 111, 255],
        Family::Ruin => [190, 177, 143, 255],
        Family::DigSite => [222, 173, 103, 255],
        Family::Nest => [164, 206, 136, 255],
        Family::Dungeon => [181, 154, 208, 255],
        Family::Portal => [100, 207, 195, 255],
        Family::Mine => [210, 186, 132, 255],
        Family::Monument => [124, 193, 190, 255],
        Family::Well => [104, 176, 205, 255],
        Family::Impact => [238, 135, 101, 255],
        Family::Fossil => [229, 216, 184, 255],
        Family::Treasure => [245, 193, 93, 255],
        Family::Igloo => [181, 218, 233, 255],
        Family::Outpost => [192, 174, 148, 255],
        Family::Landmark => [204, 210, 185, 255],
    }
}
fn px(im: &mut RgbaImage, x: i32, y: i32, c: [u8; 4]) {
    if (0..32).contains(&x) && (0..32).contains(&y) {
        im.put_pixel(x as u32, y as u32, Rgba(c));
    }
}
fn line(im: &mut RgbaImage, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: [u8; 4], thick: i32) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        for oy in -thick..=thick {
            for ox in -thick..=thick {
                if ox * ox + oy * oy <= thick * thick + 1 {
                    px(im, x0 + ox, y0 + oy, c);
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
fn rect(im: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 4]) {
    for y in y0..=y1 {
        for x in x0..=x1 {
            px(im, x, y, c);
        }
    }
}
fn circle(im: &mut RgbaImage, cx: i32, cy: i32, r: i32, c: [u8; 4]) {
    for y in -r..=r {
        for x in -r..=r {
            if x * x + y * y <= r * r {
                px(im, cx + x, cy + y, c);
            }
        }
    }
}

fn roof(im: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: [u8; 4]) {
    line(im, x, y + h, x + w / 2, y, c, 1);
    line(im, x + w / 2, y, x + w, y + h, c, 1);
}

fn digsite(im: &mut RgbaImage, c: [u8; 4]) {
    line(im, 12, 9, 20, 17, c, 2);
    line(im, 20, 17, 20, 21, c, 1);
    line(im, 8, 22, 24, 22, c, 1);
    line(im, 11, 24, 21, 24, c, 1);
    line(im, 10, 20, 14, 24, c, 1);
}
fn nest(im: &mut RgbaImage, c: [u8; 4]) {
    for i in 0..4 {
        line(im, 8 + i, 20 + i / 2, 24 - i, 20 + i / 2, c, 1);
    }
    circle(im, 16, 16, 3, c);
}

fn monument(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 11, 11, 21, 22, c);
    rect(im, 9, 9, 23, 10, c);
    for y in [14, 18] {
        line(im, 12, y, 20, y, [18, 29, 37, 255], 1);
    }
    rect(im, 14, 23, 18, 25, c);
}

fn impact(im: &mut RgbaImage, c: [u8; 4]) {
    circle(im, 16, 18, 7, c);
    circle(im, 16, 18, 3, [18, 29, 37, 255]);
    line(im, 10, 9, 14, 14, c, 1);
    line(im, 12, 7, 15, 13, c, 1);
    line(im, 21, 8, 18, 14, c, 1);
}
fn landmark(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 10, 15, 22, 23, c);
    roof(im, 9, 10, 14, 5, c);
    rect(im, 15, 18, 17, 23, [18, 29, 37, 255]);
}
fn fossil(im: &mut RgbaImage, c: [u8; 4]) {
    line(im, 10, 11, 22, 22, c, 1);
    for (x, y) in [(10, 11), (22, 22)] {
        circle(im, x - 2, y + 1, 2, c);
        circle(im, x + 1, y - 2, 2, c);
    }
    line(im, 14, 11, 11, 17, c, 0);
    line(im, 18, 15, 15, 21, c, 0);
}

fn player(im: &mut RgbaImage, c: [u8; 4]) {
    // Clear head-and-shoulders silhouette, distinct from generated structures.
    circle(im, 16, 11, 4, c);
    for y in 16..=24 {
        let half = if y < 19 { 4 + (y - 16) } else { 7 };
        for x in 16 - half..=16 + half {
            if (x - 16) * (x - 16) + (y - 19) * (y - 19) <= 64 {
                px(im, x, y, c);
            }
        }
    }
}

#[cfg(feature = "gui")]
pub struct IconCache {
    textures: std::collections::HashMap<Sprite, eframe::egui::TextureHandle>,
}
#[cfg(feature = "gui")]
impl Default for IconCache {
    fn default() -> Self {
        Self {
            textures: std::collections::HashMap::new(),
        }
    }
}
#[cfg(feature = "gui")]
impl IconCache {
    pub fn get(&mut self, ctx: &eframe::egui::Context, id: &str) -> &eframe::egui::TextureHandle {
        let sprite = sprite(id);
        self.textures.entry(sprite).or_insert_with(|| {
            let im = raster(id, 32);
            let pixels = im
                .pixels()
                .map(|p| eframe::egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                .collect();
            let data = eframe::egui::ColorImage::new([32, 32], pixels);
            ctx.load_texture(
                format!("structure-icon-{sprite:?}"),
                data,
                eframe::egui::TextureOptions::NEAREST,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruin_topics_follow_condition_and_building_type_across_mods() {
        for (id, expected) in [
            ("tfc_ruins:ruin_rich", RuinTopic::Masonry),
            (
                "tfc_ruined_world:small_limestone_church_2",
                RuinTopic::Church,
            ),
            ("tfc_ruined_world:towerhouse_2", RuinTopic::Tower),
            ("tfc_ruined_world:castle_1", RuinTopic::Tower),
            ("tfc_ruined_world:ancient_monument_2", RuinTopic::Monument),
            ("tfg:aqueduct/aqueduct", RuinTopic::Monument),
            ("tfg:ocean/ocean_moai", RuinTopic::Monument),
            ("tfg:ocean/ocean_moai_0@-138,-156", RuinTopic::Monument),
            ("mod:ruins/stone_church@-31,84", RuinTopic::Church),
            ("mod:ancient_monuments/stone_circle", RuinTopic::Monument),
            ("wan_ancient_beasts:abandoned_village", RuinTopic::Masonry),
        ] {
            assert_eq!(ruin_topic(id), Some(expected), "{id}");
            assert_eq!(sprite(id), Sprite::Ruin(expected), "{id}");
        }
        for id in [
            "tfg:beneath/tower",
            "mod:church",
            "minecraft:monument",
            "tfg:ocean/ocean_monument",
            "minecraft:ancient_city",
            "tfg:illagers/arabic_village_living",
            "tfg:moon/moonbase",
            "tfg:earth/ancient_shield_volcano",
        ] {
            assert_eq!(ruin_topic(id), None, "{id}");
        }
        assert_eq!(
            sprite("minecraft:ruined_portal"),
            Sprite::Cubiomes("portal")
        );
        assert_eq!(
            raster("tfg:aqueduct/aqueduct", 32),
            raster("tfc_ruined_world:ancient_monument_2", 32),
            "Aqueduct uses the original ruined monument silhouette"
        );
    }

    #[test]
    fn requested_tfg_symbols_and_cubiomes_variants_keep_their_identity() {
        assert_eq!(sprite("tfg:moon/cheese_ores@-9,24"), Sprite::MoonCheese);
        assert_eq!(sprite("tfg:moon/moonbase"), Sprite::MoonBase);
        assert_eq!(
            sprite("tfg:moon/moon_rabbit_houses"),
            Sprite::MoonRabbitHouse
        );
        assert_eq!(sprite("tfg:illagers/camps"), Sprite::Tfg(Family::Camp));
        assert_eq!(sprite("example:church"), Sprite::Tfg(Family::Church));
        assert_eq!(
            sprite("tfg:illagers/illager_camps"),
            Sprite::Tfg(Family::Camp)
        );
        assert_eq!(
            sprite("tfg:illagers/norse_village_living"),
            Sprite::Tfg(Family::Settlement)
        );
        assert_eq!(
            sprite("tfc_ruins:ruin_small"),
            Sprite::Ruin(RuinTopic::Masonry)
        );
        assert_eq!(
            sprite("tfc_ruined_world:limestone_church"),
            Sprite::Ruin(RuinTopic::Church)
        );
        assert_eq!(sprite("tfg:mineshaft/mineshaft"), Sprite::Tfg(Family::Mine));
        assert_eq!(sprite("minecraft:mineshaft"), Sprite::Tfg(Family::Mine));
        assert_ne!(
            sprite("minecraft:village"),
            sprite("tfg:illagers/norse_village_living")
        );
        assert_ne!(
            sprite("minecraft:desert_pyramid"),
            sprite("minecraft:jungle_pyramid")
        );
        assert_ne!(
            raster("minecraft:desert_pyramid", 32),
            raster("minecraft:jungle_pyramid", 32)
        );
        for id in [
            "tfg:illagers/camps",
            "tfg:illagers/norse_village_living",
            "tfc_ruins:ruin_small",
            "tfc_ruined_world:limestone_church",
            "tfg:mineshaft/mineshaft",
            "minecraft:stronghold",
            "tfg:moon/cheese_ores",
            "tfg:moon/moonbase",
            "tfg:moon/moon_rabbit_houses",
            "tfc_ruined_world:towerhouse_1",
            "tfc_ruined_world:ancient_monument_1",
        ] {
            let icon = raster(id, 32);
            assert!(icon.pixels().any(|pixel| pixel[3] == 0), "{id}");
            assert!(icon.pixels().any(|pixel| pixel[3] == 255), "{id}");
            for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
                assert_eq!(icon.get_pixel(x, y)[3], 0, "{id}");
            }
        }
        let cross = raster("example:church", 32);
        assert!(
            cross.pixels().filter(|pixel| pixel[3] != 0).count() < 400,
            "no common disc behind the cross"
        );
        let gate = raster("minecraft:mineshaft", 32);
        assert_eq!(
            gate.get_pixel(16, 16)[3],
            0,
            "the tunnel opening stays transparent"
        );
    }

    #[test]
    fn installed_catalog_gets_family_icons_and_fallback_is_visible() {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../assets/data/tfg/installed-structure-runtime.json"
        ))
        .unwrap();
        let mut count = 0;
        for item in data["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["kind"] == "structure")
        {
            let id = item["id"].as_str().unwrap();
            let icon = raster(id, 24);
            assert_eq!(icon.dimensions(), (24, 24), "{id}");
            assert!(icon.pixels().any(|p| p[3] > 0), "{id}");
            count += 1;
        }
        assert!(count >= 60);
        assert_eq!(family("minecraft:shipwreck"), Family::Vessel);
        assert_eq!(family("minecraft:ruined_portal"), Family::Portal);
        assert_eq!(family("minecraft:nether_fossil"), Family::Fossil);
        assert_eq!(family("tfg:beneath/outpost"), Family::Outpost);
        assert_eq!(family("newmod:unknown_architecture"), Family::Landmark);
        assert_eq!(family("minecraft:world_spawn"), Family::WorldSpawn);
        assert_eq!(family("minecraft:player/1234-abcd"), Family::Player);
    }
    #[test]
    fn icon_families_are_visually_distinct() {
        let ids = [
            "minecraft:village",
            "minecraft:shipwreck",
            "minecraft:desert_temple",
            "tfg:beneath/tower",
            "minecraft:nether_fortress",
            "minecraft:ruined_portal",
            "tfg:moon/cheese_ores",
            "tfg:moon/moonbase",
            "tfg:moon/moon_rabbit_houses",
            "tfc_ruined_world:limestone_church",
            "tfc_ruined_world:towerhouse_1",
            "tfc_ruined_world:ancient_monument_1",
            "tfc_ruins:ruin_small",
        ];
        let images: Vec<_> = ids.iter().map(|id| raster(id, 32).into_raw()).collect();
        for i in 0..images.len() {
            for j in i + 1..images.len() {
                assert_ne!(images[i], images[j]);
            }
        }
    }

    #[test]
    fn spawn_and_player_icons_are_transparent_and_distinct() {
        let spawn = raster("minecraft:world_spawn", 32);
        let player = raster("minecraft:player/1234-abcd", 32);
        assert_ne!(spawn.as_raw(), player.as_raw());
        assert_eq!(spawn.get_pixel(0, 0)[3], 0);
        assert_eq!(player.get_pixel(0, 0)[3], 0);
    }
}
