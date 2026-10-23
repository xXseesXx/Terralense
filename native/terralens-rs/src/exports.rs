//! Bounded, cancellable exports. Requests execute in the atlas engine process.
use crate::{
    atlas::{self, Layer},
    model::{Bounds, Marker, Point},
    world::World,
};
use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use image::{ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
const MAX_SIDE: u32 = 4096;
const MAX_PIXELS: u64 = 16_777_216;
const WORLD_BORDER: f64 = 30_000_000.;
const MAX_SEED_FILE: u64 = 1_048_576;
const MAX_IMPORTED_SEEDS: usize = 128;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);
static EXPORT_FONT: OnceLock<FontArc> = OnceLock::new();
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Map {
        bounds: Bounds,
        layer: Layer,
        width: u32,
        height: u32,
        path: PathBuf,
    },
    /// A fresh seed-derived map raster with share-ready cartographic overlays.
    ComposedMap {
        bounds: Bounds,
        layer: Layer,
        width: u32,
        height: u32,
        path: PathBuf,
        markers: Vec<Marker>,
        #[serde(default)]
        composition: Composition,
    },
    Markers {
        markers: Vec<Marker>,
        path: PathBuf,
    },
    Data {
        value: serde_json::Value,
        path: PathBuf,
    },
    ReadSession {
        path: PathBuf,
    },
    ReadSeeds {
        path: PathBuf,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Composition {
    pub coordinates: bool,
    pub scale_bar: bool,
    pub legend: Vec<LegendChip>,
    pub scale: Option<ScaleLegend>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LegendChip {
    pub label: String,
    pub color: [u8; 3],
    pub percent: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScaleLegend {
    pub min: f32,
    pub max: f32,
    pub unit: String,
    pub low: String,
    pub high: String,
    pub stops: Vec<[u8; 3]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub path: PathBuf,
    pub note: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}
pub fn default_directory() -> PathBuf {
    std::env::var_os("XDG_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")))
        .unwrap_or_else(std::env::temp_dir)
        .join("TerraLens")
}
pub fn execute(
    world: &World,
    request: &Request,
    cancel: impl Fn() -> bool,
) -> Result<Outcome, String> {
    execute_with_display(
        world,
        request,
        &crate::display::DisplayPrefs::default(),
        cancel,
    )
}
pub fn execute_with_display(
    world: &World,
    request: &Request,
    display: &crate::display::DisplayPrefs,
    cancel: impl Fn() -> bool,
) -> Result<Outcome, String> {
    match request {
        Request::Map {
            bounds,
            layer,
            width,
            height,
            path,
        } => export_map(
            world, *bounds, *layer, *width, *height, path, display, cancel,
        ),
        Request::ComposedMap {
            bounds,
            layer,
            width,
            height,
            path,
            markers,
            composition,
        } => export_composed_map(
            world,
            *bounds,
            *layer,
            *width,
            *height,
            path,
            markers,
            composition,
            display,
            cancel,
        ),
        Request::Markers { markers, path } => export_markers(markers, path, cancel),
        Request::Data { value, path } => {
            let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
            atomic_write(path, &bytes, &cancel)?;
            Ok(Outcome {
                path: path.clone(),
                note: "JSON".into(),
                data: None,
            })
        }
        Request::ReadSession { path } => {
            if cancel() {
                return Err("Cancelled".into());
            }
            let file = fs::File::open(path).map_err(|e| format!("Could not open session: {e}"))?;
            if file.metadata().map_err(|e| e.to_string())?.len() > 1_048_576 {
                return Err("Session exceeds 1 MiB".into());
            }
            let value = serde_json::from_reader(file.take(1_048_577))
                .map_err(|e| format!("Invalid session JSON: {e}"))?;
            Ok(Outcome {
                path: path.clone(),
                note: "Session loaded".into(),
                data: Some(value),
            })
        }
        Request::ReadSeeds { path } => read_seeds(path, cancel),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedSeed {
    pub seed: i64,
    pub name: String,
}

/// Parses a Cubiomes style seed list or JSON seed array without doing file I/O.
pub fn parse_seed_list(input: &str) -> Result<Vec<ImportedSeed>, String> {
    parse_seed_list_cancellable(input, || false)
}

fn read_seeds(path: &PathBuf, cancel: impl Fn() -> bool) -> Result<Outcome, String> {
    if cancel() {
        return Err("Cancelled".into());
    }
    let file = fs::File::open(path).map_err(|e| format!("Could not open seed list: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_SEED_FILE {
        return Err("Seed list exceeds 1 MiB".into());
    }
    let mut bytes = Vec::new();
    let mut reader = file.take(MAX_SEED_FILE + 1);
    let mut buf = [0u8; 16 * 1024];
    loop {
        if cancel() {
            return Err("Cancelled".into());
        }
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Could not read seed list: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if bytes.len() as u64 > MAX_SEED_FILE {
            return Err("Seed list exceeds 1 MiB".into());
        }
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Seed list must be UTF-8".to_string())?;
    let seeds = parse_seed_list_cancellable(text, &cancel)?;
    let data = serde_json::json!({"seedList": seeds});
    Ok(Outcome {
        path: path.clone(),
        note: format!(
            "Imported {} seeds",
            data["seedList"].as_array().map_or(0, Vec::len)
        ),
        data: Some(data),
    })
}

fn parse_seed_list_cancellable(
    input: &str,
    cancel: impl Fn() -> bool,
) -> Result<Vec<ImportedSeed>, String> {
    if cancel() {
        return Err("Cancelled".into());
    }
    let trimmed = input.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err("Seed list is empty".into());
    }
    if trimmed.starts_with('[') {
        return parse_seed_json(trimmed, cancel);
    }
    parse_seed_lines(trimmed, cancel)
}

fn parse_seed_json(input: &str, cancel: impl Fn() -> bool) -> Result<Vec<ImportedSeed>, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| format!("Invalid seed JSON: {e}"))?;
    let array = value
        .as_array()
        .ok_or_else(|| "Seed JSON must be an array".to_string())?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (row, value) in array.iter().enumerate() {
        if cancel() {
            return Err("Cancelled".into());
        }
        let entry = match value {
            serde_json::Value::Number(n) => ImportedSeed {
                seed: parse_i64(&n.to_string(), row + 1)?,
                name: String::new(),
            },
            serde_json::Value::String(s) => ImportedSeed {
                seed: parse_i64(s, row + 1)?,
                name: String::new(),
            },
            serde_json::Value::Object(obj) => {
                let seed = obj
                    .get("seed")
                    .ok_or_else(|| format!("Seed JSON row {} has no seed", row + 1))?;
                let seed = match seed {
                    serde_json::Value::String(s) => parse_i64(s, row + 1)?,
                    serde_json::Value::Number(n) => parse_i64(&n.to_string(), row + 1)?,
                    _ => return Err(format!("Seed JSON row {} has invalid seed", row + 1)),
                };
                let name = obj
                    .get("name")
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| format!("Seed JSON row {} has invalid name", row + 1))
                    })
                    .transpose()?
                    .unwrap_or_default();
                validate_seed_name(&name, row + 1)?;
                ImportedSeed { seed, name }
            }
            _ => {
                return Err(format!(
                    "Seed JSON row {} must be a seed or object",
                    row + 1
                ));
            }
        };
        insert_seed(entry, row + 1, &mut seen, &mut out)?;
    }
    Ok(out)
}

