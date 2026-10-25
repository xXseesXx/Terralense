//! Nonblocking Minecraft account-skin heads for saved player markers.
//!
//! Call [`request`] while drawing a player marker, then poll [`cached`] and
//! [`revision`] from the UI thread. All disk and network work runs on one
//! bounded worker. The only network destinations are Mojang's session server
//! and `textures.minecraft.net`; no account credentials are read or sent.

use crossbeam_channel::{Receiver, Sender, bounded};
use image::{ImageReader, Rgba, RgbaImage, imageops};
use reqwest::blocking::{Client, Response};
use reqwest::redirect::Policy;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const MAX_JSON_BYTES: usize = 64 * 1024;
const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const FAILURE_RETRY: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct State {
    heads: HashMap<String, RgbaImage>,
    revisions: HashMap<String, u64>,
    queued: HashSet<String>,
    refresh_at: HashMap<String, Instant>,
    retry_at: HashMap<String, Instant>,
}

struct Service {
    tx: Sender<String>,
    state: Mutex<State>,
    worker_available: bool,
}

static SERVICE: OnceLock<Service> = OnceLock::new();

/// Ask the background worker to load or fetch a player's account skin.
/// Repeated requests for the same UUID are coalesced. Invalid IDs are ignored.
pub fn request(uuid: &str) {
    let Some(uuid) = normalize_uuid(uuid) else {
        return;
    };
    let service = service();
    if !service.worker_available {
        return;
    }
    let mut state = service.state.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    if state.queued.contains(&uuid)
        || state.refresh_at.get(&uuid).is_some_and(|at| *at > now)
        || state.retry_at.get(&uuid).is_some_and(|at| *at > now)
    {
        return;
    }
    state.queued.insert(uuid.clone());
    if service.tx.try_send(uuid.clone()).is_err() {
        state.queued.remove(&uuid);
    }
}

/// Return the current skin face, if already available in memory. Never reads
/// disk, starts a request, or waits for the worker.
pub fn cached(uuid: &str) -> Option<RgbaImage> {
    let uuid = normalize_uuid(uuid)?;
    SERVICE
        .get()?
        .state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .heads
        .get(&uuid)
        .cloned()
}

/// Monotonically increasing per-UUID value, changed when a head first becomes
/// available. Use it to refresh a GPU texture without keying heads by marker
/// family. Unknown and not-yet-loaded IDs return zero.
pub fn revision(uuid: &str) -> u64 {
    let Some(uuid) = normalize_uuid(uuid) else {
        return 0;
    };
    let Some(service) = SERVICE.get() else {
        return 0;
    };
    service
        .state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .revisions
        .get(&uuid)
        .copied()
        .unwrap_or(0)
}

/// Whether this UUID is currently queued or being processed by the worker.
/// This is a memory-only check intended for short repaint scheduling.
pub fn pending(uuid: &str) -> bool {
    let Some(uuid) = normalize_uuid(uuid) else {
        return false;
    };
    SERVICE.get().is_some_and(|service| {
        service
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .queued
            .contains(&uuid)
    })
}

/// Read a previously cached account head for an offline export. This performs
/// disk I/O and must only be called from a worker, never from the GUI thread.
pub fn export_head(uuid: &str, size: u32) -> Option<RgbaImage> {
    let uuid = normalize_uuid(uuid)?;
    let bytes = fs::read(head_cache_path(&cache_root(), &uuid)).ok()?;
    let head = decode_cached_head(&bytes)?;
    (size == 0).then_some(head.clone()).or_else(|| {
        Some(imageops::resize(
            &head,
            size,
            size,
            imageops::FilterType::Nearest,
        ))
    })
}

