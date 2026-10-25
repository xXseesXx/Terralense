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
use std::collections::{HashMap, HashSet};
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
    #[serde(default)]
    pub completed: Vec<ChunkCoverage>,
}

/// Generated chunk slots whose `Status` and `structures.starts` were validated while
/// the existing structure scan decoded each Anvil region. Bits use `local_z * 32 + local_x`.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChunkCoverage {
    pub region_x: i32,
    pub region_z: i32,
    #[serde(default)]
    pub slots: [u64; 16],
}

impl ChunkCoverage {
    fn mark(&mut self, chunk_x: i32, chunk_z: i32) {
        let slot = (chunk_z.rem_euclid(32) * 32 + chunk_x.rem_euclid(32)) as usize;
        self.slots[slot / 64] |= 1u64 << (slot % 64);
    }

    /// True only if this exact chunk passed the saved-chunk completion checks.
    pub fn contains(&self, chunk_x: i32, chunk_z: i32) -> bool {
        if chunk_x.div_euclid(32) != self.region_x || chunk_z.div_euclid(32) != self.region_z {
            return false;
        }
        let slot = (chunk_z.rem_euclid(32) * 32 + chunk_x.rem_euclid(32)) as usize;
        self.slots[slot / 64] & (1u64 << (slot % 64)) != 0
    }
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
    if let Some(warning) = &world.warning
        && warning.starts_with("Could not read level.dat:")
    {
        return Err(warning.clone());
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
                    if dim.path().is_dir()
                        && let Ok(rel) = dim.path().strip_prefix(&dimension_root)
                    {
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
    ids.sort();
    ids.dedup();
    ids.into_iter().filter_map(|id| {
        let storage = dimension_storage(world, &id)?;
        let cfg = configured.and_then(|c| c.get(&id));
        let generator = cfg.and_then(|c| c.get("generator"));
        let supported = match Dimension::from_id(&id) {
            Some(Dimension::Overworld) => true,
            Some(Dimension::Beneath) => has_wrapped_noise_settings(generator, "tfg:nether_noise"),
            Some(Dimension::Moon) => has_wrapped_noise_settings(generator, "tfg:moon_noise"),
            Some(Dimension::Mars) => has_wrapped_noise_settings(generator, "tfg:mars_noise"),
            None => false,
        };
        let (generated, coverage_center) = region_coverage(&storage);
        let player_matches = player.and_then(|p| p.get("Dimension")).and_then(Nbt::string).is_some_and(|d| d == id || (id == "minecraft:overworld" && d == "overworld"));
        let pos = player_matches.then(|| player.and_then(|p| p.get("Pos")).and_then(Nbt::list)).flatten();
        let center = match (pos.and_then(|p| p.first()).and_then(Nbt::f64), pos.and_then(|p| p.get(2)).and_then(Nbt::f64)) {
            (Some(x), Some(z)) => Point { x: x.floor(), z: z.floor() }, _ => coverage_center,
        };
        let name = Dimension::from_id(&id).map(|d| d.label().to_owned()).unwrap_or_else(|| id.clone());
        let warning = (!supported).then(|| format!("Dimension {id} is recognized, but seed preview is not supported for its generator yet."));
        let dimension = Dimension::from_id(&id);
        let min_y = dimension.map_or(-64, Dimension::min_y);
        let height = dimension.map_or(384, Dimension::height);
        Some(DimensionInfo { id, name, path: storage, generated, center, min_y, height, supported, warning })
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

fn has_wrapped_noise_settings(generator: Option<&Nbt>, expected_settings: &str) -> bool {
    let Some(generator) = generator else {
        return false;
    };
    if generator.get("type").and_then(Nbt::string) != Some("kubejs_tfc:wrapped") {
        return false;
    }
    let Some(inner) = generator.get("generator") else {
        return false;
    };
    inner.get("type").and_then(Nbt::string) == Some("minecraft:noise")
        && inner.get("settings").and_then(Nbt::string) == Some(expected_settings)
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
        if let Some(v) = n.get(key).and_then(Nbt::f64)
            && v.is_finite()
        {
            *out = v;
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
    ) && valid_marker_position(x, y, z)
    {
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
    if let Some(values) = player.get("UUID").and_then(Nbt::int_array)
        && values.len() == 4
    {
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
pub(crate) enum Nbt {
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
    pub(crate) fn get(&self, k: &str) -> Option<&Nbt> {
        if let Self::Compound(m) = self {
            m.get(k)
        } else {
            None
        }
    }
    pub(crate) fn string(&self) -> Option<&str> {
        if let Self::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub(crate) fn list(&self) -> Option<&[Nbt]> {
        if let Self::List(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub(crate) fn int_array(&self) -> Option<&[i32]> {
        if let Self::IntArray(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub(crate) fn f64(&self) -> Option<f64> {
        match self {
            Self::Number(v) => Some(*v),
            Self::Long(v) => Some(*v as f64),
            _ => None,
        }
    }
    pub(crate) fn i64(&self) -> Option<i64> {
        match self {
            Self::Long(v) => Some(*v),
            Self::Number(v) if v.is_finite() => Some(*v as i64),
            _ => None,
        }
    }
    pub(crate) fn uuid_string(&self) -> Option<String> {
        nbt_uuid(self)
    }
}

/// Extract the geometry and block-entity inventory for one confirmed saved structure.
/// This function is called by the background engine, never by the UI event loop.
pub(crate) fn read_saved_structure_preview(
    world_path: &Path,
    marker: &Marker,
    cancel: &impl Fn() -> bool,
) -> Result<Option<crate::structure_preview::StructurePreview>, String> {
    use crate::structure_preview::{
        BlockPos, Container, Piece, PreviewSource, ShapeBlock, StructurePreview,
    };
    if marker.kind != MarkerKind::Structure || !marker.confirmed {
        return Ok(None);
    }
    let start_cx = (marker.x.floor() as i32).div_euclid(16);
    let start_cz = (marker.z.floor() as i32).div_euclid(16);
    let Some(start_chunk) = surface::read_chunk_nbt(world_path, start_cx, start_cz)? else {
        return Ok(None);
    };
    let starts = start_chunk
        .get("structures")
        .and_then(|s| s.get("starts"))
        .and_then(Nbt::compound);
    let Some((start_id, start)) = starts.and_then(|starts| {
        starts.iter().find(|(sid, tag)| {
            tag.get("id").and_then(Nbt::string).unwrap_or(sid.as_str()) == marker.id
        })
    }) else {
        return Ok(None);
    };

    let mut preview = StructurePreview::empty(&marker.id, PreviewSource::Saved);
    preview.bounds = structure_bounds(start).or(marker.bounds);
    if let Some(children) = start.get("Children").and_then(Nbt::list) {
        for child in children {
            let Some(bounds) = child.get("BB").and_then(parse_structure_box) else {
                preview.complete = false;
                continue;
            };
            let template = child
                .get("pool_element")
                .and_then(|element| element.get("location"))
                .and_then(nbt_resource_location)
                .or_else(|| child.get("Template").and_then(nbt_resource_location))
                .or_else(|| child.get("template").and_then(nbt_resource_location));
            preview.pieces.push(Piece { template, bounds });
        }
    }
    if preview.pieces.is_empty() {
        if let Some(bounds) = preview.bounds {
            preview.pieces.push(Piece {
                template: None,
                bounds,
            });
            preview
                .notes
                .push("Saved start has aggregate bounds only".into());
        } else {
            return Ok(None);
        }
    }
    let Some(bounds) = preview.bounds else {
        return Ok(None);
    };
    const MAX_CHUNKS: usize = 1024;
    const MAX_BLOCKS: usize = 160_000;
    let min_cx = bounds.min_x.div_euclid(16);
    let max_cx = bounds.max_x.div_euclid(16);
    let min_cz = bounds.min_z.div_euclid(16);
    let max_cz = bounds.max_z.div_euclid(16);
    let total = ((max_cx as i64 - min_cx as i64 + 1).max(0) as usize)
        .saturating_mul((max_cz as i64 - min_cz as i64 + 1).max(0) as usize);
    let mut chunks = Vec::with_capacity(total.min(MAX_CHUNKS));
    if total <= MAX_CHUNKS {
        for cz in min_cz..=max_cz {
            for cx in min_cx..=max_cx {
                chunks.push((cx, cz));
            }
        }
    } else {
        let center = bounds.center();
        let center_x = (center.x.floor() as i32)
            .div_euclid(16)
            .clamp(min_cx, max_cx);
        let center_z = (center.z.floor() as i32)
            .div_euclid(16)
            .clamp(min_cz, max_cz);
        let mut radius = 0i32;
        while chunks.len() < MAX_CHUNKS {
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    if radius > 0 && dx.abs() != radius && dz.abs() != radius {
                        continue;
                    }
                    let cx = center_x.saturating_add(dx);
                    let cz = center_z.saturating_add(dz);
                    if (min_cx..=max_cx).contains(&cx) && (min_cz..=max_cz).contains(&cz) {
                        chunks.push((cx, cz));
                        if chunks.len() == MAX_CHUNKS {
                            break;
                        }
                    }
                }
                if chunks.len() == MAX_CHUNKS {
                    break;
                }
            }
            radius = radius.saturating_add(1);
        }
        preview.complete = false;
        preview.notes.push(format!(
            "Structure spans {total} chunks; preview limited to {MAX_CHUNKS} nearest chunks"
        ));
    }

    let mut missing_chunks = 0usize;
    for (chunk_x, chunk_z) in chunks {
        if cancel() {
            return Err("Inspection cancelled".into());
        }
        let Some(root) = surface::read_chunk_nbt(world_path, chunk_x, chunk_z)? else {
            preview.complete = false;
            missing_chunks += 1;
            continue;
        };
        let data = root.get("Level").unwrap_or(&root);
        if !matches!(
            data.get("Status").and_then(Nbt::string),
            Some("minecraft:full" | "full")
        ) {
            preview.complete = false;
            missing_chunks += 1;
            continue;
        }
        let chunk_min_x = chunk_x.saturating_mul(16);
        let chunk_min_z = chunk_z.saturating_mul(16);
        let local_pieces = preview
            .pieces
            .iter()
            .filter(|piece| {
                piece.bounds.max_x >= chunk_min_x
                    && piece.bounds.min_x <= chunk_min_x.saturating_add(15)
                    && piece.bounds.max_z >= chunk_min_z
                    && piece.bounds.min_z <= chunk_min_z.saturating_add(15)
            })
            .collect::<Vec<_>>();
        let mut block_names = HashMap::<BlockPos, String>::new();
        let sections = data
            .get("sections")
            .or_else(|| data.get("Sections"))
            .and_then(Nbt::list);
        if let Some(sections) = sections {
            for section in sections {
                if cancel() {
                    return Err("Inspection cancelled".into());
                }
                let Some(section_y) = section.get("Y").and_then(Nbt::i64).map(|n| n as i32) else {
                    continue;
                };
                let section_base_y = section_y.saturating_mul(16);
                if section_base_y > bounds.max_y || section_base_y.saturating_add(15) < bounds.min_y
                {
                    continue;
                }
                let (palette, packed) = if let Some(states) = section.get("block_states") {
                    (
                        states.get("palette").and_then(Nbt::list),
                        states.get("data").and_then(Nbt::long_array),
                    )
                } else {
                    (
                        section.get("Palette").and_then(Nbt::list),
                        section.get("BlockStates").and_then(Nbt::long_array),
                    )
                };
                let Some(palette) = palette else { continue };
                let names = palette
                    .iter()
                    .filter_map(|p| p.get("Name").and_then(Nbt::string).map(str::to_owned))
                    .collect::<Vec<_>>();
                if names.len() != palette.len() || names.is_empty() {
                    continue;
                }
                let bits =
                    (usize::BITS - (names.len().saturating_sub(1)).leading_zeros()).max(4) as usize;
                let per_long = 64 / bits;
                let mask = (1u64 << bits) - 1;
                if names.len() > 1 && packed.is_none_or(|v| v.len() < 4096usize.div_ceil(per_long))
                {
                    preview.complete = false;
                    continue;
                }
                for index in 0..4096usize {
                    let palette_index = if names.len() == 1 {
                        0
                    } else {
                        let data = packed.unwrap();
                        ((data[index / per_long] as u64 >> ((index % per_long) * bits)) & mask)
                            as usize
                    };
                    let Some(name) = names.get(palette_index) else {
                        continue;
                    };
                    if is_air_block(name) {
                        continue;
                    }
                    let x = chunk_x.saturating_mul(16) + (index % 16) as i32;
                    let z = chunk_z.saturating_mul(16) + ((index / 16) % 16) as i32;
                    let y = section_base_y.saturating_add((index / 256) as i32);
                    if !local_pieces.iter().any(|piece| {
                        piece.bounds.contains_column(x, z)
                            && (piece.bounds.min_y..=piece.bounds.max_y).contains(&y)
                    }) {
                        continue;
                    }
                    let pos = BlockPos { x, y, z };
                    if is_container(name, "") {
                        block_names.insert(pos, name.clone());
                    }
                    if preview.blocks.len() >= MAX_BLOCKS {
                        preview.complete = false;
                        continue;
                    }
                    preview.blocks.push(ShapeBlock {
                        pos,
                        block: name.clone(),
                        color: preview_tint(name),
                    });
                }
            }
        }
        let mut known_container_positions = HashSet::new();
        let Some(block_entities) = data
            .get("block_entities")
            .or_else(|| data.get("TileEntities"))
            .and_then(Nbt::list)
        else {
            for (pos, block) in block_names {
                let mut container = Container::new(pos, block);
                container
                    .notes
                    .push("No saved block entity data; contents are unknown.".into());
                container.dimension = Some(storage_dimension_id(world_path));
                preview.containers.push(container);
            }
            continue;
        };
        for be in block_entities {
            let pos = if let Some(a) = be
                .get("Pos")
                .and_then(Nbt::int_array)
                .filter(|a| a.len() == 3)
            {
                BlockPos {
                    x: a[0],
                    y: a[1],
                    z: a[2],
                }
            } else if let (Some(x), Some(y), Some(z)) = (
                be.get("x")
                    .and_then(Nbt::i64)
                    .and_then(|n| n.try_into().ok()),
                be.get("y")
                    .and_then(Nbt::i64)
                    .and_then(|n| n.try_into().ok()),
                be.get("z")
                    .and_then(Nbt::i64)
                    .and_then(|n| n.try_into().ok()),
            ) {
                BlockPos { x, y, z }
            } else {
                continue;
            };
            if !local_pieces.iter().any(|piece| {
                piece.bounds.contains_column(pos.x, pos.z)
                    && (piece.bounds.min_y..=piece.bounds.max_y).contains(&pos.y)
            }) {
                continue;
            }
            let be_id = be.get("id").and_then(Nbt::string).unwrap_or("");
            let block = block_names
                .get(&pos)
                .cloned()
                .unwrap_or_else(|| be_id.to_owned());
            if !is_container(&block, be_id) {
                continue;
            }
            known_container_positions.insert(pos);
            let mut container = Container::new(pos, block.clone());
            container.dimension = Some(storage_dimension_id(world_path));
            container.lootr = block.starts_with("lootr:") || be_id.starts_with("lootr:");
            container.lootr_id = ["tileId", "TileId", "tile_id", "LootrId", "lootrId"]
                .iter()
                .find_map(|key| be.get(key).and_then(nbt_uuid));
            container.loot_table = be
                .get("LootTable")
                .or_else(|| be.get("lootrLootTable"))
                .or_else(|| be.get("specialLootChest_table"))
                .and_then(Nbt::string)
                .map(str::to_owned);
            container.loot_seed = be
                .get("LootTableSeed")
                .or_else(|| be.get("specialLootChest_seed"))
                .and_then(Nbt::i64);
            let inventory = be.get("inventory").or_else(|| be.get("Inventory"));
            if let Some(items) = be
                .get("Items")
                .or_else(|| inventory.and_then(|i| i.get("Items")))
                .and_then(Nbt::list)
            {
                container.items = items.iter().filter_map(preview_item).collect();
                container.contents_known = container.loot_table.is_none() && !container.lootr;
            } else if container.loot_table.is_none() && !container.lootr {
                // A known empty vanilla inventory is distinct from an ungenerated table.
                container.contents_known = true;
            }
            preview.containers.push(container);
        }
        for (pos, block) in block_names {
            if known_container_positions.contains(&pos) {
                continue;
            }
            let mut container = Container::new(pos, block);
            container.dimension = Some(storage_dimension_id(world_path));
            container
                .notes
                .push("No saved block entity data; contents are unknown.".into());
            preview.containers.push(container);
        }
    }
    preview
        .blocks
        .sort_unstable_by_key(|b| (b.pos.y, b.pos.z, b.pos.x));
    preview
        .containers
        .sort_unstable_by_key(|c| (c.pos.y, c.pos.z, c.pos.x));
    if preview.blocks.len() >= MAX_BLOCKS {
        preview.complete = false;
        preview
            .notes
            .push(format!("Shape limited to {MAX_BLOCKS} visible blocks"));
    }
    if missing_chunks > 0 {
        preview.notes.push(format!(
            "{missing_chunks} structure chunks are not fully generated in this save"
        ));
    }
    let _ = start_id;
    Ok(Some(preview))
}

fn nbt_resource_location(node: &Nbt) -> Option<String> {
    node.string()
        .or_else(|| node.get("id").and_then(Nbt::string))
        .or_else(|| node.get("Name").and_then(Nbt::string))
        .filter(|id| id.contains(':') || id.contains('/'))
        .map(str::to_owned)
}

fn parse_structure_box(node: &Nbt) -> Option<crate::model::StructureBounds> {
    let values: [i32; 6] = if let Some(values) = node.int_array() {
        if values.len() != 6 {
            return None;
        }
        values.try_into().ok()?
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
    (values[0] <= values[3] && values[1] <= values[4] && values[2] <= values[5]).then_some(
        crate::model::StructureBounds {
            min_x: values[0],
            min_y: values[1],
            min_z: values[2],
            max_x: values[3],
            max_y: values[4],
            max_z: values[5],
        },
    )
}

fn is_air_block(name: &str) -> bool {
    matches!(
        name.rsplit(':').next().unwrap_or(name),
        "air" | "cave_air" | "void_air"
    )
}

fn is_container(block: &str, be_id: &str) -> bool {
    let tokens = format!("{block} {be_id}")
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    [
        "chest",
        "barrel",
        "vessel",
        "vase",
        "urn",
        "jar",
        "crate",
        "shulker",
        "pot",
        "basket",
        "cabinet",
        "coffer",
        "sarcophagus",
    ]
    .iter()
    .any(|needle| tokens.iter().any(|part| part == needle))
        || tokens
            .windows(2)
            .any(|pair| pair == ["suspicious", "sand"] || pair == ["suspicious", "gravel"])
}

pub(crate) fn preview_item(tag: &Nbt) -> Option<crate::structure_preview::ItemStack> {
    let id = tag.get("id")?.string()?.to_owned();
    let count = u32::try_from(tag.get("Count").or_else(|| tag.get("count"))?.i64()?).ok()?;
    if count == 0 {
        return None;
    }
    let slot = tag
        .get("Slot")
        .and_then(Nbt::i64)
        .and_then(|n| u16::try_from(n).ok());
    Some(crate::structure_preview::ItemStack {
        id,
        count,
        slot,
        description: item_metadata_description(tag),
    })
}

fn item_metadata_description(item: &Nbt) -> String {
    let mut parts = Vec::<String>::new();
    let legacy = item.get("tag");
    let components = item.get("components");
    let display_name = legacy
        .and_then(|tag| tag.get("display"))
        .and_then(|display| display.get("Name"))
        .and_then(Nbt::string)
        .or_else(|| {
            components
                .and_then(|c| c.get("minecraft:custom_name"))
                .and_then(Nbt::string)
        })
        .or_else(|| {
            components
                .and_then(|c| c.get("custom_name"))
                .and_then(Nbt::string)
        })
        .map(parse_display_name)
        .filter(|name| !name.is_empty());
    if let Some(name) = display_name {
        push_item_note(&mut parts, format!("Name: {name}"));
    }

    if let Some(botarium) = find_named(item, "BotariumData")
        && let Some(fluids) = botarium.get("StoredFluids")
    {
        append_stored_fluids(&mut parts, fluids);
    }
    // Botarium has used both direct item NBT and component custom-data storage.
    if !parts.iter().any(|note| note.contains("mB ")) {
        for key in ["StoredFluids", "botarium:stored_fluids"] {
            if let Some(fluids) = find_named(item, key) {
                append_stored_fluids(&mut parts, fluids);
                if parts.iter().any(|note| note.contains("mB ")) {
                    break;
                }
            }
        }
    }

    let damage = item
        .get("Damage")
        .and_then(Nbt::i64)
        .or_else(|| legacy.and_then(|tag| tag.get("Damage")).and_then(Nbt::i64))
        .or_else(|| {
            components
                .and_then(|c| c.get("minecraft:damage"))
                .and_then(Nbt::i64)
        });
    if let Some(damage) = damage.filter(|v| *v > 0) {
        push_item_note(&mut parts, format!("Damage: {damage}"));
    }
    append_enchantments(&mut parts, legacy.and_then(|t| t.get("Enchantments")));
    append_enchantments(&mut parts, legacy.and_then(|t| t.get("StoredEnchantments")));
    if let Some(enchantments) = components.and_then(|c| c.get("minecraft:enchantments")) {
        append_component_enchantments(&mut parts, enchantments);
    }
    if let Some(enchantments) = components.and_then(|c| c.get("minecraft:stored_enchantments")) {
        append_component_enchantments(&mut parts, enchantments);
    }
    append_tfc_traits(&mut parts, item);

    let joined = parts.join(" · ");
    joined.chars().take(512).collect()
}

fn parse_display_name(raw: &str) -> String {
    fn flatten(value: &serde_json::Value, out: &mut String, depth: u8) {
        if depth > 8 || out.len() >= 120 {
            return;
        }
        match value {
            serde_json::Value::String(s) => out.push_str(s),
            serde_json::Value::Array(items) => {
                for item in items.iter().take(16) {
                    flatten(item, out, depth + 1);
                }
            }
            serde_json::Value::Object(fields) => {
                if let Some(text) = fields.get("text") {
                    flatten(text, out, depth + 1);
                }
                if let Some(translate) = fields.get("translate").and_then(serde_json::Value::as_str)
                    && out.is_empty()
                {
                    out.push_str(translate);
                }
                if let Some(extra) = fields.get("extra") {
                    flatten(extra, out, depth + 1);
                }
            }
            _ => {}
        }
    }
    let mut name = String::new();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) {
        flatten(&json, &mut name, 0);
    }
    if name.is_empty() {
        name.push_str(raw);
    }
    name.chars()
        .filter(|ch| !ch.is_control())
        .take(120)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn push_item_note(parts: &mut Vec<String>, note: String) {
    if parts.len() >= 12 || note.is_empty() {
        return;
    }
    let remaining =
        512usize.saturating_sub(parts.iter().map(String::len).sum::<usize>() + 3 * parts.len());
    let clipped = note.chars().take(remaining.min(160)).collect::<String>();
    if !clipped.is_empty() {
        parts.push(clipped);
    }
}

/// Find an exact NBT key in a shallow, bounded subtree. This is used only for known
/// metadata fields, never to serialize arbitrary item NBT into the UI.
fn find_named<'a>(root: &'a Nbt, wanted: &str) -> Option<&'a Nbt> {
    fn visit<'a>(node: &'a Nbt, key: &str, depth: u8, budget: &mut usize) -> Option<&'a Nbt> {
        if depth > 8 || *budget == 0 {
            return None;
        }
        *budget -= 1;
        match node {
            Nbt::Compound(fields) => {
                if let Some(value) = fields.get(key) {
                    return Some(value);
                }
                let mut keys = fields.keys().collect::<Vec<_>>();
                keys.sort_unstable();
                for name in keys.into_iter().take(128) {
                    if let Some(value) = fields
                        .get(name)
                        .and_then(|child| visit(child, key, depth + 1, budget))
                    {
                        return Some(value);
                    }
                }
                None
            }
            Nbt::List(items) => items
                .iter()
                .take(64)
                .find_map(|child| visit(child, key, depth + 1, budget)),
            _ => None,
        }
    }
    visit(root, wanted, 0, &mut 1024)
}

fn append_stored_fluids(parts: &mut Vec<String>, fluids: &Nbt) {
    fn add(parts: &mut Vec<String>, entry: &Nbt) {
        let fluid = entry.get("Fluid").or_else(|| entry.get("fluid"));
        let id = fluid
            .and_then(Nbt::string)
            .or_else(|| fluid.and_then(|f| f.get("id")).and_then(Nbt::string))
            .or_else(|| fluid.and_then(|f| f.get("Name")).and_then(Nbt::string))
            .or_else(|| entry.get("FluidName").and_then(Nbt::string));
        let amount = entry
            .get("Amount")
            .or_else(|| entry.get("amount"))
            .and_then(Nbt::i64);
        if let (Some(id), Some(amount)) = (id, amount.filter(|amount| *amount > 0)) {
            push_item_note(parts, format!("Fluid: {amount} mB {id}"));
        }
    }
    if let Some(entries) = fluids.list() {
        for entry in entries.iter().take(8) {
            add(parts, entry);
        }
    } else if let Some(entries) = fluids.compound() {
        let mut keys = entries.keys().collect::<Vec<_>>();
        keys.sort_unstable();
        for key in keys.into_iter().take(8) {
            if let Some(entry) = entries.get(key) {
                add(parts, entry);
            }
        }
    } else {
        add(parts, fluids);
    }
}

fn append_enchantments(parts: &mut Vec<String>, node: Option<&Nbt>) {
    let Some(entries) = node.and_then(Nbt::list) else {
        return;
    };
    for enchant in entries.iter().take(8) {
        let id = enchant.get("id").and_then(Nbt::string).unwrap_or("");
        let level = enchant
            .get("lvl")
            .or_else(|| enchant.get("level"))
            .and_then(Nbt::i64);
        if !id.is_empty()
            && let Some(level) = level
        {
            push_item_note(
                parts,
                format!(
                    "Enchantment: {} {level}",
                    id.rsplit(':').next().unwrap_or(id)
                ),
            );
        }
    }
}

fn append_component_enchantments(parts: &mut Vec<String>, component: &Nbt) {
    let Some(levels) = component
        .get("levels")
        .or_else(|| component.get("Levels"))
        .and_then(Nbt::compound)
    else {
        return;
    };
    let mut keys = levels.keys().collect::<Vec<_>>();
    keys.sort_unstable();
    for id in keys.into_iter().take(8) {
        if let Some(level) = levels.get(id).and_then(Nbt::i64) {
            push_item_note(
                parts,
                format!(
                    "Enchantment: {} {level}",
                    id.rsplit(':').next().unwrap_or(id)
                ),
            );
        }
    }
}

fn append_tfc_traits(parts: &mut Vec<String>, item: &Nbt) {
    const KEYS: &[&str] = &[
        "food",
        "nutrients",
        "rot",
        "rotProgress",
        "rot_progress",
        "freshness",
        "purity",
        "work",
        "metal",
        "temperature",
        "heat",
        "protein",
        "carbohydrate",
        "lipid",
        "grain",
        "fruit",
        "vegetable",
        "vegetables",
        "dairy",
        "meat",
        "saturation",
        "rotDate",
        "creationDate",
    ];
    fn visit(node: &Nbt, depth: u8, budget: &mut usize, parts: &mut Vec<String>) {
        if depth > 7 || *budget == 0 || parts.len() >= 12 {
            return;
        }
        *budget -= 1;
        if let Some(fields) = node.compound() {
            let mut keys = fields.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for key in keys.into_iter().take(96) {
                let Some(value) = fields.get(key) else {
                    continue;
                };
                if KEYS.iter().any(|known| key.eq_ignore_ascii_case(known)) {
                    if let Some(number) = value.f64().filter(|n| n.is_finite()) {
                        push_item_note(parts, format!("{}: {:.2}", key, number));
                    } else if let Some(text) = value.string() {
                        push_item_note(
                            parts,
                            format!("{}: {}", key, text.chars().take(64).collect::<String>()),
                        );
                    }
                }
                if depth < 7 {
                    visit(value, depth + 1, budget, parts);
                }
            }
        } else if let Some(items) = node.list() {
            for child in items.iter().take(32) {
                visit(child, depth + 1, budget, parts);
            }
        }
    }
    visit(item, 0, &mut 1024, parts);
}

fn nbt_uuid(tag: &Nbt) -> Option<String> {
    if let Some(s) = tag.string() {
        return valid_uuid(s).then(|| normalized_uuid(s));
    }
    let values = tag.int_array()?;
    if values.len() != 4 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (i, value) in values.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    let tail = bytes[10..16]
        .iter()
        .fold(0u64, |n, byte| (n << 8) | *byte as u64);
    Some(format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes(bytes[0..4].try_into().ok()?),
        u16::from_be_bytes(bytes[4..6].try_into().ok()?),
        u16::from_be_bytes(bytes[6..8].try_into().ok()?),
        u16::from_be_bytes(bytes[8..10].try_into().ok()?),
        tail
    ))
}

fn preview_tint(block: &str) -> [u8; 3] {
    let path = block.rsplit_once(':').map_or(block, |(_, p)| p);
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.contains("wood") || name.contains("log") || name.contains("plank") {
        [132, 96, 62]
    } else if name.contains("mud") || name.contains("dirt") || name.contains("soil") {
        [126, 93, 67]
    } else if name.contains("sand") || name.contains("gravel") {
        [194, 177, 128]
    } else if name.contains("water") || name.contains("ice") {
        [76, 137, 176]
    } else if name.contains("leaf") || name.contains("grass") {
        [99, 145, 67]
    } else if name.contains("brick") || name.contains("stone") || name.contains("rock") {
        [119, 116, 109]
    } else {
        [133, 128, 113]
    }
}

fn storage_dimension_id(path: &Path) -> String {
    let components = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>();
    if components
        .windows(2)
        .any(|w| w[0] == "dimensions" && w[1] == "ad_astra")
    {
        if components.iter().any(|c| c == "moon") {
            "ad_astra:moon".into()
        } else if components.iter().any(|c| c == "mars") {
            "ad_astra:mars".into()
        } else {
            "minecraft:overworld".into()
        }
    } else if components.iter().any(|c| c == "DIM-1") {
        "minecraft:the_nether".into()
    } else if components.iter().any(|c| c == "DIM1") {
        "minecraft:the_end".into()
    } else {
        "minecraft:overworld".into()
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
                if !(0..=2_000_000).contains(&n) || (kind == 0 && n > 0) {
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
    GroundChunk, GroundColumn, SurfaceChunk, SurfaceColumn, clip_surface_chunk, read_biome_chunk,
    read_ground_tile, read_surface_band, read_surface_slice, read_surface_tile,
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
pub(crate) fn read_nbt_file(path: &Path) -> Result<Nbt, String> {
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
    if let Some(c) = cache.lock().unwrap().get(world)
        && c.stamps == stamps
    {
        return Ok(c.scan.clone());
    }
    let results = paths.par_iter().map(|p| scan_region(p)).collect::<Vec<_>>();
    let mut scan = StructureScan::default();
    for (mut markers, chunks, errors, completed) in results {
        scan.markers.append(&mut markers);
        scan.chunks += chunks;
        scan.errors.extend(errors);
        if let Some(completed) = completed {
            scan.completed.push(completed);
        }
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

fn scan_region(path: &Path) -> (Vec<Marker>, usize, Vec<String>, Option<ChunkCoverage>) {
    let mut markers = Vec::new();
    let mut chunks = 0;
    let mut errors = Vec::new();
    let (region_x, region_z) = region_coords(path).unwrap_or((0, 0));
    let known_region = region_coords(path).is_some();
    let mut coverage = known_region.then_some(ChunkCoverage {
        region_x,
        region_z,
        slots: [0; 16],
    });
    let data = match fs::read(path) {
        Ok(v) => v,
        Err(e) => return (markers, 0, vec![format!("{}: {e}", path.display())], None),
    };
    if data.is_empty() {
        return (markers, 0, errors, None);
    }
    if data.len() < 4096 {
        return (
            markers,
            0,
            vec![format!("{}: short Anvil header", path.display())],
            None,
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
                if let Some(bitmap) = coverage.as_mut()
                    && chunk_has_complete_structure_metadata(&nbt, region_x, region_z, i)
                {
                    bitmap.mark(
                        region_x * 32 + (i % 32) as i32,
                        region_z * 32 + (i / 32) as i32,
                    );
                }
            }
            Err(e) => errors.push(format!("{} slot {i}: {e}", path.display())),
        }
    }
    let coverage = coverage.filter(|entry| entry.slots.iter().any(|word| *word != 0));
    (markers, chunks, errors, coverage)
}

fn region_coords(path: &Path) -> Option<(i32, i32)> {
    let stem = path.file_stem()?.to_str()?.strip_prefix("r.")?;
    let (x, z) = stem.split_once('.')?;
    Some((x.parse().ok()?, z.parse().ok()?))
}

fn chunk_has_complete_structure_metadata(
    root: &Nbt,
    region_x: i32,
    region_z: i32,
    slot: usize,
) -> bool {
    let expected_x = (region_x as i64) * 32 + (slot % 32) as i64;
    let expected_z = (region_z as i64) * 32 + (slot / 32) as i64;
    if expected_x < i32::MIN as i64
        || expected_x > i32::MAX as i64
        || expected_z < i32::MIN as i64
        || expected_z > i32::MAX as i64
    {
        return false;
    }
    let data = root.get("Level").unwrap_or(root);
    let full = matches!(
        data.get("Status").and_then(Nbt::string),
        Some("minecraft:full" | "full")
    );
    let coordinates_match = data.get("xPos").and_then(Nbt::i64) == Some(expected_x)
        && data.get("zPos").and_then(Nbt::i64) == Some(expected_z);
    let starts_present = data
        .get("structures")
        .and_then(|structures| structures.get("starts"))
        .and_then(Nbt::compound)
        .is_some();
    full && coordinates_match && starts_present
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
    pub(crate) fn compound(&self) -> Option<&HashMap<String, Nbt>> {
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
    fn supported_planets_require_exact_wrapped_tfg_noise_profiles() {
        let generator = |settings: &str| {
            Nbt::Compound(HashMap::from([
                ("type".into(), Nbt::String("kubejs_tfc:wrapped".into())),
                (
                    "generator".into(),
                    Nbt::Compound(HashMap::from([
                        ("type".into(), Nbt::String("minecraft:noise".into())),
                        ("settings".into(), Nbt::String(settings.into())),
                    ])),
                ),
            ]))
        };
        assert!(has_wrapped_noise_settings(
            Some(&generator("tfg:nether_noise")),
            "tfg:nether_noise"
        ));
        assert!(has_wrapped_noise_settings(
            Some(&generator("tfg:moon_noise")),
            "tfg:moon_noise"
        ));
        assert!(has_wrapped_noise_settings(
            Some(&generator("tfg:mars_noise")),
            "tfg:mars_noise"
        ));
        assert!(!has_wrapped_noise_settings(
            Some(&generator("ad_astra:moon")),
            "tfg:moon_noise"
        ));
        assert!(!has_wrapped_noise_settings(
            Some(&generator("tfg:mars_noise")),
            "tfg:moon_noise"
        ));
        assert!(!has_wrapped_noise_settings(None, "tfg:moon_noise"));
        let missing_inner = Nbt::Compound(HashMap::from([(
            "type".into(),
            Nbt::String("kubejs_tfc:wrapped".into()),
        )]));
        assert!(!has_wrapped_noise_settings(
            Some(&missing_inner),
            "tfg:moon_noise"
        ));
        let stock = Nbt::Compound(HashMap::from([
            ("type".into(), Nbt::String("minecraft:noise".into())),
            ("settings".into(), Nbt::String("minecraft:moon".into())),
        ]));
        assert!(!has_wrapped_noise_settings(Some(&stock), "tfg:moon_noise"));
        let wrong_inner = Nbt::Compound(HashMap::from([
            ("type".into(), Nbt::String("kubejs_tfc:wrapped".into())),
            (
                "generator".into(),
                Nbt::Compound(HashMap::from([
                    ("type".into(), Nbt::String("ad_astra:moon".into())),
                    ("settings".into(), Nbt::String("tfg:moon_noise".into())),
                ])),
            ),
        ]));
        assert!(!has_wrapped_noise_settings(
            Some(&wrong_inner),
            "tfg:moon_noise"
        ));
        let world = Path::new("/tmp/save");
        assert_eq!(
            dimension_storage(world, "minecraft:the_nether"),
            Some(world.join("DIM-1"))
        );
        assert!(dimension_storage(world, "minecraft:../../outside").is_none());
    }

    #[test]
    fn dimension_discovery_applies_planet_profiles_storage_paths_and_heights() {
        let world =
            std::env::temp_dir().join(format!("terralens-planet-dim-test-{}", std::process::id()));
        fs::create_dir_all(&world).unwrap();
        let wrapped = |settings: &str| {
            Nbt::Compound(HashMap::from([
                ("type".into(), Nbt::String("kubejs_tfc:wrapped".into())),
                (
                    "generator".into(),
                    Nbt::Compound(HashMap::from([
                        ("type".into(), Nbt::String("minecraft:noise".into())),
                        ("settings".into(), Nbt::String(settings.into())),
                    ])),
                ),
            ]))
        };
        let dimension =
            |generator: Nbt| Nbt::Compound(HashMap::from([("generator".into(), generator)]));
        let configured = Nbt::Compound(HashMap::from([
            ("ad_astra:moon".into(), dimension(wrapped("tfg:moon_noise"))),
            ("ad_astra:mars".into(), dimension(wrapped("tfg:mars_noise"))),
            (
                "ad_astra:moon_orbit".into(),
                dimension(wrapped("tfg:moon_noise")),
            ),
            (
                "ad_astra:venus".into(),
                dimension(wrapped("ad_astra:venus")),
            ),
        ]));
        let infos = discover_dimensions(&world, Some(&configured), None);
        let get = |id: &str| infos.iter().find(|info| info.id == id).unwrap();
        assert!(get("ad_astra:moon").supported);
        assert_eq!(
            get("ad_astra:moon").path,
            world.join("dimensions/ad_astra/moon")
        );
        assert_eq!(
            (get("ad_astra:moon").min_y, get("ad_astra:moon").height),
            (0, 384)
        );
        assert!(get("ad_astra:mars").supported);
        assert_eq!(
            (get("ad_astra:mars").min_y, get("ad_astra:mars").height),
            (-64, 384)
        );
        assert!(!get("ad_astra:moon_orbit").supported);
        assert!(!get("ad_astra:venus").supported);
        fs::remove_dir_all(world).unwrap();
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
    fn write_region_records(path: &Path, records: &[(usize, Vec<u8>, u8)]) {
        let mut region = vec![0u8; 8192];
        let mut next_sector = 2u32;
        for (slot, raw, compression) in records {
            let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
            gzip.write_all(raw).unwrap();
            let compressed = gzip.finish().unwrap();
            let length = compressed.len() + 1;
            let sector_count = (length + 4).div_ceil(4096);
            let header_offset = slot * 4;
            region[header_offset] = (next_sector >> 16) as u8;
            region[header_offset + 1] = (next_sector >> 8) as u8;
            region[header_offset + 2] = next_sector as u8;
            region[header_offset + 3] = sector_count as u8;
            let base = next_sector as usize * 4096;
            region.resize((next_sector as usize + sector_count) * 4096, 0);
            region[base..base + 4].copy_from_slice(&(length as u32).to_be_bytes());
            region[base + 4] = *compression;
            region[base + 5..base + 5 + compressed.len()].copy_from_slice(&compressed);
            next_sector += sector_count as u32;
        }
        fs::write(path, region).unwrap();
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
            let mut chunk = string_tag("Status", "minecraft:full");
            chunk.extend(int_tag("xPos", 0));
            chunk.extend(int_tag("zPos", 0));
            chunk.extend(structures);
            let raw = compound("", chunk);
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
        assert!(
            over.completed
                .iter()
                .any(|coverage| coverage.contains(0, 0))
        );
        assert_eq!(nether.markers.len(), 1);
        assert_eq!(nether.markers[0].id, "mod_b:beneath_site");
        assert_eq!(nether.markers[0].bounds.unwrap().max_y, 90);
        assert!(
            nether
                .completed
                .iter()
                .any(|coverage| coverage.contains(0, 0))
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn chunk_completion_bitmap_requires_full_matching_chunk_and_starts_metadata() {
        let nonce = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("terralens-coverage-{}-{nonce}", std::process::id()));
        let region_dir = root.join("region");
        fs::create_dir_all(&region_dir).unwrap();

        let valid_negative = {
            let mut body = string_tag("Status", "minecraft:full");
            body.extend(int_tag("xPos", -1));
            body.extend(int_tag("zPos", -1));
            body.extend(compound("structures", compound("starts", Vec::new())));
            compound("", body)
        };
        write_region_records(
            &region_dir.join("r.-1.-1.mca"),
            &[(1023, valid_negative, 1)],
        );

        let chunk = |x, z, status: &str, has_starts: bool| {
            let mut body = string_tag("Status", status);
            body.extend(int_tag("xPos", x));
            body.extend(int_tag("zPos", z));
            if has_starts {
                body.extend(compound("structures", compound("starts", Vec::new())));
            }
            compound("", body)
        };
        write_region_records(
            &region_dir.join("r.0.0.mca"),
            &[
                // Full bit is required; a partial chunk remains unknown.
                (0, chunk(0, 0, "minecraft:features", true), 1),
                // A full chunk in header slot 1 but reporting xPos 2 is corrupt/mismatched.
                (1, chunk(2, 0, "minecraft:full", true), 1),
                // Correct coordinates and status, but no starts compound.
                (2, chunk(2, 0, "minecraft:full", false), 1),
                // A present but undecodable record is not authoritative.
                (3, chunk(3, 0, "minecraft:full", true), 9),
                // Slot 4 has no header record at all.
            ],
        );

        let scan = scan_structures(&root).unwrap();
        assert_eq!(
            scan.completed.len(),
            1,
            "only the negative region has a valid bit"
        );
        let coverage = &scan.completed[0];
        assert_eq!((coverage.region_x, coverage.region_z), (-1, -1));
        assert!(
            coverage.contains(-1, -1),
            "region slot 1023 uses local z*32+x"
        );
        assert!(!coverage.contains(0, 0));
        assert!(!coverage.contains(-2, -1));
        assert!(!coverage.contains(1, 0));
        assert!(!coverage.contains(2, 0));
        assert!(!coverage.contains(3, 0));
        assert!(!coverage.contains(4, 0));
        assert!(
            scan.errors
                .iter()
                .any(|error| error.contains("unknown chunk compression"))
        );
        fs::remove_dir_all(root).unwrap();
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