fn parse_seed_lines(input: &str, cancel: impl Fn() -> bool) -> Result<Vec<ImportedSeed>, String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut csv = false;
    let mut first_data = true;
    for (index, raw) in input.lines().enumerate() {
        if cancel() {
            return Err("Cancelled".into());
        }
        let line_no = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = parse_csv_row(line).map_err(|e| format!("Seed list line {line_no}: {e}"))?;
        if first_data {
            first_data = false;
            if fields.len() == 2
                && fields[0].trim().eq_ignore_ascii_case("seed")
                && fields[1].trim().eq_ignore_ascii_case("name")
            {
                csv = true;
                continue;
            }
        }
        let (seed_text, name) = if csv || fields.len() == 2 {
            if fields.len() != 2 {
                return Err(format!("Seed list line {line_no}: expected seed,name"));
            }
            (fields[0].trim(), fields[1].trim().to_owned())
        } else if fields.len() == 1 {
            (fields[0].trim(), String::new())
        } else {
            return Err(format!(
                "Seed list line {line_no}: expected one seed or seed,name"
            ));
        };
        let seed = parse_i64(seed_text, line_no)?;
        validate_seed_name(&name, line_no)?;
        insert_seed(ImportedSeed { seed, name }, line_no, &mut seen, &mut out)?;
    }
    if out.is_empty() {
        return Err("Seed list contains no seeds".into());
    }
    Ok(out)
}

fn parse_csv_row(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
            }
        } else if ch == ',' {
            fields.push(std::mem::take(&mut field));
            closed = false;
        } else if ch == '"' && field.trim().is_empty() && !closed {
            field.clear();
            quoted = true;
        } else if closed && !ch.is_whitespace() {
            return Err("unexpected text after quoted field".into());
        } else {
            field.push(ch);
        }
    }
    if quoted {
        return Err("unterminated quoted field".into());
    }
    fields.push(field);
    Ok(fields)
}

fn parse_i64(text: &str, row: usize) -> Result<i64, String> {
    text.trim()
        .parse::<i64>()
        .map_err(|_| format!("Seed list row {row} has invalid signed 64-bit seed"))
}
fn validate_seed_name(name: &str, row: usize) -> Result<(), String> {
    if name.chars().count() > 128 {
        return Err(format!("Seed list row {row} name exceeds 128 characters"));
    }
    if name.chars().any(char::is_control) {
        return Err(format!(
            "Seed list row {row} name contains control characters"
        ));
    }
    Ok(())
}
fn insert_seed(
    seed: ImportedSeed,
    row: usize,
    seen: &mut std::collections::HashSet<i64>,
    out: &mut Vec<ImportedSeed>,
) -> Result<(), String> {
    if seen.insert(seed.seed) {
        if out.len() == MAX_IMPORTED_SEEDS {
            return Err(format!(
                "Seed list exceeds {MAX_IMPORTED_SEEDS} unique seeds"
            ));
        }
        let name = if seed.name.is_empty() {
            seed.seed.to_string()
        } else {
            seed.name
        };
        out.push(ImportedSeed {
            seed: seed.seed,
            name,
        });
    } else if row == 0 {
        unreachable!();
    }
    Ok(())
}
fn export_map(
    world: &World,
    b: Bounds,
    layer: Layer,
    w: u32,
    h: u32,
    path: &PathBuf,
    display: &crate::display::DisplayPrefs,
    cancel: impl Fn() -> bool,
) -> Result<Outcome, String> {
    let image = render_map(world, b, layer, w, h, display, &cancel)?;
    write_png(path, image, &cancel)?;
    Ok(Outcome {
        path: path.clone(),
        note: format!("{w} × {h} · {}", layer.label()),
        data: None,
    })
}

