//! Read-only Prism instance and Minecraft save import.
//!
//! Save files are only opened for reading. Region records are indexed from the Anvil
//! location table and only generated chunks are decompressed. A complete index is
//! cached against the region file metadata so a running game invalidates it naturally.
use crate::dimension::{Dimension, DimensionInfo};
use crate::model::{Marker, MarkerKind, Point, Settings, WorldInfo, WorldLocation};
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

const MAX_NBT: usize = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, Default)]
pub struct PrismInstanceInfo {
    pub path: PathBuf,
    pub name: String,
    pub minecraft_version: String,
    pub forge_version: String,
    pub core_version: String,
    pub tfc_version: String,
    pub worlds: Vec<WorldInfo>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StructureScan {
    pub markers: Vec<Marker>,
    pub chunks: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RegionStamp {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}
#[derive(Clone, Debug)]
struct CachedScan {
    stamps: Vec<RegionStamp>,
    scan: StructureScan,
}
static SCAN_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedScan>>> = OnceLock::new();

/// The usual Prism instance for this profile. `TERRALENS_INSTANCE` can select another one.
pub fn default_instance() -> PathBuf {
    std::env::var_os("TERRALENS_INSTANCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs_home().join(".local/share/PrismLauncher/instances/TerraFirmaGreg-Modern")
        })
}
fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Deserialize, Default)]
struct Pack {
    #[serde(default)]
    components: Vec<Component>,
}
#[derive(Deserialize)]
struct Component {
    uid: String,
    #[serde(default)]
    version: String,
}

/// Reads Prism component versions and all world saves in `instance/minecraft/saves`.
pub fn inspect_instance(instance: &Path) -> Result<PrismInstanceInfo, String> {
    let game = minecraft_dir(instance)?;
    let pack: Pack = fs::read(instance.join("mmc-pack.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let versions: HashMap<_, _> = pack
        .components
        .into_iter()
        .map(|c| (c.uid, c.version))
        .collect();
    let mut info = PrismInstanceInfo {
        path: instance.to_path_buf(),
        name: instance
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        minecraft_version: version(&versions, "net.minecraft"),
        forge_version: version(&versions, "net.minecraftforge"),
        core_version: "unknown".into(),
        tfc_version: "unknown".into(),
        worlds: vec![],
    };
    // Mod jars remain optional metadata: never extract or execute anything from a pack.
    let mods = game.join("mods");
    if let Ok(items) = fs::read_dir(mods) {
        for item in items.flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            if name.starts_with("TerraFirmaGreg-Core-Modern-") && name.ends_with(".jar") {
                info.core_version = name
                    .trim_start_matches("TerraFirmaGreg-Core-Modern-")
                    .trim_end_matches(".jar")
                    .to_string();
                if let Some(v) = mod_version(&item.path(), "tfc") {
                    info.tfc_version = v;
                }
            } else if name.starts_with("tfc-") && name.ends_with(".jar") {
                info.tfc_version = name
                    .trim_start_matches("tfc-")
                    .trim_end_matches(".jar")
                    .to_string();
            }
        }
    }
    let saves = game.join("saves");
    if saves.is_dir() {
        let mut dirs = fs::read_dir(saves)
            .map_err(|e| format!("Cannot list saves: {e}"))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.join("level.dat").is_file())
            .collect::<Vec<_>>();
        dirs.sort();
        dirs.sort_by_key(|p| {
            fs::metadata(p.join("level.dat"))
                .and_then(|m| m.modified())
                .ok()
        });
        dirs.reverse();
        info.worlds = dirs.iter().map(|p| read_world(p)).collect();
    }
    Ok(info)
}

/// Convenience for the app: enumerate saved worlds under one Prism instance.
pub fn discover_worlds(instance: &Path) -> Result<Vec<WorldInfo>, String> {
    Ok(inspect_instance(instance)?.worlds)
}

fn version(m: &HashMap<String, String>, key: &str) -> String {
    m.get(key).cloned().unwrap_or_else(|| "unknown".into())
}

/// Reads a mod's declared version from a jar's mods.toml without extracting the archive.
fn mod_version(jar: &Path, wanted: &str) -> Option<String> {
    let bytes = fs::read(jar).ok()?;
    let search_start = bytes.len().saturating_sub(65_557);
    let eocd = bytes[search_start..]
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")?
        + search_start;
    let count = u16::from_le_bytes(bytes.get(eocd + 10..eocd + 12)?.try_into().ok()?) as usize;
    let mut cursor = u32::from_le_bytes(bytes.get(eocd + 16..eocd + 20)?.try_into().ok()?) as usize;
    for _ in 0..count {
        if bytes.get(cursor..cursor + 4)? != b"PK\x01\x02" {
            return None;
        }
        let method = u16::from_le_bytes(bytes.get(cursor + 10..cursor + 12)?.try_into().ok()?);
        let compressed =
            u32::from_le_bytes(bytes.get(cursor + 20..cursor + 24)?.try_into().ok()?) as usize;
        let uncompressed =
            u32::from_le_bytes(bytes.get(cursor + 24..cursor + 28)?.try_into().ok()?) as usize;
        let name_len =
            u16::from_le_bytes(bytes.get(cursor + 28..cursor + 30)?.try_into().ok()?) as usize;
        let extra_len =
            u16::from_le_bytes(bytes.get(cursor + 30..cursor + 32)?.try_into().ok()?) as usize;
        let comment_len =
            u16::from_le_bytes(bytes.get(cursor + 32..cursor + 34)?.try_into().ok()?) as usize;
        let local =
            u32::from_le_bytes(bytes.get(cursor + 42..cursor + 46)?.try_into().ok()?) as usize;
        let name = std::str::from_utf8(bytes.get(cursor + 46..cursor + 46 + name_len)?).ok()?;
        if name == "META-INF/mods.toml" && uncompressed <= MAX_NBT {
            let local_name =
                u16::from_le_bytes(bytes.get(local + 26..local + 28)?.try_into().ok()?) as usize;
            let local_extra =
                u16::from_le_bytes(bytes.get(local + 28..local + 30)?.try_into().ok()?) as usize;
            let start = local.checked_add(30 + local_name + local_extra)?;
            let data = bytes.get(start..start.checked_add(compressed)?)?;
            let mut text = Vec::with_capacity(uncompressed);
            match method {
                0 => text.extend_from_slice(data),
                8 => {
                    DeflateDecoder::new(data)
                        .take((MAX_NBT + 1) as u64)
                        .read_to_end(&mut text)
                        .ok()?;
                }
                _ => return None,
            }
            if text.len() > MAX_NBT {
                return None;
            }
            let source = String::from_utf8_lossy(&text);
            let mut is_wanted = false;
            for line in source.lines() {
                let line = line.trim();
                if let Some((key, value)) = line.split_once('=') {
                    let value = value.trim().trim_matches('"');
                    match key.trim() {
                        "modId" => is_wanted = value == wanted,
                        "version" if is_wanted => return Some(value.into()),
                        _ => {}
                    }
                }
            }
            return None;
        }
        cursor = cursor.checked_add(46 + name_len + extra_len + comment_len)?;
    }
    None
}
fn minecraft_dir(instance: &Path) -> Result<PathBuf, String> {
    for n in ["minecraft", ".minecraft"] {
        let p = instance.join(n);
        if p.is_dir() {
            return Ok(p);
        }
    }
    Err(format!("No minecraft directory in {}", instance.display()))
}