/// A pixel-art Steve-style head used until the account skin arrives or if the
/// account has no public skin. Nearest-neighbor scaling keeps the sprite crisp.
pub fn fallback_head(size: u32) -> RgbaImage {
    let mut head = RgbaImage::from_pixel(8, 8, Rgba([151, 105, 74, 255]));
    let hair = Rgba([91, 58, 33, 255]);
    let skin = Rgba([198, 145, 105, 255]);
    let eye_white = Rgba([235, 231, 219, 255]);
    let eye = Rgba([53, 118, 150, 255]);
    // Hairline and temples.
    for x in 0..8 {
        head.put_pixel(x, 0, hair);
        head.put_pixel(x, 1, if x == 0 || x == 7 { hair } else { skin });
    }
    head.put_pixel(0, 2, hair);
    head.put_pixel(7, 2, hair);
    // Familiar eyes, nose and mouth.
    for x in [1, 2, 5, 6] {
        head.put_pixel(x, 3, eye_white);
    }
    head.put_pixel(2, 3, eye);
    head.put_pixel(5, 3, eye);
    head.put_pixel(4, 4, Rgba([170, 112, 78, 255]));
    for x in 2..=5 {
        head.put_pixel(x, 6, Rgba([102, 58, 43, 255]));
    }
    if size == 0 {
        return head;
    }
    imageops::resize(&head, size, size, imageops::FilterType::Nearest)
}

fn service() -> &'static Service {
    SERVICE.get_or_init(|| {
        let root = cache_root();
        let (tx, rx) = bounded::<String>(64);
        let worker_root = root.clone();
        let worker_available = thread::Builder::new()
            .name("terralens-player-skins".into())
            .spawn(move || worker(rx, worker_root))
            .is_ok();
        Service {
            tx,
            state: Mutex::new(State::default()),
            worker_available,
        }
    })
}

fn worker(rx: Receiver<String>, root: PathBuf) {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(8))
        .redirect(Policy::none())
        .user_agent("TerraLens/0.3 (Minecraft player head cache)")
        .build()
        .ok();
    while let Ok(uuid) = rx.recv() {
        let disk_path = head_cache_path(&root, &uuid);
        let bytes = fs::read(&disk_path).ok();
        let disk_head = bytes
            .as_deref()
            .filter(|bytes| bytes.len() <= MAX_PNG_BYTES)
            .and_then(decode_cached_head);
        let fresh = fs::metadata(&disk_path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < CACHE_TTL);
        if let Some(head) = disk_head.as_ref() {
            publish(&uuid, head.clone());
            if fresh {
                if let Some(service) = SERVICE.get() {
                    let mut state = service.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.queued.remove(&uuid);
                    state.refresh_at.insert(uuid, Instant::now() + CACHE_TTL);
                }
                continue;
            }
        }

        let updated = client.as_ref().and_then(|client| fetch_head(client, &uuid));
        if let Some(head) = updated {
            if let Some(parent) = disk_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let mut bytes = std::io::Cursor::new(Vec::new());
            if head.write_to(&mut bytes, image::ImageFormat::Png).is_ok() {
                let _ = fs::write(&disk_path, bytes.into_inner());
            }
            publish(&uuid, head);
            if let Some(service) = SERVICE.get() {
                let mut state = service.state.lock().unwrap_or_else(|e| e.into_inner());
                state.queued.remove(&uuid);
                state
                    .refresh_at
                    .insert(uuid.clone(), Instant::now() + CACHE_TTL);
                state.retry_at.remove(&uuid);
            }
        } else if let Some(service) = SERVICE.get() {
            let mut state = service.state.lock().unwrap_or_else(|e| e.into_inner());
            state.queued.remove(&uuid);
            state.retry_at.insert(uuid, Instant::now() + FAILURE_RETRY);
        }
    }
}

fn publish(uuid: &str, head: RgbaImage) {
    if let Some(service) = SERVICE.get() {
        let mut state = service.state.lock().unwrap_or_else(|e| e.into_inner());
        state.heads.insert(uuid.to_owned(), head);
        let revision = state.revisions.entry(uuid.to_owned()).or_default();
        *revision = revision.saturating_add(1);
    }
}

fn fetch_head(client: &Client, uuid: &str) -> Option<RgbaImage> {
    let url =
        format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}?unsigned=false");
    let json = read_bounded(client.get(url).send().ok()?, MAX_JSON_BYTES)?;
    let profile: Value = serde_json::from_slice(&json).ok()?;
    if profile.get("id").and_then(Value::as_str)? != uuid {
        return None;
    }
    let props = profile.get("properties")?.as_array()?;
    let property = props
        .iter()
        .find(|p| p.get("name").and_then(Value::as_str) == Some("textures"))?;
    let encoded = property.get("value")?.as_str()?;
    let decoded = decode_base64(encoded)?;
    if decoded.len() > MAX_JSON_BYTES {
        return None;
    }
    let payload: Value = serde_json::from_slice(&decoded).ok()?;
    let url = payload.get("textures")?.get("SKIN")?.get("url")?.as_str()?;
    let url = normalize_texture_url(url)?;
    let png = read_bounded(client.get(url).send().ok()?, MAX_PNG_BYTES)?;
    decode_head(&png)
}