fn export_composed_map(
    world: &World,
    bounds: Bounds,
    layer: Layer,
    width: u32,
    height: u32,
    path: &PathBuf,
    markers: &[Marker],
    composition: &Composition,
    display: &crate::display::DisplayPrefs,
    cancel: impl Fn() -> bool,
) -> Result<Outcome, String> {
    let mut image = render_map(world, bounds, layer, width, height, display, &cancel)?;
    compose_map(&mut image, bounds, layer, markers, composition, &cancel)?;
    if cancel() {
        return Err("Export cancelled".into());
    }
    write_png(path, image, &cancel)?;
    Ok(Outcome {
        path: path.clone(),
        note: format!("{width} × {height} · {} · composed", layer.label()),
        data: None,
    })
}

fn validate_map(bounds: Bounds, width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(format!(
            "Map dimensions must be at most {MAX_SIDE} × {MAX_SIDE} pixels"
        ));
    }
    if ![bounds.min_x, bounds.min_z, bounds.max_x, bounds.max_z]
        .iter()
        .all(|v| v.is_finite())
        || bounds.min_x >= bounds.max_x
        || bounds.min_z >= bounds.max_z
        || bounds.min_x < -WORLD_BORDER
        || bounds.min_z < -WORLD_BORDER
        || bounds.max_x > WORLD_BORDER
        || bounds.max_z > WORLD_BORDER
    {
        return Err("Map bounds are invalid or outside the Minecraft world border".into());
    }
    Ok(())
}

fn render_map(
    world: &World,
    bounds: Bounds,
    layer: Layer,
    width: u32,
    height: u32,
    display: &crate::display::DisplayPrefs,
    cancel: &impl Fn() -> bool,
) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, String> {
    validate_map(bounds, width, height)?;
    let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(width, height);
    let dx = (bounds.max_x - bounds.min_x) / f64::from(width);
    let dz = (bounds.max_z - bounds.min_z) / f64::from(height);
    for tz in (0..height).step_by(64) {
        for tx in (0..width).step_by(64) {
            for py in tz..(tz + 64).min(height) {
                if cancel() {
                    return Err("Export cancelled".into());
                }
                let z = (bounds.min_z + (f64::from(py) + 0.5) * dz).floor() as i32;
                for px in tx..(tx + 64).min(width) {
                    let x = (bounds.min_x + (f64::from(px) + 0.5) * dx).floor() as i32;
                    img.put_pixel(
                        px,
                        py,
                        Rgba(atlas::raster_color_with_display(
                            world, layer, x, z, display,
                        )),
                    );
                }
            }
        }
    }
    Ok(img)
}

fn write_png(
    path: &PathBuf,
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    cancel: &impl Fn() -> bool,
) -> Result<(), String> {
    if cancel() {
        return Err("Export cancelled".into());
    }
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    atomic_write(path, &png, cancel)
}

fn compose_map(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    bounds: Bounds,
    layer: Layer,
    markers: &[Marker],
    composition: &Composition,
    cancel: &impl Fn() -> bool,
) -> Result<(), String> {
    let width = image.width();
    let height = image.height();
    for (i, marker) in markers.iter().enumerate() {
        if i % 128 == 0 && cancel() {
            return Err("Export cancelled".into());
        }
        if !marker.x.is_finite() || !marker.z.is_finite() {
            continue;
        }
        let center_visible = bounds.contains(marker.x, marker.z);
        if let Some(bb) = marker
            .bounds
            .filter(|_| matches!(&marker.kind, crate::model::MarkerKind::Structure))
            .filter(|bb| {
                bb.max_x as f64 >= bounds.min_x
                    && bb.min_x as f64 <= bounds.max_x
                    && bb.max_z as f64 >= bounds.min_z
                    && bb.min_z as f64 <= bounds.max_z
            })
        {
            draw_structure_bounds(image, bounds, bb, [255, 204, 76]);
        }
        if !center_visible {
            continue;
        }
        let px = project_x(marker.x, bounds, width);
        let py = project_z(marker.z, bounds, height);
        if matches!(
            &marker.kind,
            crate::model::MarkerKind::Structure
                | crate::model::MarkerKind::Spawn
                | crate::model::MarkerKind::Player
        ) {
            let icon = crate::structure_icons::raster(&marker.id, 24);
            composite_center(image, &icon, px as i32, py as i32);
            continue;
        }
        if matches!(&marker.kind, crate::model::MarkerKind::Ore) {
            if let Some(bytes) = crate::ore_icons::icon_png(&marker.id) {
                if let Ok(icon) = image::load_from_memory(bytes) {
                    let icon = image::imageops::resize(
                        &icon.to_rgba8(),
                        20,
                        20,
                        image::imageops::FilterType::Nearest,
                    );
                    composite_center(image, &icon, px as i32, py as i32);
                    continue;
                }
            }
        }
        let color = match &marker.kind {
            crate::model::MarkerKind::Structure => [255, 170, 55, 255],
            crate::model::MarkerKind::Spawn => [255, 220, 112, 255],
            crate::model::MarkerKind::Player => [137, 207, 255, 255],
            crate::model::MarkerKind::Tree => [130, 255, 120, 255],
            crate::model::MarkerKind::Forest => [53, 216, 139, 255],
            crate::model::MarkerKind::Rock => [237, 208, 155, 255],
            crate::model::MarkerKind::Biome => [255, 255, 255, 255],
            crate::model::MarkerKind::Ore => [255, 255, 255, 255],
        };
        draw_diamond(image, px as i32, py as i32, color);
    }

    draw_badge(image, 8, 8, layer.label().to_ascii_uppercase().as_str());
    if composition.coordinates {
        let labels = [
            (
                format!("X{} Z{}", coord(bounds.min_x), coord(bounds.min_z)),
                8,
                26,
            ),
            (
                format!("X{} Z{}", coord(bounds.max_x), coord(bounds.min_z)),
                (width as i32 - 100).max(8),
                26,
            ),
            (
                format!("X{} Z{}", coord(bounds.min_x), coord(bounds.max_z)),
                8,
                (height as i32 - 20).max(8),
            ),
            (
                format!("X{} Z{}", coord(bounds.max_x), coord(bounds.max_z)),
                (width as i32 - 100).max(8),
                (height as i32 - 20).max(8),
            ),
        ];
        for (text, x, y) in labels {
            draw_badge(image, x, y, &text);
        }
    }
    if composition.scale_bar {
        draw_scale_bar(image, bounds);
    }
    if !composition.legend.is_empty() || composition.scale.is_some() {
        draw_legend(image, &composition.legend, composition.scale.as_ref());
    }
    Ok(())
}

