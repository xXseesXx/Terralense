//! Original, dependency-light structure pins shared by the live map and composed exports.
//! The glyphs use simple vector silhouettes rather than copied game or third-party art.
use image::{ImageBuffer, Rgba, RgbaImage, imageops};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Settlement,
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
    if id.contains("portal") || id.contains("gateway") {
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

/// Render a transparent 32px vector glyph at a requested raster size.
pub fn raster(id: &str, size: u32) -> RgbaImage {
    let size = size.clamp(8, 128);
    let mut im = ImageBuffer::from_pixel(32, 32, Rgba([0, 0, 0, 0]));
    let accent = color(family(id));
    // Dark circular plaque and fine pale rim keep icons legible on every biome palette.
    circle(&mut im, 16, 16, 15, [18, 29, 37, 242]);
    circle_ring(&mut im, 16, 16, 14, [218, 227, 224, 230]);
    match family(id) {
        Family::Settlement => settlement(&mut im, accent),
        Family::Vessel => vessel(&mut im, accent),
        Family::Temple => temple(&mut im, accent),
        Family::Tower => tower(&mut im, accent),
        Family::Fortress => fortress(&mut im, accent),
        Family::Ruin => ruin(&mut im, accent),
        Family::DigSite => digsite(&mut im, accent),
        Family::Nest => nest(&mut im, accent),
        Family::Dungeon => dungeon(&mut im, accent),
        Family::Portal => portal(&mut im, accent),
        Family::Mine => mine(&mut im, accent),
        Family::Monument => monument(&mut im, accent),
        Family::Well => well(&mut im, accent),
        Family::Impact => impact(&mut im, accent),
        Family::Fossil => fossil(&mut im, accent),
        Family::Treasure => treasure(&mut im, accent),
        Family::Igloo => igloo(&mut im, accent),
        Family::Outpost => outpost(&mut im, accent),
        Family::Landmark => landmark(&mut im, accent),
    }
    imageops::resize(&im, size, size, imageops::FilterType::Lanczos3)
}

fn color(f: Family) -> [u8; 4] {
    match f {
        Family::Settlement => [229, 183, 111, 255],
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
fn circle_ring(im: &mut RgbaImage, cx: i32, cy: i32, r: i32, c: [u8; 4]) {
    for y in -r..=r {
        for x in -r..=r {
            let d = x * x + y * y;
            if (r - 1) * (r - 1) <= d && d <= r * r {
                px(im, cx + x, cy + y, c);
            }
        }
    }
}
fn roof(im: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: [u8; 4]) {
    line(im, x, y + h, x + w / 2, y, c, 1);
    line(im, x + w / 2, y, x + w, y + h, c, 1);
}
fn settlement(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 8, 20, 23, 23, c);
    roof(im, 8, 12, 9, 6, c);
    roof(im, 16, 10, 8, 7, c);
    rect(im, 11, 18, 13, 22, [18, 29, 37, 255]);
    rect(im, 19, 17, 21, 20, [18, 29, 37, 255]);
}
fn vessel(im: &mut RgbaImage, c: [u8; 4]) {
    line(im, 7, 20, 24, 20, c, 1);
    line(im, 9, 21, 22, 21, c, 1);
    line(im, 11, 22, 20, 22, c, 1);
    line(im, 16, 8, 16, 20, c, 1);
    line(im, 16, 9, 22, 16, c, 1);
    line(im, 15, 11, 10, 17, c, 1);
}
fn temple(im: &mut RgbaImage, c: [u8; 4]) {
    line(im, 8, 12, 24, 12, c, 1);
    roof(im, 8, 7, 16, 5, c);
    for x in [10, 14, 18, 22] {
        line(im, x, 13, x, 21, c, 1);
    }
    line(im, 7, 22, 25, 22, c, 1);
    line(im, 9, 24, 23, 24, c, 1);
}
fn tower(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 11, 13, 21, 24, c);
    rect(im, 13, 9, 19, 12, c);
    line(im, 12, 8, 16, 5, c, 1);
    line(im, 16, 5, 20, 8, c, 1);
    rect(im, 15, 18, 17, 23, [18, 29, 37, 255]);
    rect(im, 14, 15, 15, 17, [18, 29, 37, 255]);
    rect(im, 18, 15, 19, 17, [18, 29, 37, 255]);
}
fn fortress(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 8, 13, 24, 23, c);
    for x in [8, 12, 17, 21] {
        rect(im, x, 9, x + 2, 12, c);
    }
    rect(im, 14, 17, 18, 23, [18, 29, 37, 255]);
}
fn ruin(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 8, 17, 12, 23, c);
    rect(im, 13, 20, 17, 23, c);
    rect(im, 18, 14, 22, 23, c);
    line(im, 9, 15, 12, 15, c, 1);
    line(im, 19, 12, 22, 12, c, 1);
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
fn dungeon(im: &mut RgbaImage, c: [u8; 4]) {
    line(im, 9, 23, 9, 16, c, 1);
    line(im, 9, 16, 12, 12, c, 1);
    line(im, 12, 12, 20, 12, c, 1);
    line(im, 20, 12, 23, 16, c, 1);
    line(im, 23, 16, 23, 23, c, 1);
    line(im, 9, 23, 23, 23, c, 1);
    rect(im, 14, 18, 18, 23, c);
}
fn portal(im: &mut RgbaImage, c: [u8; 4]) {
    for r in [9, 6, 3] {
        circle_ring(im, 16, 16, r, c);
    }
    line(im, 9, 24, 23, 24, c, 1);
}
fn mine(im: &mut RgbaImage, c: [u8; 4]) {
    roof(im, 9, 12, 14, 8, c);
    line(im, 11, 18, 11, 23, c, 1);
    line(im, 21, 18, 21, 23, c, 1);
    line(im, 12, 19, 20, 19, c, 1);
    line(im, 13, 22, 19, 22, c, 1);
}
fn monument(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 11, 11, 21, 22, c);
    rect(im, 9, 9, 23, 10, c);
    for y in [14, 18] {
        line(im, 12, y, 20, y, [18, 29, 37, 255], 1);
    }
    rect(im, 14, 23, 18, 25, c);
}
fn well(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 10, 16, 22, 22, c);
    rect(im, 13, 18, 19, 20, [64, 156, 190, 255]);
    line(im, 9, 15, 23, 15, c, 1);
    line(im, 11, 12, 11, 15, c, 1);
    line(im, 21, 12, 21, 15, c, 1);
    line(im, 11, 12, 21, 12, c, 1);
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
fn treasure(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 8, 13, 24, 23, c);
    line(im, 8, 16, 24, 16, [18, 29, 37, 255], 0);
    rect(im, 14, 15, 18, 19, [245, 236, 188, 255]);
    rect(im, 15, 16, 17, 18, [18, 29, 37, 255]);
    roof(im, 8, 9, 16, 4, c);
}
fn igloo(im: &mut RgbaImage, c: [u8; 4]) {
    circle(im, 16, 20, 10, c);
    rect(im, 6, 23, 26, 30, [18, 29, 37, 255]);
    line(im, 7, 22, 25, 22, c, 1);
    rect(im, 14, 18, 18, 23, [18, 29, 37, 255]);
    line(im, 8, 16, 24, 16, [102, 153, 177, 255], 0);
}
fn outpost(im: &mut RgbaImage, c: [u8; 4]) {
    rect(im, 11, 10, 21, 16, c);
    roof(im, 8, 6, 16, 4, c);
    line(im, 12, 16, 10, 24, c, 1);
    line(im, 20, 16, 22, 24, c, 1);
    line(im, 10, 22, 21, 18, c, 0);
    rect(im, 14, 11, 18, 14, [18, 29, 37, 255]);
}

#[cfg(feature = "gui")]
pub struct IconCache {
    textures: std::collections::HashMap<Family, eframe::egui::TextureHandle>,
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
        let family = family(id);
        self.textures.entry(family).or_insert_with(|| {
            let im = raster(id, 32);
            let pixels = im
                .pixels()
                .map(|p| eframe::egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                .collect();
            let data = eframe::egui::ColorImage::new([32, 32], pixels);
            ctx.load_texture(
                format!("structure-icon-{family:?}"),
                data,
                eframe::egui::TextureOptions::LINEAR,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        ];
        let images: Vec<_> = ids.iter().map(|id| raster(id, 32).into_raw()).collect();
        for i in 0..images.len() {
            for j in i + 1..images.len() {
                assert_ne!(images[i], images[j]);
            }
        }
    }
}