fn read_bounded(response: Response, limit: usize) -> Option<Vec<u8>> {
    if !response.status().is_success() {
        return None;
    }
    if response
        .content_length()
        .is_some_and(|len| len > limit as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

fn decode_head(bytes: &[u8]) -> Option<RgbaImage> {
    if bytes.len() > MAX_PNG_BYTES {
        return None;
    }
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(64);
    limits.max_image_height = Some(64);
    limits.max_alloc = Some(64 * 64 * 4);
    reader.limits(limits);
    let image = reader.decode().ok()?.into_rgba8();
    if image.width() != 64 || !matches!(image.height(), 32 | 64) {
        return None;
    }
    let mut face = imageops::crop_imm(&image, 8, 8, 8, 8).to_image();
    // Minecraft strips alpha from the base head region. Only the second-layer
    // hat uses alpha; transparent base pixels are still skin-colored face data.
    for pixel in face.pixels_mut() {
        pixel[3] = 255;
    }
    // Mojang's legacy Notch transparency hack treats the unused upper-right
    // half of a 64×32 image as padding when that whole area is opaque. A legacy
    // image with any real alpha in that region can carry a valid hat layer.
    let legacy_opaque_padding = image.height() == 32 && legacy_padding_is_opaque(&image);
    if !legacy_opaque_padding {
        let hat = imageops::crop_imm(&image, 40, 8, 8, 8).to_image();
        imageops::overlay(&mut face, &hat, 0, 0);
    }
    Some(face)
}

fn legacy_padding_is_opaque(image: &RgbaImage) -> bool {
    (32..64).all(|x| (0..32).all(|y| image.get_pixel(x, y)[3] >= 128))
}

fn head_cache_path(root: &std::path::Path, uuid: &str) -> PathBuf {
    root.join(format!("head-v2-{uuid}.png"))
}

fn decode_cached_head(bytes: &[u8]) -> Option<RgbaImage> {
    if bytes.len() > MAX_PNG_BYTES {
        return None;
    }
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8);
    limits.max_image_height = Some(8);
    limits.max_alloc = Some(8 * 8 * 4);
    reader.limits(limits);
    let image = reader.decode().ok()?.into_rgba8();
    (image.width() == 8 && image.height() == 8).then_some(image)
}

fn normalize_texture_url(raw: &str) -> Option<reqwest::Url> {
    let Ok(mut url) = reqwest::Url::parse(raw) else {
        return None;
    };
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str() != Some("textures.minecraft.net")
        || url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let expected_port = if url.scheme() == "http" { 80 } else { 443 };
    if url.port_or_known_default() != Some(expected_port) {
        return None;
    }
    // Mojang profile properties historically advertise http URLs. Upgrade only
    // after validating the exact host and default port; never follow redirects.
    url.set_scheme("https").ok()?;
    url.set_port(None).ok()?;
    Some(url)
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    if input.len() > MAX_JSON_BYTES * 2 {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
        if out.len() > MAX_JSON_BYTES {
            return None;
        }
    }
    Some(out)
}

fn normalize_uuid(uuid: &str) -> Option<String> {
    let compact: String = uuid.chars().filter(|&ch| ch != '-').collect();
    if compact.len() != 32 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(compact.to_ascii_lowercase())
}

fn cache_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(root) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join("TerraLens/cache/player-heads");
    }
    if let Some(root) = std::env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join("terralens/player-heads");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".cache/terralens/player-heads")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_real_uuid_shapes() {
        assert_eq!(
            normalize_uuid("12345678-1234-1234-1234-123456789ABC").as_deref(),
            Some("12345678123412341234123456789abc")
        );
        assert_eq!(normalize_uuid("player/1234-abcd"), None);
        assert_eq!(
            normalize_uuid("0".repeat(32).as_str()).as_deref(),
            Some("0".repeat(32).as_str())
        );
    }

    #[test]
    fn extracts_face_with_transparent_hat_composited() {
        let mut skin = RgbaImage::from_pixel(64, 64, Rgba([1, 2, 3, 255]));
        skin.put_pixel(8, 8, Rgba([10, 20, 30, 255]));
        skin.put_pixel(40, 8, Rgba([100, 110, 120, 128]));
        let mut encoded = std::io::Cursor::new(Vec::new());
        skin.write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let head = decode_head(encoded.get_ref()).unwrap();
        let pixel = head.get_pixel(0, 0);
        assert_eq!(pixel[0], 55);
        assert!(pixel[3] >= 254);
        assert_eq!(*head.get_pixel(1, 0), Rgba([1, 2, 3, 255]));
        let mut cached = std::io::Cursor::new(Vec::new());
        head.write_to(&mut cached, image::ImageFormat::Png).unwrap();
        assert_eq!(decode_cached_head(cached.get_ref()).unwrap(), head);
    }

    #[test]
    fn base_face_is_opaque_even_if_source_alpha_is_clear() {
        let mut skin = RgbaImage::from_pixel(64, 64, Rgba([1, 2, 3, 255]));
        skin.put_pixel(8, 8, Rgba([120, 80, 60, 0]));
        skin.put_pixel(40, 8, Rgba([0, 0, 0, 0]));
        let mut encoded = std::io::Cursor::new(Vec::new());
        skin.write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let head = decode_head(encoded.get_ref()).unwrap();
        assert_eq!(*head.get_pixel(0, 0), Rgba([120, 80, 60, 255]));
    }

    #[test]
    fn treats_opaque_unused_hat_area_as_legacy_padding() {
        let mut skin = RgbaImage::from_pixel(64, 32, Rgba([1, 2, 3, 255]));
        for y in 0..8 {
            for x in 0..8 {
                skin.put_pixel(
                    8 + x,
                    8 + y,
                    Rgba([(x * 13) as u8, (y * 17) as u8, 91, 255]),
                );
                skin.put_pixel(40 + x, 8 + y, Rgba([0, 0, 0, 255]));
            }
        }
        let mut encoded = std::io::Cursor::new(Vec::new());
        skin.write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let head = decode_head(encoded.get_ref()).unwrap();
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(
                    *head.get_pixel(x, y),
                    Rgba([(x * 13) as u8, (y * 17) as u8, 91, 255])
                );
            }
        }
    }

    #[test]
    fn preserves_legacy_hat_when_transparency_marks_modern_layer() {
        let mut skin = RgbaImage::from_pixel(64, 32, Rgba([1, 2, 3, 255]));
        for y in 0..8 {
            for x in 0..8 {
                skin.put_pixel(8 + x, 8 + y, Rgba([x as u8 + 10, y as u8 + 20, 91, 255]));
                skin.put_pixel(40 + x, 8 + y, Rgba([0, 0, 0, 0]));
            }
        }
        // One visible overlay pixel plus transparent pixels establishes a
        // modern hat area, even though the skin file has legacy dimensions.
        skin.put_pixel(40, 8, Rgba([190, 70, 35, 255]));
        let mut encoded = std::io::Cursor::new(Vec::new());
        skin.write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let head = decode_head(encoded.get_ref()).unwrap();
        assert_eq!(*head.get_pixel(0, 0), Rgba([190, 70, 35, 255]));
        assert_eq!(*head.get_pixel(1, 0), Rgba([11, 20, 91, 255]));
    }

    #[test]
    fn rejects_texture_host_confusion_and_non_https() {
        assert_eq!(
            normalize_texture_url("https://textures.minecraft.net/texture/abc")
                .unwrap()
                .scheme(),
            "https"
        );
        assert!(normalize_texture_url("https://textures.minecraft.net.evil.test/a").is_none());
        assert_eq!(
            normalize_texture_url("http://textures.minecraft.net/texture/abc")
                .unwrap()
                .scheme(),
            "https"
        );
        assert!(normalize_texture_url("http://textures.minecraft.net:81/texture/abc").is_none());
    }

    #[test]
    fn uses_versioned_cache_keys_to_bypass_bad_legacy_heads() {
        assert!(head_cache_path(std::path::Path::new("cache"), "abc").ends_with("head-v2-abc.png"));
    }

    #[test]
    fn fallback_is_pixelated_and_transparent_free() {
        let head = fallback_head(32);
        assert_eq!(head.dimensions(), (32, 32));
        assert!(head.pixels().all(|p| p[3] == 255));
        assert!(head.pixels().collect::<HashSet<_>>().len() > 3);
    }
}