fn coord(value: f64) -> String {
    format!("{:.0}", value)
}
fn project_x(x: f64, bounds: Bounds, width: u32) -> u32 {
    (((x - bounds.min_x) / (bounds.max_x - bounds.min_x)) * f64::from(width))
        .floor()
        .clamp(0., f64::from(width.saturating_sub(1))) as u32
}
fn project_z(z: f64, bounds: Bounds, height: u32) -> u32 {
    (((z - bounds.min_z) / (bounds.max_z - bounds.min_z)) * f64::from(height))
        .floor()
        .clamp(0., f64::from(height.saturating_sub(1))) as u32
}
fn draw_structure_bounds(
    img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    map: Bounds,
    bb: crate::model::StructureBounds,
    color: [u8; 3],
) {
    let min_x = project_x(bb.min_x as f64, map, img.width()) as i32;
    let max_x = project_x(bb.max_x as f64, map, img.width()) as i32;
    let min_z = project_z(bb.min_z as f64, map, img.height()) as i32;
    let max_z = project_z(bb.max_z as f64, map, img.height()) as i32;
    let rgba = [color[0], color[1], color[2], 255];
    draw_line(img, min_x, min_z, max_x, min_z, rgba);
    draw_line(img, max_x, min_z, max_x, max_z, rgba);
    draw_line(img, max_x, max_z, min_x, max_z, rgba);
    draw_line(img, min_x, max_z, min_x, min_z, rgba);
}
fn draw_diamond(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, x: i32, y: i32, color: [u8; 4]) {
    for dy in -4i32..=4 {
        let span = 4 - dy.abs();
        for dx in -span..=span {
            put(img, x + dx, y + dy, color);
        }
    }
}
fn draw_line(
    img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    color: [u8; 4],
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        put(img, x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * error;
        if e2 >= dy {
            error += dy;
            x0 += sx;
        }
        if e2 <= dx {
            error += dx;
            y0 += sy;
        }
    }
}
fn put(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, x: i32, y: i32, c: [u8; 4]) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        let p = img.get_pixel_mut(x as u32, y as u32);
        let a = c[3] as u32;
        for k in 0..3 {
            p[k] = ((c[k] as u32 * a + p[k] as u32 * (255 - a)) / 255) as u8;
        }
        p[3] = 255;
    }
}
fn composite_center(
    img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    icon: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
) {
    let ox = cx - icon.width() as i32 / 2;
    let oy = cy - icon.height() as i32 / 2;
    for y in 0..icon.height() {
        for x in 0..icon.width() {
            let p = *icon.get_pixel(x, y);
            if p[3] > 0 {
                let xx = ox + x as i32;
                let yy = oy + y as i32;
                if xx >= 0 && yy >= 0 && (xx as u32) < img.width() && (yy as u32) < img.height() {
                    image::imageops::overlay(img, icon, ox as i64, oy as i64);
                    return;
                }
            }
        }
    }
}
fn draw_badge(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, x: i32, y: i32, text: &str) {
    let width = (text_width(text, 12.0).ceil() as i32 + 8).min(img.width() as i32);
    for yy in 0..14 {
        for xx in 0..width {
            put(img, x + xx, y + yy, [7, 13, 20, 220]);
        }
    }
    draw_text(img, x + 4, y + 1, text, [245, 248, 252, 255]);
}
fn text_width(text: &str, size: f32) -> f32 {
    let font = EXPORT_FONT.get_or_init(|| {
        FontArc::try_from_slice(include_bytes!("../assets/export-font.ttf"))
            .expect("bundled export font is valid")
    });
    let scaled = font.as_scaled(PxScale::from(size));
    text.chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)))
        .sum()
}
fn draw_text(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, x: i32, y: i32, text: &str, color: [u8; 4]) {
    let font = EXPORT_FONT.get_or_init(|| {
        FontArc::try_from_slice(include_bytes!("../assets/export-font.ttf"))
            .expect("bundled export font is valid")
    });
    let scaled = font.as_scaled(PxScale::from(12.0));
    let mut cursor = x as f32;
    let baseline = y as f32 + scaled.ascent();
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        let id = if id.0 == 0 { scaled.glyph_id('?') } else { id };
        let glyph = id.with_scale_and_position(PxScale::from(12.0), point(cursor, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let alpha = (coverage * color[3] as f32).round().clamp(0.0, 255.0) as u8;
                put(
                    img,
                    bounds.min.x.floor() as i32 + gx as i32,
                    bounds.min.y.floor() as i32 + gy as i32,
                    [color[0], color[1], color[2], alpha],
                );
            });
        }
        cursor += scaled.h_advance(id);
    }
}
fn draw_scale_bar(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, bounds: Bounds) {
    let span = (bounds.max_x - bounds.min_x).abs();
    if span <= 0. {
        return;
    }
    let target = span * 0.2;
    let power = 10_f64.powf(target.log10().floor());
    let normalized = target / power;
    let factor = if normalized >= 5. {
        5.
    } else if normalized >= 2. {
        2.
    } else {
        1.
    };
    let blocks = factor * power;
    let px = (blocks / span * img.width() as f64)
        .round()
        .clamp(12., (img.width() as f64 * 0.4).max(12.)) as i32;
    let x = 12;
    let y = img.height() as i32 - 43;
    for yy in y - 13..=y + 4 {
        for xx in x - 4..=x + px + 4 {
            put(img, xx, yy, [7, 13, 20, 220]);
        }
    }
    draw_line(img, x, y, x + px, y, [255, 255, 255, 255]);
    draw_line(img, x, y - 2, x, y + 2, [255, 255, 255, 255]);
    draw_line(img, x + px, y - 2, x + px, y + 2, [255, 255, 255, 255]);
    draw_text(
        img,
        x,
        y - 12,
        &format!("{blocks:.0} BLOCKS"),
        [245, 248, 252, 255],
    );
}
fn draw_legend(
    img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    entries: &[LegendChip],
    scale: Option<&ScaleLegend>,
) {
    let narrow = img.width() < 240;
    let mut y = if narrow { 22i32 } else { 8i32 };
    let x = if narrow {
        8
    } else {
        (img.width() as i32 - 184).max(8)
    };
    if let Some(scale) = scale {
        let width = 128i32;
        let height = 6;
        let colors = if scale.stops.is_empty() {
            vec![[210, 210, 210], [40, 70, 210]]
        } else {
            scale.stops.clone()
        };
        for px in 0..width {
            let t = px as f32 / (width - 1) as f32;
            let k = (t * (colors.len() - 1) as f32).floor() as usize;
            let n = (k + 1).min(colors.len() - 1);
            let f = t * (colors.len() - 1) as f32 - k as f32;
            let c = [
                (colors[k][0] as f32 * (1. - f) + colors[n][0] as f32 * f) as u8,
                (colors[k][1] as f32 * (1. - f) + colors[n][1] as f32 * f) as u8,
                (colors[k][2] as f32 * (1. - f) + colors[n][2] as f32 * f) as u8,
                255,
            ];
            for py in 0..height {
                put(img, x + px, y + 11 + py, c);
            }
        }
        draw_text(img, x, y, &scale.low, [245, 248, 252, 255]);
        let right = format!("{}", scale.high);
        let right_x = x + width - text_width(&right, 12.0).ceil() as i32;
        draw_text(img, right_x, y, &right, [245, 248, 252, 255]);
        return;
    }
    for e in entries.iter().take(5) {
        let label = match e.percent {
            Some(p) => format!("{} {:.0}%", e.label, p),
            None => e.label.clone(),
        };
        let width = (text_width(&label, 12.0).ceil() as i32 + 22).min(176);
        for yy in y..y + 11 {
            for xx in x..x + width {
                put(img, xx, yy, [7, 13, 20, 220]);
            }
        }
        for yy in y + 3..y + 8 {
            for xx in x + 3..x + 8 {
                put(img, xx, yy, [e.color[0], e.color[1], e.color[2], 255]);
            }
        }
        draw_text(img, x + 11, y + 2, &label, [245, 248, 252, 255]);
        y += 12;
    }
}