fn read_world(path: &Path) -> WorldInfo {
    let fallback = || WorldInfo {
        id: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: path.to_path_buf(),
        seed: 0,
        center: Point::default(),
        settings: Settings::default(),
        compatible: false,
        warning: None,
        dimensions: Vec::new(),
        map_locations: Vec::new(),
    };
    let parsed = (|| -> Result<WorldInfo, String> {
        let level = read_nbt_file(&path.join("level.dat"))?;
        let data = level.get("Data").unwrap_or(&level);
        let worldgen = data.get("WorldGenSettings");
        let seed = worldgen
            .and_then(|g| g.get("seed"))
            .and_then(Nbt::i64)
            .or_else(|| data.get("RandomSeed").and_then(Nbt::i64))
            .unwrap_or(0);
        let dimension_defs = worldgen.and_then(|g| g.get("dimensions"));
        let generator = dimension_defs
            .and_then(|d| d.get("minecraft:overworld"))
            .and_then(|d| d.get("generator"));
        let generator_type = generator
            .and_then(|g| g.get("type"))
            .and_then(Nbt::string)
            .unwrap_or("unknown");
        let settings_node = generator.and_then(|g| g.get("tfc_settings"));
        let settings = read_settings(settings_node);
        let saved_player = data
            .get("Player")
            .is_none()
            .then(|| read_saved_player(path))
            .flatten();
        let player = data.get("Player").or(saved_player.as_ref());
        let is_overworld = player
            .and_then(|p| p.get("Dimension"))
            .and_then(Nbt::string)
            .map(|s| s == "minecraft:overworld" || s == "overworld")
            .unwrap_or(true);
        let pos = player.and_then(|p| p.get("Pos")).and_then(Nbt::list);
        let px = pos.and_then(|p| p.first()).and_then(Nbt::f64);
        let pz = pos.and_then(|p| p.get(2)).and_then(Nbt::f64);
        let spawn_x = data.get("SpawnX").and_then(Nbt::f64).unwrap_or(0.);
        let spawn_z = data.get("SpawnZ").and_then(Nbt::f64).unwrap_or(0.);
        let center = if is_overworld && px.is_some() && pz.is_some() {
            Point {
                x: px.unwrap().floor(),
                z: pz.unwrap().floor(),
            }
        } else {
            Point {
                x: spawn_x,
                z: spawn_z,
            }
        };
        let ver_path = path.join("data/tfg_worldgen_versions.dat");
        let worldgen_version = if ver_path.exists() {
            read_nbt_file(&ver_path)
                .ok()
                .and_then(|n| {
                    n.get("data")
                        .and_then(|n| n.get("generated"))
                        .and_then(|n| n.get("minecraft:overworld"))
                        .and_then(Nbt::i64)
                })
                .unwrap_or(0)
        } else {
            0
        };
        let mut warning = None;
        if worldgen_version != 1 {
            warning = Some(format!(
                "Overworld generation v{worldgen_version} is not supported by the TFG v1 preview."
            ));
        } else if generator_type != "tfc:overworld" {
            warning = Some(format!("Unsupported overworld generator: {generator_type}"));
        }
        let name = data
            .get("LevelName")
            .and_then(Nbt::string)
            .map(str::to_owned)
            .unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let dimensions = discover_dimensions(path, dimension_defs, player);
        let map_locations = read_world_locations(path, data, data.get("Player"));
        Ok(WorldInfo {
            id: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            name,
            path: path.to_path_buf(),
            seed,
            center,
            settings,
            compatible: warning.is_none(),
            warning,
            dimensions,
            map_locations,
        })
    })();
    parsed.unwrap_or_else(|e| {
        let mut w = fallback();
        w.warning = Some(format!("Could not read level.dat: {e}"));
        w
    })
}

/// Load one world with an error result for import workflows that must reject malformed saves.
pub(crate) fn load_world(path: &Path) -> Result<WorldInfo, String> {
    if !path.join("level.dat").is_file() {
        return Err(format!(
            "{} is not a Minecraft world folder (level.dat missing)",
            path.display()
        ));
    }
    let world = read_world(path);
    if let Some(warning) = &world.warning {
        if warning.starts_with("Could not read level.dat:") {
            return Err(warning.clone());
        }
    }
    Ok(world)
}