fn export_markers(
    ms: &[Marker],
    path: &PathBuf,
    cancel: impl Fn() -> bool,
) -> Result<Outcome, String> {
    let mut csv = String::from(
        "name,kind,namespace,id,x,z,y,confirmed,detail,bbox_min_x,bbox_min_y,bbox_min_z,bbox_max_x,bbox_max_y,bbox_max_z\r\n",
    );
    for m in ms {
        if cancel() {
            return Err("Export cancelled".into());
        }
        let kind = match &m.kind {
            crate::model::MarkerKind::Structure => "structure",
            crate::model::MarkerKind::Spawn => "spawn",
            crate::model::MarkerKind::Player => "player",
            crate::model::MarkerKind::Ore => "ore",
            crate::model::MarkerKind::Biome => "biome",
            crate::model::MarkerKind::Rock => "rock",
            crate::model::MarkerKind::Forest => "forest",
            crate::model::MarkerKind::Tree => "tree",
        };
        let bb = m.bounds;
        let fields = [
            m.label.clone(),
            kind.into(),
            m.namespace.clone(),
            m.id.clone(),
            m.x.to_string(),
            m.z.to_string(),
            m.y.map(|v| v.to_string()).unwrap_or_default(),
            m.confirmed.to_string(),
            m.detail.clone(),
            bb.map(|b| b.min_x.to_string()).unwrap_or_default(),
            bb.map(|b| b.min_y.to_string()).unwrap_or_default(),
            bb.map(|b| b.min_z.to_string()).unwrap_or_default(),
            bb.map(|b| b.max_x.to_string()).unwrap_or_default(),
            bb.map(|b| b.max_y.to_string()).unwrap_or_default(),
            bb.map(|b| b.max_z.to_string()).unwrap_or_default(),
        ];
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                csv.push(',')
            }
            if matches!(i, 4..=7 | 9..=14) {
                csv.push_str(&format!("\"{}\"", f.replace('"', "\"\"")));
            } else {
                csv.push_str(&csv_field(f));
            }
        }
        csv.push_str("\r\n")
    }
    atomic_write(path, csv.as_bytes(), &cancel)?;
    Ok(Outcome {
        path: path.clone(),
        note: format!("{} markers · CSV", ms.len()),
        data: None,
    })
}
fn csv_field(s: &str) -> String {
    let t = s.trim_start_matches([' ', '\t', '\r', '\n']);
    let safe = if t.starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}
fn atomic_write(path: &PathBuf, bytes: &[u8], cancel: &impl Fn() -> bool) -> Result<(), String> {
    if cancel() {
        return Err("Export cancelled".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("Could not create export directory: {e}"))?;
    let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(".terralens-{}-{id}.partial", std::process::id()));
    let res = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("Could not create temporary export: {e}"))?;
        for chunk in bytes.chunks(1024 * 1024) {
            if cancel() {
                return Err("Export cancelled".into());
            }
            f.write_all(chunk)
                .map_err(|e| format!("Could not write export: {e}"))?
        }
        f.sync_all()
            .map_err(|e| format!("Could not finish export: {e}"))?;
        drop(f);
        if cancel() {
            return Err("Export cancelled".into());
        }
        fs::rename(&tmp, path).map_err(|e| format!("Could not save export: {e}"))?;
        Ok(())
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}
/// Copyable Xaero Minimap 26.6.0 chat payload (`xaero-waypoint:` prefix).
/// The installed handler parses this as name:initials:x:y:z:color:rotation:yaw:destination.
pub fn xaero_share(name: &str, p: Point, y: i32, color: u8) -> String {
    let clean: String = name
        .chars()
        .filter(|c| !matches!(c, ':' | '\n' | '\r' | '\0'))
        .take(32)
        .collect();
    let initials: String = clean
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into());
    let x = if p.x.is_finite() {
        p.x.round().clamp(-WORLD_BORDER, WORLD_BORDER) as i32
    } else {
        0
    };
    let z = if p.z.is_finite() {
        p.z.round().clamp(-WORLD_BORDER, WORLD_BORDER) as i32
    } else {
        0
    };
    format!(
        "xaero-waypoint:{clean}:{initials}:{x}:{y}:{z}:{}:false:0:External",
        color.min(20)
    )
}