fn discover_dimensions(
    world: &Path,
    configured: Option<&Nbt>,
    player: Option<&Nbt>,
) -> Vec<DimensionInfo> {
    let mut ids = configured
        .and_then(Nbt::compound)
        .map(|m| m.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    ids.push("minecraft:overworld".into());
    if world.join("DIM-1").is_dir() {
        ids.push("minecraft:the_nether".into());
    }
    let dimension_root = world.join("dimensions");
    if let Ok(namespaces) = fs::read_dir(&dimension_root) {
        for ns in namespaces.flatten() {
            if !ns.path().is_dir() {
                continue;
            }
            if let Ok(paths) = fs::read_dir(ns.path()) {
                for dim in paths.flatten() {
                    if dim.path().is_dir() {
                        if let Ok(rel) = dim.path().strip_prefix(&dimension_root) {
                            let path = rel
                                .components()
                                .filter_map(|p| match p {
                                    std::path::Component::Normal(v) => {
                                        Some(v.to_string_lossy().to_string())
                                    }
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("/");
                            if path.contains('/') {
                                ids.push(path.replace('/', ":"));
                            }
                        }
                    }
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids.into_iter().filter_map(|id| {
        let storage = dimension_storage(world, &id)?;
        let cfg = configured.and_then(|c| c.get(&id));
        let generator = cfg.and_then(|c| c.get("generator"));
        let nether_settings = generator.is_some_and(contains_tfg_nether_settings);
        let supported = id == "minecraft:overworld" || (id == "minecraft:the_nether" && nether_settings);
        let (generated, coverage_center) = region_coverage(&storage);
        let player_matches = player.and_then(|p| p.get("Dimension")).and_then(Nbt::string).is_some_and(|d| d == id || (id == "minecraft:overworld" && d == "overworld"));
        let pos = player_matches.then(|| player.and_then(|p| p.get("Pos")).and_then(Nbt::list)).flatten();
        let center = match (pos.and_then(|p| p.first()).and_then(Nbt::f64), pos.and_then(|p| p.get(2)).and_then(Nbt::f64)) {
            (Some(x), Some(z)) => Point { x: x.floor(), z: z.floor() }, _ => coverage_center,
        };
        let name = Dimension::from_id(&id).map(|d| d.label().to_owned()).unwrap_or_else(|| id.clone());
        let warning = (!supported).then(|| format!("Dimension {id} is recognized, but seed preview is not supported for its generator yet."));
        let is_beneath = Dimension::from_id(&id) == Some(Dimension::Beneath);
        Some(DimensionInfo { id, name, path: storage, generated, center, min_y: if is_beneath { 0 } else { -64 }, height: if is_beneath { 256 } else { 384 }, supported, warning })
    }).collect()
}

fn dimension_storage(world: &Path, id: &str) -> Option<PathBuf> {
    if id == "minecraft:overworld" {
        return Some(world.to_path_buf());
    }
    if id == "minecraft:the_nether" {
        return Some(world.join("DIM-1"));
    }
    let (namespace, path) = id.split_once(':')?;
    if namespace.is_empty()
        || path.is_empty()
        || Path::new(namespace)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    Some(world.join("dimensions").join(namespace).join(path))
}

fn contains_tfg_nether_settings(n: &Nbt) -> bool {
    if n.get("settings")
        .and_then(Nbt::string)
        .is_some_and(|s| s == "tfg:nether_noise")
    {
        return true;
    }
    n.compound()
        .is_some_and(|m| m.values().any(contains_tfg_nether_settings))
        || n.list()
            .is_some_and(|v| v.iter().any(contains_tfg_nether_settings))
}

fn region_coverage(storage: &Path) -> (bool, Point) {
    let region = storage.join("region");
    let Ok(entries) = fs::read_dir(region) else {
        return (false, Point::default());
    };
    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_z = i64::MAX;
    let mut max_z = i64::MIN;
    let mut any = false;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(coords) = name.strip_prefix("r.").and_then(|s| s.strip_suffix(".mca")) else {
            continue;
        };
        let Some((rx, rz)) = coords.split_once('.') else {
            continue;
        };
        let (Ok(rx), Ok(rz)) = (rx.parse::<i32>(), rz.parse::<i32>()) else {
            continue;
        };
        let Ok(mut file) = fs::File::open(entry.path()) else {
            continue;
        };
        let mut header = [0u8; 4096];
        if file.read_exact(&mut header).is_err() {
            continue;
        }
        for slot in 0..1024 {
            let at = slot * 4;
            if header[at..at + 4].iter().all(|b| *b == 0) {
                continue;
            }
            let x = rx as i64 * 32 + (slot % 32) as i64;
            let z = rz as i64 * 32 + (slot / 32) as i64;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_z = min_z.min(z);
            max_z = max_z.max(z);
            any = true;
        }
    }
    if !any {
        return (false, Point::default());
    }
    (
        true,
        Point {
            x: (min_x + max_x) as f64 * 8.0 + 8.0,
            z: (min_z + max_z) as f64 * 8.0 + 8.0,
        },
    )
}

fn read_settings(node: Option<&Nbt>) -> Settings {
    let mut s = Settings::default();
    let Some(n) = node else { return s };
    for (key, out) in [
        ("temperature_scale", &mut s.temperature_scale),
        ("temperature_constant", &mut s.temperature_constant),
        ("rainfall_scale", &mut s.rainfall_scale),
        ("rainfall_constant", &mut s.rainfall_constant),
        ("continentalness", &mut s.continentalness),
        ("grass_density", &mut s.grass_density),
    ] {
        if let Some(v) = n.get(key).and_then(Nbt::f64) {
            if v.is_finite() {
                *out = v;
            }
        }
    }
    s
}

fn read_saved_player(world: &Path) -> Option<Nbt> {
    let entries = fs::read_dir(world.join("playerdata")).ok()?;
    let mut candidates = entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "dat")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    candidates
        .into_iter()
        .find_map(|(_, path)| read_nbt_file(&path).ok())
}

const WORLD_BORDER_LIMIT: f64 = 29_999_984.0;

fn valid_marker_position(x: f64, y: f64, z: f64) -> bool {
    x.is_finite()
        && z.is_finite()
        && y.is_finite()
        && x.abs() <= WORLD_BORDER_LIMIT
        && z.abs() <= WORLD_BORDER_LIMIT
        && y.abs() <= 4096.0
}

fn read_world_locations(world: &Path, data: &Nbt, embedded: Option<&Nbt>) -> Vec<WorldLocation> {
    let mut locations = Vec::new();
    if let (Some(x), Some(y), Some(z)) = (
        data.get("SpawnX").and_then(Nbt::f64),
        data.get("SpawnY").and_then(Nbt::f64),
        data.get("SpawnZ").and_then(Nbt::f64),
    ) {
        if valid_marker_position(x, y, z) {
            locations.push(location_marker(
                "minecraft:world_spawn".into(),
                "World spawn".into(),
                MarkerKind::Spawn,
                "minecraft:overworld".into(),
                x,
                y,
                z,
                "Saved world spawn position".into(),
                [97, 190, 115],
            ));
        }
    }
    let names = read_player_names(world);
    let mut players =
        std::collections::BTreeMap::<String, (String, String, f64, f64, f64, bool)>::new();
    if let Some(player) = embedded {
        insert_player_location(
            &mut players,
            player,
            player_uuid(player).unwrap_or_else(|| "embedded-player".into()),
            &names,
            true,
        );
    }
    if let Ok(entries) = fs::read_dir(world.join("playerdata")) {
        let mut files = entries
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "dat")
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|entry| entry.file_name());
        for entry in files {
            let Some(uuid) = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| valid_uuid(s))
                .map(str::to_owned)
            else {
                continue;
            };
            let Ok(player) = read_nbt_file(&entry.path()) else {
                continue;
            };
            insert_player_location(&mut players, &player, uuid, &names, false);
        }
    }
    for (uuid, (label, dimension, x, y, z, _embedded)) in players {
        let normalized = normalized_uuid(&uuid);
        let short = normalized.get(..8).unwrap_or("unknown");
        let fallback = format!("Player {short}");
        let label = if label.is_empty() { fallback } else { label };
        locations.push(location_marker(
            format!("minecraft:player/{normalized}"),
            label,
            MarkerKind::Player,
            dimension,
            x,
            y,
            z,
            "Saved player position · last recorded in this world".into(),
            [110, 168, 221],
        ));
    }
    locations
}

fn insert_player_location(
    players: &mut std::collections::BTreeMap<String, (String, String, f64, f64, f64, bool)>,
    player: &Nbt,
    uuid: String,
    names: &HashMap<String, String>,
    embedded: bool,
) {
    let dimension = player
        .get("Dimension")
        .and_then(Nbt::string)
        .map(str::to_owned);
    let pos = player.get("Pos").and_then(Nbt::list);
    let (Some(dimension), Some(pos)) = (dimension, pos) else {
        return;
    };
    let (Some(x), Some(y), Some(z)) = (
        pos.first().and_then(Nbt::f64),
        pos.get(1).and_then(Nbt::f64),
        pos.get(2).and_then(Nbt::f64),
    ) else {
        return;
    };
    if pos.len() < 3 || !valid_marker_position(x, y, z) {
        return;
    }
    let normalized = normalized_uuid(&uuid);
    let name = names.get(&normalized).cloned().unwrap_or_default();
    let key = if uuid == "embedded-player" {
        uuid
    } else {
        normalized
    };
    let value = (name, dimension, x, y, z, embedded);
    match players.get(&key) {
        Some(existing) if existing.5 && !embedded => {}
        _ => {
            players.insert(key, value);
        }
    }
}

fn valid_uuid(uuid: &str) -> bool {
    let compact = normalized_uuid(uuid);
    compact.len() == 32 && compact.chars().all(|c| c.is_ascii_hexdigit())
}

fn location_marker(
    id: String,
    label: String,
    kind: MarkerKind,
    dimension: String,
    x: f64,
    y: f64,
    z: f64,
    detail: String,
    color: [u8; 3],
) -> WorldLocation {
    WorldLocation {
        dimension,
        marker: Marker {
            id,
            label,
            namespace: "minecraft".into(),
            x,
            z,
            y: Some(y),
            kind,
            confirmed: true,
            min_y: Some(y),
            max_y: Some(y),
            detail,
            color,
            bounds: None,
        },
    }
}

fn player_uuid(player: &Nbt) -> Option<String> {
    if let Some(values) = player.get("UUID").and_then(Nbt::int_array) {
        if values.len() == 4 {
            let mut hex = String::with_capacity(32);
            for value in values {
                hex.push_str(&format!("{:08x}", *value as u32));
            }
            return Some(format!(
                "{}-{}-{}-{}-{}",
                &hex[0..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..32]
            ));
        }
    }
    let most = player.get("UUIDMost").and_then(Nbt::i64)? as u64;
    let least = player.get("UUIDLeast").and_then(Nbt::i64)? as u64;
    let compact = format!("{most:016x}{least:016x}");
    Some(format!(
        "{}-{}-{}-{}-{}",
        &compact[0..8],
        &compact[8..12],
        &compact[12..16],
        &compact[16..20],
        &compact[20..32]
    ))
}

fn normalized_uuid(uuid: &str) -> String {
    uuid.chars()
        .filter(|c| *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn read_player_names(world: &Path) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let directories = world
        .parent()
        .into_iter()
        .chain(world.parent().and_then(Path::parent));
    for directory in directories {
        for filename in ["usercache.json", "usernamecache.json"] {
            let Ok(bytes) = fs::read(directory.join(filename)) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            if let Some(entries) = value.as_array() {
                for entry in entries {
                    if let (Some(uuid), Some(name)) = (
                        entry.get("uuid").and_then(|x| x.as_str()),
                        entry.get("name").and_then(|x| x.as_str()),
                    ) {
                        names
                            .entry(normalized_uuid(uuid))
                            .or_insert_with(|| name.to_owned());
                    }
                }
            } else if let Some(entries) = value.as_object() {
                for (key, value) in entries {
                    let Some(value) = value.as_str() else {
                        continue;
                    };
                    let pair = if valid_uuid(key) {
                        Some((key.as_str(), value))
                    } else if valid_uuid(value) {
                        Some((value, key.as_str()))
                    } else {
                        None
                    };
                    if let Some((uuid, name)) = pair {
                        names
                            .entry(normalized_uuid(uuid))
                            .or_insert_with(|| name.to_owned());
                    }
                }
            }
        }
    }
    names
}

#[derive(Clone, Debug)]
enum Nbt {
    Number(f64),
    Long(i64),
    String(String),
    List(Vec<Nbt>),
    Compound(HashMap<String, Nbt>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
    Other,
}
impl Nbt {
    fn get(&self, k: &str) -> Option<&Nbt> {
        if let Self::Compound(m) = self {
            m.get(k)
        } else {
            None
        }
    }
    fn string(&self) -> Option<&str> {
        if let Self::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    fn list(&self) -> Option<&[Nbt]> {
        if let Self::List(v) = self {
            Some(v)
        } else {
            None
        }
    }
    fn int_array(&self) -> Option<&[i32]> {
        if let Self::IntArray(v) = self {
            Some(v)
        } else {
            None
        }
    }
    fn f64(&self) -> Option<f64> {
        match self {
            Self::Number(v) => Some(*v),
            Self::Long(v) => Some(*v as f64),
            _ => None,
        }
    }
    fn i64(&self) -> Option<i64> {
        match self {
            Self::Long(v) => Some(*v),
            Self::Number(v) if v.is_finite() => Some(*v as i64),
            _ => None,
        }
    }
}
struct Reader<'a> {
    d: &'a [u8],
    p: usize,
    read_long_arrays: bool,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let e = self.p.checked_add(n).ok_or("NBT length overflow")?;
        if e > self.d.len() {
            return Err("Truncated NBT".into());
        }
        let b = &self.d[self.p..e];
        self.p = e;
        Ok(b)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn text(&mut self) -> Result<String, String> {
        let n = u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn payload(&mut self, t: u8, depth: usize) -> Result<Nbt, String> {
        if depth > MAX_DEPTH {
            return Err("NBT nesting limit exceeded".into());
        }
        Ok(match t {
            1 => Nbt::Number(self.u8()? as i8 as f64),
            2 => Nbt::Number(i16::from_be_bytes(self.take(2)?.try_into().unwrap()) as f64),
            3 => Nbt::Number(self.i32()? as f64),
            4 => Nbt::Long(i64::from_be_bytes(self.take(8)?.try_into().unwrap())),
            5 => Nbt::Number(f32::from_be_bytes(self.take(4)?.try_into().unwrap()) as f64),
            6 => Nbt::Number(f64::from_be_bytes(self.take(8)?.try_into().unwrap())),
            7 => {
                let n = self.i32()?;
                if n < 0 || n as usize > MAX_NBT {
                    return Err("Invalid NBT byte array length".into());
                }
                self.take(n as usize)?;
                Nbt::Other
            }
            8 => Nbt::String(self.text()?),
            9 => {
                let kind = self.u8()?;
                let n = self.i32()?;
                if n < 0 || n > 2_000_000 || (kind == 0 && n > 0) {
                    return Err("Invalid NBT list".into());
                }
                let mut v = Vec::with_capacity(n.min(65536) as usize);
                for _ in 0..n {
                    v.push(self.payload(kind, depth + 1)?)
                }
                Nbt::List(v)
            }
            10 => {
                let mut m = HashMap::new();
                loop {
                    let k = self.u8()?;
                    if k == 0 {
                        break;
                    }
                    let name = self.text()?;
                    m.insert(name, self.payload(k, depth + 1)?);
                }
                Nbt::Compound(m)
            }
            11 | 12 => {
                let n = self.i32()?;
                let max = if t == 12 { MAX_NBT / 8 } else { MAX_NBT / 4 };
                if n < 0 || n as usize > max {
                    return Err("Invalid NBT array length".into());
                }
                if t == 11 {
                    if n == 4 || n == 6 {
                        let mut values = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            values.push(self.i32()?);
                        }
                        Nbt::IntArray(values)
                    } else {
                        self.take(n as usize * 4)?;
                        Nbt::Other
                    }
                } else if self.read_long_arrays {
                    let mut values = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        values.push(i64::from_be_bytes(self.take(8)?.try_into().unwrap()));
                    }
                    Nbt::LongArray(values)
                } else {
                    self.take(n as usize * 8)?;
                    Nbt::Other
                }
            }
            _ => return Err(format!("Unknown NBT tag {t}")),
        })
    }
    fn root(&mut self) -> Result<Nbt, String> {
        if self.u8()? != 10 {
            return Err("Expected NBT compound root".into());
        }
        self.text()?;
        self.payload(10, 0)
    }
}

mod surface;
pub use surface::{
    SurfaceChunk, SurfaceColumn, clip_surface_chunk, read_biome_chunk, read_surface_band,
    read_surface_slice, read_surface_tile,
};
fn decode_nbt(bytes: &[u8]) -> Result<Nbt, String> {
    decode_nbt_mode(bytes, false)
}
fn decode_nbt_with_long_arrays(bytes: &[u8]) -> Result<Nbt, String> {
    decode_nbt_mode(bytes, true)
}
fn decode_nbt_mode(bytes: &[u8], read_long_arrays: bool) -> Result<Nbt, String> {
    if bytes.len() > MAX_NBT {
        return Err("NBT exceeds 64 MiB limit".into());
    }
    Reader {
        d: bytes,
        p: 0,
        read_long_arrays,
    }
    .root()
}
fn read_nbt_file(path: &Path) -> Result<Nbt, String> {
    let raw = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoded = Vec::new();
    GzDecoder::new(raw.as_slice())
        .take((MAX_NBT + 1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|e| format!("Invalid gzip NBT: {e}"))?;
    decode_nbt(&decoded)
}

/// Scans confirmed structure starts from saved overworld Anvil region files.
pub fn scan_structures(world: &Path) -> Result<StructureScan, String> {
    let region = world.join("region");
    let mut paths = fs::read_dir(&region)
        .map_err(|e| format!("{}: {e}", region.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("mca"))
        .collect::<Vec<_>>();
    paths.sort();
    let stamps = paths
        .iter()
        .filter_map(|p| {
            let m = fs::metadata(p).ok()?;
            Some(RegionStamp {
                path: p.clone(),
                len: m.len(),
                modified: m.modified().ok(),
            })
        })
        .collect::<Vec<_>>();
    let cache = SCAN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(c) = cache.lock().unwrap().get(world) {
        if c.stamps == stamps {
            return Ok(c.scan.clone());
        }
    }
    let results = paths.par_iter().map(|p| scan_region(p)).collect::<Vec<_>>();
    let mut scan = StructureScan::default();
    for (mut markers, chunks, errors) in results {
        scan.markers.append(&mut markers);
        scan.chunks += chunks;
        scan.errors.extend(errors);
    }
    scan.markers.sort_by(|a, b| {
        a.z.total_cmp(&b.z)
            .then(a.x.total_cmp(&b.x))
            .then(a.id.cmp(&b.id))
    });
    cache.lock().unwrap().insert(
        world.to_path_buf(),
        CachedScan {
            stamps,
            scan: scan.clone(),
        },
    );
    Ok(scan)
}

fn scan_region(path: &Path) -> (Vec<Marker>, usize, Vec<String>) {
    let mut markers = Vec::new();
    let mut chunks = 0;
    let mut errors = Vec::new();
    let data = match fs::read(path) {
        Ok(v) => v,
        Err(e) => return (markers, 0, vec![format!("{}: {e}", path.display())]),
    };
    if data.is_empty() {
        return (markers, 0, errors);
    }
    if data.len() < 4096 {
        return (
            markers,
            0,
            vec![format!("{}: short Anvil header", path.display())],
        );
    }
    for i in 0..1024 {
        let e = &data[i * 4..i * 4 + 4];
        let sector = ((e[0] as usize) << 16) | ((e[1] as usize) << 8) | e[2] as usize;
        if sector == 0 {
            continue;
        }
        let off = sector * 4096;
        if off + 5 > data.len() {
            errors.push(format!("{} slot {i}: invalid chunk header", path.display()));
            continue;
        }
        let len = u32::from_be_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        if len < 1 || off + 4 + len > data.len() {
            errors.push(format!("{} slot {i}: invalid chunk length", path.display()));
            continue;
        }
        let raw = &data[off + 5..off + 4 + len];
        let mut decoded = Vec::new();
        let result = match data[off + 4] {
            1 => GzDecoder::new(raw).read_to_end(&mut decoded),
            2 => ZlibDecoder::new(raw).read_to_end(&mut decoded),
            3 => {
                decoded.extend_from_slice(raw);
                Ok(decoded.len())
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unknown chunk compression",
            )),
        };
        if let Err(e) = result {
            errors.push(format!("{} slot {i}: decompress: {e}", path.display()));
            continue;
        }
        match decode_nbt(&decoded) {
            Ok(nbt) => {
                chunks += 1;
                markers.extend(chunk_markers(&nbt));
            }
            Err(e) => errors.push(format!("{} slot {i}: {e}", path.display())),
        }
    }
    (markers, chunks, errors)
}
fn chunk_markers(root: &Nbt) -> Vec<Marker> {
    let Some(starts) = root
        .get("structures")
        .and_then(|s| s.get("starts"))
        .and_then(Nbt::compound)
    else {
        return vec![];
    };
    starts
        .iter()
        .filter_map(|(sid, tag)| structure_marker(sid, tag))
        .collect()
}
impl Nbt {
    fn compound(&self) -> Option<&HashMap<String, Nbt>> {
        if let Self::Compound(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
fn structure_marker(sid: &str, tag: &Nbt) -> Option<Marker> {
    let id = tag.get("id").and_then(Nbt::string).unwrap_or(sid);
    if id == "INVALID" {
        return None;
    }
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    let cx = tag.get("ChunkX").and_then(Nbt::i64).unwrap_or(0) as f64;
    let cz = tag.get("ChunkZ").and_then(Nbt::i64).unwrap_or(0) as f64;
    let bounds = structure_bounds(tag);
    let min_y = bounds.map(|b| b.min_y as f64);
    let max_y = bounds.map(|b| b.max_y as f64);
    let detail = match bounds {
        Some(b) => format!(
            "Saved generated structure start · bounds X {}–{}, Y {}–{}, Z {}–{}",
            b.min_x, b.max_x, b.min_y, b.max_y, b.min_z, b.max_z
        ),
        _ => "Saved generated structure start; vertical bounds unavailable".into(),
    };
    Some(Marker {
        id: id.into(),
        label: path
            .split('/')
            .map(|p| {
                let mut c = p.chars();
                c.next()
                    .map(|f| f.to_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join("/"),
        namespace: namespace.into(),
        x: cx * 16. + 8.,
        z: cz * 16. + 8.,
        y: min_y,
        kind: MarkerKind::Structure,
        confirmed: true,
        min_y,
        max_y,
        bounds,
        detail,
        color: [210, 170, 104],
    })
}

fn structure_bounds(tag: &Nbt) -> Option<crate::model::StructureBounds> {
    let parse = |node: &Nbt| -> Option<[i32; 6]> {
        let values: [i32; 6] = if let Some(values) = node.int_array() {
            if values.len() == 6 {
                values.try_into().ok()?
            } else {
                return None;
            }
        } else {
            let values = node.list()?;
            if values.len() != 6 {
                return None;
            }
            [
                values[0].i64()?.try_into().ok()?,
                values[1].i64()?.try_into().ok()?,
                values[2].i64()?.try_into().ok()?,
                values[3].i64()?.try_into().ok()?,
                values[4].i64()?.try_into().ok()?,
                values[5].i64()?.try_into().ok()?,
            ]
        };
        if values[0] > values[3] || values[1] > values[4] || values[2] > values[5] {
            return None;
        }
        Some(values)
    };
    let direct = tag.get("BB").and_then(parse);
    let aggregate = || {
        let boxes = tag
            .get("Children")?
            .list()?
            .iter()
            .filter_map(|child| child.get("BB").and_then(parse))
            .collect::<Vec<_>>();
        if boxes.is_empty() {
            return None;
        }
        Some([
            boxes.iter().map(|b| b[0]).min()?,
            boxes.iter().map(|b| b[1]).min()?,
            boxes.iter().map(|b| b[2]).min()?,
            boxes.iter().map(|b| b[3]).max()?,
            boxes.iter().map(|b| b[4]).max()?,
            boxes.iter().map(|b| b[5]).max()?,
        ])
    };
    let bb = direct.or_else(aggregate)?;
    Some(crate::model::StructureBounds {
        min_x: bb[0],
        min_y: bb[1],
        min_z: bb[2],
        max_x: bb[3],
        max_y: bb[4],
        max_z: bb[5],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    #[test]
    fn beneath_generator_support_requires_tfg_noise_settings_and_paths_are_confined() {
        let wrapped = Nbt::Compound(HashMap::from([(
            "generator".into(),
            Nbt::Compound(HashMap::from([(
                "settings".into(),
                Nbt::String("tfg:nether_noise".into()),
            )])),
        )]));
        let vanilla = Nbt::Compound(HashMap::from([(
            "settings".into(),
            Nbt::String("minecraft:nether".into()),
        )]));
        let tfc_only = Nbt::String("tfc:nether_noise".into());
        assert!(contains_tfg_nether_settings(&wrapped));
        assert!(!contains_tfg_nether_settings(&vanilla));
        assert!(!contains_tfg_nether_settings(&tfc_only));
        let world = Path::new("/tmp/save");
        assert_eq!(
            dimension_storage(world, "minecraft:the_nether"),
            Some(world.join("DIM-1"))
        );
        assert!(dimension_storage(world, "minecraft:../../outside").is_none());
    }
    fn name(v: &str) -> Vec<u8> {
        let mut b = (v.len() as u16).to_be_bytes().to_vec();
        b.extend(v.as_bytes());
        b
    }
    fn long_tag(t: u8, n: &str, v: i64) -> Vec<u8> {
        let mut b = vec![t];
        b.extend(name(n));
        b.extend(v.to_be_bytes());
        b
    }
    fn string_tag(n: &str, v: &str) -> Vec<u8> {
        let mut b = vec![8];
        b.extend(name(n));
        b.extend((v.len() as u16).to_be_bytes());
        b.extend(v.as_bytes());
        b
    }
    fn double_list_tag(n: &str, values: [f64; 3]) -> Vec<u8> {
        let mut b = vec![9];
        b.extend(name(n));
        b.push(6);
        b.extend(3i32.to_be_bytes());
        for value in values {
            b.extend(value.to_be_bytes());
        }
        b
    }
    fn int_array_tag(n: &str, values: &[i32]) -> Vec<u8> {
        let mut bytes = vec![11];
        bytes.extend(name(n));
        bytes.extend((values.len() as i32).to_be_bytes());
        for value in values {
            bytes.extend(value.to_be_bytes());
        }
        bytes
    }
    fn int_tag(n: &str, v: i32) -> Vec<u8> {
        let mut b = vec![3];
        b.extend(name(n));
        b.extend(v.to_be_bytes());
        b
    }
    fn compound(n: &str, mut body: Vec<u8>) -> Vec<u8> {
        let mut b = vec![10];
        b.extend(name(n));
        body.push(0);
        b.extend(body);
        b
    }
    fn write_gzip(path: &Path, bytes: &[u8]) {
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(bytes).unwrap();
        fs::write(path, gz.finish().unwrap()).unwrap();
    }
    #[test]
    fn exact_long_seed_survives() {
        let seed = 7_787_531_943_268_589_495i64;
        let mut b = vec![10];
        b.extend(name(""));
        b.extend(long_tag(4, "seed", seed));
        b.push(0);
        let n = decode_nbt(&b).unwrap();
        assert_eq!(n.get("seed").and_then(Nbt::i64), Some(seed));
    }
    #[test]
    fn reads_saved_world_and_rejects_worldgen_v0() {
        let root =
            std::env::temp_dir().join(format!("terralens-prism-test-{}", std::process::id()));
        let world = root.join("saves/Test World");
        fs::create_dir_all(world.join("data")).unwrap();
        let seed = 7_787_531_943_268_589_495i64;
        let mut generator_body = string_tag("type", "tfc:overworld");
        let settings = compound(
            "tfc_settings",
            [
                int_tag("temperature_scale", 31_000),
                int_tag("rainfall_scale", 18_000),
            ]
            .concat(),
        );
        generator_body.extend(settings);
        let generator = compound("generator", generator_body);
        let overworld = compound("minecraft:overworld", generator);
        let dimensions = compound("dimensions", overworld);
        let mut worldgen_body = long_tag(4, "seed", seed);
        worldgen_body.extend(dimensions);
        let worldgen = compound("WorldGenSettings", worldgen_body);
        let mut player_body = string_tag("Dimension", "minecraft:overworld");
        let mut pos = vec![9];
        pos.extend(name("Pos"));
        pos.push(6);
        pos.extend(3i32.to_be_bytes());
        for p in [123.75f64, 65.0, -456.25] {
            pos.extend(p.to_be_bytes());
        }
        player_body.extend(pos);
        let player = compound("Player", player_body);
        let mut data = string_tag("LevelName", "Test World");
        data.extend(worldgen);
        data.extend(player);
        data.extend(int_tag("SpawnX", 0));
        data.extend(int_tag("SpawnZ", 0));
        write_gzip(
            &world.join("level.dat"),
            &compound("", compound("Data", data)),
        );
        let generated = compound("generated", int_tag("minecraft:overworld", 0));
        write_gzip(
            &world.join("data/tfg_worldgen_versions.dat"),
            &compound("", compound("data", generated)),
        );
        let info = read_world(&world);
        assert_eq!(info.seed, seed);
        assert_eq!(info.name, "Test World");
        assert_eq!(info.center, Point { x: 123., z: -457. });
        assert_eq!(info.settings.temperature_scale, 31_000.);
        assert_eq!(info.settings.rainfall_scale, 18_000.);
        assert!(!info.compatible);
        assert!(info.warning.unwrap().contains("v0"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_confirmed_structure_start_and_box() {
        let child = Nbt::Compound(HashMap::from([(
            "BB".into(),
            Nbt::IntArray(vec![-45, 30, 60, -20, 50, 88]),
        )]));
        let start = Nbt::Compound(HashMap::from([
            ("id".into(), Nbt::String("tfc_ruins:ancient_tower".into())),
            ("ChunkX".into(), Nbt::Number(-3.)),
            ("ChunkZ".into(), Nbt::Number(4.)),
            ("Children".into(), Nbt::List(vec![child])),
        ]));
        let marker = structure_marker("unused:key", &start).unwrap();
        assert_eq!(marker.id, "tfc_ruins:ancient_tower");
        assert_eq!((marker.x, marker.z), (-40., 72.));
        assert_eq!((marker.min_y, marker.max_y), (Some(30.), Some(50.)));
        assert_eq!(
            marker.bounds,
            Some(crate::model::StructureBounds {
                min_x: -45,
                min_y: 30,
                min_z: 60,
                max_x: -20,
                max_y: 50,
                max_z: 88
            })
        );
        assert!(marker.detail.contains("X -45–-20"));
        assert!(marker.confirmed);
    }

    #[test]
    fn generic_mod_structure_bbox_is_kept_even_when_start_point_is_far_away() {
        let start = Nbt::Compound(HashMap::from([
            ("id".into(), Nbt::String("randommod:deep/temple".into())),
            ("ChunkX".into(), Nbt::Number(1000.)),
            ("ChunkZ".into(), Nbt::Number(-800.)),
            ("BB".into(), Nbt::IntArray(vec![-20, 5, 70, 31, 45, 120])),
        ]));
        let marker = structure_marker("unused", &start).unwrap();
        assert_eq!(marker.id, "randommod:deep/temple");
        assert_eq!((marker.x, marker.z), (16008., -12792.));
        assert_eq!(marker.bounds.unwrap().min_x, -20);
        assert_eq!(marker.bounds.unwrap().max_z, 120);
    }

    #[test]
    fn saved_spawn_and_player_locations_validate_deduplicate_and_keep_dimensions() {
        let root =
            std::env::temp_dir().join(format!("terralens-location-test-{}", std::process::id()));
        let world = root.join(".minecraft/saves/world");
        fs::create_dir_all(world.join("playerdata")).unwrap();
        fs::write(
            world
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("usercache.json"),
            br#"[{"name":"NetherExplorer","uuid":"00000000-0000-0001-0000-000000000002"}]"#,
        )
        .unwrap();
        fs::write(
            world.parent().unwrap().join("usernamecache.json"),
            br#"{"00000000-0000-0001-0000-000000000002":"DirectForgeName"}"#,
        )
        .unwrap();
        let data = Nbt::Compound(HashMap::from([
            ("SpawnX".into(), Nbt::Number(120.)),
            ("SpawnY".into(), Nbt::Number(72.)),
            ("SpawnZ".into(), Nbt::Number(-200.)),
        ]));
        let embedded = Nbt::Compound(HashMap::from([
            ("UUIDMost".into(), Nbt::Long(1)),
            ("UUIDLeast".into(), Nbt::Long(2)),
            (
                "Dimension".into(),
                Nbt::String("minecraft:the_nether".into()),
            ),
            (
                "Pos".into(),
                Nbt::List(vec![
                    Nbt::Number(-42.5),
                    Nbt::Number(88.0),
                    Nbt::Number(19.25),
                ]),
            ),
        ]));
        let player_file = |most, least, dimension: &str, pos| {
            let mut body = long_tag(4, "UUIDMost", most);
            body.extend(long_tag(4, "UUIDLeast", least));
            body.extend(string_tag("Dimension", dimension));
            body.extend(double_list_tag("Pos", pos));
            compound("", body)
        };
        write_gzip(
            &world.join("playerdata/00000000-0000-0001-0000-000000000002.dat"),
            &player_file(1, 2, "minecraft:the_nether", [9., 80., 10.]),
        );
        write_gzip(
            &world.join("playerdata/00000000-0000-0003-0000-000000000004.dat"),
            &player_file(3, 4, "minecraft:overworld", [30., 65., 40.]),
        );
        fs::write(world.join("playerdata/malformed.dat"), b"bad gzip").unwrap();
        let locations = read_world_locations(&world, &data, Some(&embedded));
        assert_eq!(locations.len(), 3);
        let spawn = locations
            .iter()
            .find(|loc| loc.marker.kind == MarkerKind::Spawn)
            .unwrap();
        assert_eq!(spawn.dimension, "minecraft:overworld");
        assert_eq!(
            (spawn.marker.x, spawn.marker.y, spawn.marker.z),
            (120., Some(72.), -200.)
        );
        let nether = locations
            .iter()
            .find(|loc| loc.dimension == "minecraft:the_nether")
            .unwrap();
        assert_eq!(nether.marker.label, "DirectForgeName");
        assert_eq!((nether.marker.x, nether.marker.y), (-42.5, Some(88.)));
        assert_eq!(
            locations
                .iter()
                .filter(|loc| loc.marker.kind == MarkerKind::Player)
                .count(),
            2
        );
        assert!(
            locations
                .iter()
                .all(|loc| loc.marker.confirmed && loc.marker.bounds.is_none())
        );

        let no_spawn = Nbt::Compound(HashMap::from([
            ("SpawnX".into(), Nbt::Number(0.)),
            ("SpawnZ".into(), Nbt::Number(0.)),
        ]));
        assert!(
            !read_world_locations(&root.join("empty"), &no_spawn, None)
                .iter()
                .any(|loc| loc.marker.kind == MarkerKind::Spawn)
        );
        let invalid = Nbt::Compound(HashMap::from([
            ("SpawnX".into(), Nbt::Number(f64::INFINITY)),
            ("SpawnY".into(), Nbt::Number(0.)),
            ("SpawnZ".into(), Nbt::Number(0.)),
        ]));
        assert!(read_world_locations(&root.join("empty"), &invalid, None).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_structures_uses_the_supplied_dimension_root_and_accepts_any_namespace() {
        let base = std::env::temp_dir().join(format!(
            "terralens-dimension-structure-test-{}",
            std::process::id()
        ));
        let overworld = base.join("world");
        let beneath = overworld.join("DIM-1");
        fs::create_dir_all(overworld.join("region")).unwrap();
        fs::create_dir_all(beneath.join("region")).unwrap();
        let write_start = |path: &Path, sid: &str, bb: [i32; 6]| {
            let start = compound(
                "randommod:site",
                [
                    string_tag("id", sid),
                    int_tag("ChunkX", -2),
                    int_tag("ChunkZ", 3),
                    int_array_tag("BB", &bb),
                ]
                .concat(),
            );
            let starts = compound("starts", start);
            let structures = compound("structures", starts);
            let raw = compound("", structures);
            let mut gz = GzEncoder::new(Vec::new(), Compression::default());
            gz.write_all(&raw).unwrap();
            let compressed = gz.finish().unwrap();
            let length = compressed.len() + 1;
            let mut mca = vec![0u8; 3 * 4096];
            mca[..4].copy_from_slice(&[0, 0, 2, 1]);
            mca[8192..8196].copy_from_slice(&(length as u32).to_be_bytes());
            mca[8196] = 1;
            mca[8197..8197 + compressed.len()].copy_from_slice(&compressed);
            fs::write(path, mca).unwrap();
        };
        write_start(
            &overworld.join("region/r.0.0.mca"),
            "mod_a:overworld_site",
            [-3, 4, 7, 8, 22, 15],
        );
        write_start(
            &beneath.join("region/r.0.0.mca"),
            "mod_b:beneath_site",
            [100, 40, -20, 120, 90, 5],
        );
        let over = scan_structures(&overworld).unwrap();
        let nether = scan_structures(&beneath).unwrap();
        assert_eq!(over.markers.len(), 1);
        assert_eq!(over.markers[0].id, "mod_a:overworld_site");
        assert_eq!(over.markers[0].bounds.unwrap().min_x, -3);
        assert_eq!(nether.markers.len(), 1);
        assert_eq!(nether.markers[0].id, "mod_b:beneath_site");
        assert_eq!(nether.markers[0].bounds.unwrap().max_y, 90);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn structure_start_bounds_take_precedence_and_children_aggregate_when_absent() {
        let piece = |bb| Nbt::Compound(HashMap::from([("BB".into(), Nbt::IntArray(bb))]));
        let children = Nbt::List(vec![
            piece(vec![-9, 20, 4, 0, 30, 12]),
            piece(vec![-20, 17, -2, 3, 45, 14]),
        ]);
        let with_start = Nbt::Compound(HashMap::from([
            ("id".into(), Nbt::String("tfg:test".into())),
            ("BB".into(), Nbt::IntArray(vec![-5, 8, 10, 6, 55, 21])),
            ("Children".into(), children.clone()),
        ]));
        assert_eq!(
            structure_bounds(&with_start),
            Some(crate::model::StructureBounds {
                min_x: -5,
                min_y: 8,
                min_z: 10,
                max_x: 6,
                max_y: 55,
                max_z: 21
            })
        );
        let without_start = Nbt::Compound(HashMap::from([("Children".into(), children)]));
        assert_eq!(
            structure_bounds(&without_start),
            Some(crate::model::StructureBounds {
                min_x: -20,
                min_y: 17,
                min_z: -2,
                max_x: 3,
                max_y: 45,
                max_z: 14
            })
        );
    }

    #[test]
    fn malformed_direct_structure_bounds_fall_back_to_valid_children_and_skip_bad_pieces() {
        let invalid_direct = Nbt::IntArray(vec![10, 8, 0, -10, 4, 2]);
        let good = Nbt::Compound(HashMap::from([(
            "BB".into(),
            Nbt::IntArray(vec![-4, 12, -2, 6, 24, 8]),
        )]));
        let invalid_piece = Nbt::Compound(HashMap::from([(
            "BB".into(),
            Nbt::IntArray(vec![20, 9, 0, 10, 30, 2]),
        )]));
        let tag = Nbt::Compound(HashMap::from([
            ("BB".into(), invalid_direct),
            ("Children".into(), Nbt::List(vec![invalid_piece, good])),
        ]));
        assert_eq!(
            structure_bounds(&tag),
            Some(crate::model::StructureBounds {
                min_x: -4,
                min_y: 12,
                min_z: -2,
                max_x: 6,
                max_y: 24,
                max_z: 8
            })
        );
        let invalid_only = Nbt::Compound(HashMap::from([(
            "BB".into(),
            Nbt::IntArray(vec![0, 0, 0, -1, 1, 1]),
        )]));
        assert!(structure_bounds(&invalid_only).is_none());
    }
}