/// Xaero Minimap 26.6.0 row, verified against this instance's waypoints.txt schema.
pub fn xaero_waypoint(name: &str, p: Point, y: i32, color: u8) -> String {
    let clean: String = name
        .chars()
        .filter(|c| !matches!(c, ':' | '\n' | '\r' | '\0'))
        .take(48)
        .collect();
    let initials: String = clean
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into());
    let x = if p.x.is_finite() {
        p.x.round().clamp(-WORLD_BORDER, WORLD_BORDER) as i32
    } else {
        0
    };
    let z = if p.z.is_finite() {
        p.z.round().clamp(-WORLD_BORDER, WORLD_BORDER) as i32
    } else {
        0
    };
    format!(
        "waypoint:{clean}:{initials}:{x}:{y}:{z}:{}:false:0:gui.xaero_default:false:0:0:false",
        color.min(20)
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xaero_safe_negatives() {
        assert_eq!(
            xaero_waypoint("Copper: west\nshaft", Point { x: -12.6, z: -34.4 }, 67, 20),
            "waypoint:Copper westshaft:C:-13:67:-34:20:false:0:gui.xaero_default:false:0:0:false"
        )
    }
    #[test]
    fn xaero_share_matches_installed_chat_import_format() {
        assert_eq!(
            xaero_share("Copper: west\nshaft", Point { x: -12.6, z: -34.4 }, 67, 20),
            "xaero-waypoint:Copper westshaft:C:-13:67:-34:20:false:0:External"
        );
        assert_eq!(
            xaero_share("Gold", Point::default(), 64, 6)
                .split(':')
                .count(),
            10
        );
    }
    #[test]
    fn cancelled_marker_export_never_replaces_target() {
        let path =
            std::env::temp_dir().join(format!("terralens-cancel-{}.csv", std::process::id()));
        std::fs::write(&path, b"keep me").unwrap();
        let result = export_markers(&[], &path, || true);
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn csv_quotes_and_formula_safety() {
        assert_eq!(csv_field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_field("ordinary, text"), "\"ordinary, text\"")
    }
    #[test]
    fn csv_export_preserves_negative_coordinates() {
        let path =
            std::env::temp_dir().join(format!("terralens-coords-{}.csv", std::process::id()));
        let marker = Marker {
            id: "tfg:ruin".into(),
            label: "Ruined tower".into(),
            namespace: "tfg".into(),
            x: -1234.0,
            z: -987.0,
            y: Some(72.0),
            kind: crate::model::MarkerKind::Structure,
            confirmed: true,
            min_y: None,
            max_y: None,
            detail: "north ridge".into(),
            color: [1, 2, 3],
            bounds: Some(crate::model::StructureBounds {
                min_x: -1240,
                min_y: 65,
                min_z: -990,
                max_x: -1220,
                max_y: 90,
                max_z: -970,
            }),
        };
        export_markers(&[marker], &path, || false).unwrap();
        let csv = std::fs::read_to_string(&path).unwrap();
        assert!(csv.contains("\"-1234\",\"-987\",\"72\""));
        assert!(csv.contains("\"-1240\",\"65\",\"-990\",\"-1220\",\"90\",\"-970\""));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn composed_map_uses_distinct_spawn_and_player_icons() {
        let mut image = ImageBuffer::from_pixel(100, 100, Rgba([20, 40, 60, 255]));
        let marker = |id: &str, label: &str, x, z, kind| Marker {
            id: id.into(),
            label: label.into(),
            namespace: "minecraft".into(),
            x,
            z,
            y: Some(64.),
            kind,
            confirmed: true,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [255; 3],
            bounds: None,
        };
        let bounds = Bounds {
            min_x: 0.,
            min_z: 0.,
            max_x: 100.,
            max_z: 100.,
        };
        let markers = [
            marker(
                "minecraft:world_spawn",
                "World spawn",
                25.,
                25.,
                crate::model::MarkerKind::Spawn,
            ),
            marker(
                "minecraft:player/abc",
                "Player",
                75.,
                75.,
                crate::model::MarkerKind::Player,
            ),
        ];
        compose_map(
            &mut image,
            bounds,
            Layer::Biomes,
            &markers,
            &Composition::default(),
            &|| false,
        )
        .unwrap();
        let spawn = image.get_pixel(25, 25).0;
        let player = image.get_pixel(75, 78).0;
        assert!(spawn[0] > 245 && spawn[1] > 235 && spawn[2] > 175);
        assert!(player[0] < 160 && player[1] > 185 && player[2] > 235);
        assert_ne!(spawn, player);
    }
    #[test]
    fn cancelled_png_export_keeps_existing_target() {
        let world = World::new(1, crate::model::Settings::default());
        let path =
            std::env::temp_dir().join(format!("terralens-cancel-{}.png", std::process::id()));
        std::fs::write(&path, b"keep me").unwrap();
        let request = Request::Map {
            bounds: Bounds {
                min_x: 0.0,
                min_z: 0.0,
                max_x: 16.0,
                max_z: 16.0,
            },
            layer: Layer::Biomes,
            width: 2,
            height: 2,
            path: path.clone(),
        };
        assert!(execute(&world, &request, || true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn composed_png_contains_markers_bounds_and_clips_off_map_features() {
        let world = World::new(1, crate::model::Settings::default());
        let dir = std::env::temp_dir();
        let path = dir.join(format!("terralens-composed-{}.png", std::process::id()));
        let raster_path = dir.join(format!("terralens-raster-{}.png", std::process::id()));
        let bounds = Bounds {
            min_x: 0.,
            min_z: 0.,
            max_x: 100.,
            max_z: 100.,
        };
        let marker = |id: &str, x: f64, z: f64, kind, color| Marker {
            id: id.into(),
            label: id.into(),
            namespace: "tfg".into(),
            x,
            z,
            y: Some(64.),
            kind,
            confirmed: true,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color,
            bounds: None,
        };
        let mut structure = marker(
            "tower",
            50.,
            50.,
            crate::model::MarkerKind::Structure,
            [255, 170, 55],
        );
        structure.bounds = Some(crate::model::StructureBounds {
            min_x: 30,
            min_y: 20,
            min_z: 30,
            max_x: 70,
            max_y: 90,
            max_z: 70,
        });
        let markers = vec![
            structure,
            marker(
                "beneath_realgar@0,0",
                80.,
                80.,
                crate::model::MarkerKind::Ore,
                [255, 255, 255],
            ),
            marker(
                "off-map",
                150.,
                50.,
                crate::model::MarkerKind::Tree,
                [7, 239, 244],
            ),
        ];
        let request = Request::ComposedMap {
            bounds,
            layer: Layer::Biomes,
            width: 100,
            height: 100,
            path: path.clone(),
            markers,
            composition: Composition {
                coordinates: false,
                scale_bar: false,
                legend: Vec::new(),
                scale: None,
            },
        };
        execute(&world, &request, || false).unwrap();
        execute(
            &world,
            &Request::Map {
                bounds,
                layer: Layer::Biomes,
                width: 100,
                height: 100,
                path: raster_path.clone(),
            },
            || false,
        )
        .unwrap();
        let composed = image::open(&path).unwrap().to_rgba8();
        let raster = image::open(&raster_path).unwrap().to_rgba8();
        assert_eq!(composed.dimensions(), (100, 100));
        assert_eq!(composed.get_pixel(30, 30).0, [255, 204, 76, 255]);
        assert_ne!(composed.get_pixel(50, 50), raster.get_pixel(50, 50));
        assert!(!composed.pixels().any(|p| p.0 == [7, 239, 244, 255]));
        assert!(
            (70..90).any(|y| (70..90).any(|x| composed.get_pixel(x, y) != raster.get_pixel(x, y)))
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(raster_path).unwrap();
    }
    #[test]
    fn numeric_scale_alone_draws_compact_antialiased_unicode_key() {
        let mut image = ImageBuffer::from_pixel(260, 80, Rgba([0, 0, 0, 255]));
        let scale = ScaleLegend {
            min: -25.0,
            max: 35.0,
            unit: "°C".into(),
            low: "−25 °C".into(),
            high: "+35 °C".into(),
            stops: vec![[40, 80, 160], [240, 150, 50]],
        };
        draw_legend(&mut image, &[], Some(&scale));
        assert!(image.pixels().filter(|p| p.0 != [0, 0, 0, 255]).count() > 128);
        let font = EXPORT_FONT.get().expect("draw_legend initialized font");
        assert_ne!(
            font.glyph_id('°').0,
            0,
            "bundled font must draw degree values"
        );
    }
    #[test]
    fn cancelled_composed_png_does_not_replace_target() {
        let world = World::new(1, crate::model::Settings::default());
        let path = std::env::temp_dir().join(format!(
            "terralens-composed-cancel-{}.png",
            std::process::id()
        ));
        std::fs::write(&path, b"keep me").unwrap();
        let request = Request::ComposedMap {
            bounds: Bounds {
                min_x: 0.,
                min_z: 0.,
                max_x: 16.,
                max_z: 16.,
            },
            layer: Layer::Biomes,
            width: 2,
            height: 2,
            path: path.clone(),
            markers: Vec::new(),
            composition: Composition::default(),
        };
        assert!(execute(&world, &request, || true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn session_export_and_import_round_trip_without_touching_world_files() {
        let dir =
            std::env::temp_dir().join(format!("terralens-session-test-{}", std::process::id()));
        let path = dir.join("atlas.json");
        let w = World::new(1, crate::model::Settings::default());
        let value = serde_json::json!({"seed":"7787531943268589495","spot":{"x":-1024,"z":512}});
        execute(
            &w,
            &Request::Data {
                value: value.clone(),
                path: path.clone(),
            },
            || false,
        )
        .unwrap();
        let read = execute(&w, &Request::ReadSession { path: path.clone() }, || false).unwrap();
        assert_eq!(read.data, Some(value));
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn imports_plain_seed_lines_comments_and_full_signed_range() {
        let seeds = parse_seed_list("# candidates\n-9223372036854775808\n0\n9223372036854775807\n")
            .unwrap();
        assert_eq!(
            seeds.iter().map(|s| s.seed).collect::<Vec<_>>(),
            [i64::MIN, 0, i64::MAX]
        );
        assert_eq!(seeds[0].name, i64::MIN.to_string());
    }

    #[test]
    fn imports_csv_header_and_escaped_quoted_names() {
        let seeds =
            parse_seed_list("seed,name\n42,\"A, brave \"\"new\"\" world\"\n-8,Alpha\n").unwrap();
        assert_eq!(
            seeds,
            [
                ImportedSeed {
                    seed: 42,
                    name: "A, brave \"new\" world".into()
                },
                ImportedSeed {
                    seed: -8,
                    name: "Alpha".into()
                },
            ]
        );
    }

    #[test]
    fn imports_json_numbers_decimal_strings_and_objects_without_float_rounding() {
        let seeds = parse_seed_list(
            r#"[9223372036854775807,"-9223372036854775808",{"seed":"42","name":"named"}]"#,
        )
        .unwrap();
        assert_eq!(seeds[0].seed, i64::MAX);
        assert_eq!(seeds[1].seed, i64::MIN);
        assert_eq!(seeds[2].name, "named");
    }

    #[test]
    fn malformed_rows_and_overflow_reject_the_whole_list() {
        assert!(
            parse_seed_list("1\nnot-a-seed\n")
                .unwrap_err()
                .contains("row 2")
        );
        assert!(parse_seed_list("9223372036854775808").is_err());
        assert!(parse_seed_list("seed,name\n1,\"unterminated\n").is_err());
    }

    #[test]
    fn unique_seed_limit_and_cancellation_are_enforced() {
        let rows = (0..129)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_seed_list(&rows).unwrap_err().contains("128 unique"));
        let calls = std::cell::Cell::new(0);
        let result = parse_seed_list_cancellable("1\n2\n3", || {
            calls.set(calls.get() + 1);
            calls.get() >= 3
        });
        assert_eq!(result.unwrap_err(), "Cancelled");
    }

    #[test]
    fn read_seed_request_returns_data_and_rejects_oversized_input() {
        let dir =
            std::env::temp_dir().join(format!("terralens-seed-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("seeds.txt");
        std::fs::write(&path, "1\n2\n").unwrap();
        let world = World::new(1, crate::model::Settings::default());
        let outcome =
            execute(&world, &Request::ReadSeeds { path: path.clone() }, || false).unwrap();
        assert_eq!(
            outcome.data.unwrap()["seedList"].as_array().unwrap().len(),
            2
        );
        std::fs::write(&path, vec![b'1'; MAX_SEED_FILE as usize + 1]).unwrap();
        assert!(
            execute(&world, &Request::ReadSeeds { path: path.clone() }, || false)
                .unwrap_err()
                .contains("1 MiB")
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn map_rejects_bad_dimensions_and_bounds() {
        let world = World::new(1, crate::model::Settings::default());
        let path = std::env::temp_dir().join("terralens-never-written.png");
        for (bounds, width) in [
            (
                Bounds {
                    min_x: 0.,
                    min_z: 0.,
                    max_x: 1.,
                    max_z: 1.,
                },
                5000,
            ),
            (
                Bounds {
                    min_x: f64::NAN,
                    min_z: 0.,
                    max_x: 1.,
                    max_z: 1.,
                },
                1,
            ),
        ] {
            let r = Request::Map {
                bounds,
                layer: Layer::Biomes,
                width,
                height: 1,
                path: path.clone(),
            };
            assert!(execute(&world, &r, || false).is_err())
        }
    }
}
