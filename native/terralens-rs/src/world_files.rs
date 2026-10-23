//! Import saved worlds from arbitrary filesystem locations without modifying them.
use crate::dimension::Dimension;
use crate::model::WorldInfo;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A resolved file/folder source and the supported worlds found beneath it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenedSource {
    pub source: PathBuf,
    pub worlds: Vec<WorldInfo>,
    pub label: String,
}

/// Accept a world folder, its level.dat, a Prism instance, or a Minecraft saves folder.
/// Every path is canonicalized and all reads remain local and read-only.
pub fn resolve_source(path: &Path) -> Result<OpenedSource, String> {
    let selected =
        fs::canonicalize(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    if selected.is_file() {
        if selected.file_name().is_none_or(|name| name != "level.dat") {
            return Err(format!(
                "Select a world folder or its level.dat file: {}",
                selected.display()
            ));
        }
        let world_path = selected.parent().ok_or("level.dat has no parent folder")?;
        let world = import_world(world_path)?;
        return Ok(opened(world_path.to_path_buf(), vec![world], None));
    }
    if !selected.is_dir() {
        return Err(format!("{} is not a file or folder", selected.display()));
    }
    if selected.join("level.dat").is_file() {
        let world = import_world(&selected)?;
        return Ok(opened(selected, vec![world], None));
    }

    let saves = if is_saves_directory(&selected) {
        Some(selected.clone())
    } else if selected.join("saves").is_dir() {
        Some(selected.join("saves"))
    } else {
        ["minecraft", ".minecraft"]
            .iter()
            .map(|child| selected.join(child).join("saves"))
            .find(|candidate| candidate.is_dir())
    };
    let Some(saves) = saves else {
        return Err(format!(
            "No level.dat or recognized Prism/Minecraft saves folder under {}",
            selected.display()
        ));
    };
    let mut folders = fs::read_dir(&saves)
        .map_err(|e| format!("Cannot list {}: {e}", saves.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| candidate.is_dir() && candidate.join("level.dat").is_file())
        .collect::<Vec<_>>();
    folders.sort();
    if folders.is_empty() {
        return Err(format!(
            "No world folders containing level.dat in {}",
            saves.display()
        ));
    }
    let mut worlds = Vec::with_capacity(folders.len());
    let mut rejected = Vec::new();
    for folder in folders {
        match import_world(&folder) {
            Ok(world) => worlds.push(world),
            Err(error) => rejected.push(error),
        }
    }
    if worlds.is_empty() {
        return Err(format!(
            "No supported worlds found in {}. {}",
            saves.display(),
            rejected.join("; ")
        ));
    }
    Ok(opened(selected, worlds, Some("Minecraft saves")))
}

fn opened(source: PathBuf, worlds: Vec<WorldInfo>, kind: Option<&str>) -> OpenedSource {
    let label = kind.map(str::to_owned).unwrap_or_else(|| {
        worlds
            .first()
            .map(|world| world.name.clone())
            .unwrap_or_else(|| source.display().to_string())
    });
    OpenedSource {
        source,
        worlds,
        label,
    }
}

fn import_world(path: &Path) -> Result<WorldInfo, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| format!("Cannot resolve world {}: {e}", path.display()))?;
    let mut world = crate::prism::load_world(&canonical)?;
    let beneat_supported = world
        .dimensions
        .iter()
        .any(|dimension| dimension.id == Dimension::Beneath.id() && dimension.supported);
    if !world.compatible && !beneat_supported {
        return Err(world.warning.clone().unwrap_or_else(|| {
            format!(
                "{} does not match a supported TFG worldgen profile",
                world.name
            )
        }));
    }
    world.id = format!(
        "imported-{:x}",
        md5::compute(canonical.as_os_str().as_encoded_bytes())
    );
    world.path = canonical;
    Ok(world)
}

fn is_saves_directory(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "saves")
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    fn name(value: &str) -> Vec<u8> {
        let mut bytes = (value.len() as u16).to_be_bytes().to_vec();
        bytes.extend(value.as_bytes());
        bytes
    }
    fn string_tag(key: &str, value: &str) -> Vec<u8> {
        let mut bytes = vec![8];
        bytes.extend(name(key));
        bytes.extend((value.len() as u16).to_be_bytes());
        bytes.extend(value.as_bytes());
        bytes
    }
    fn long_tag(key: &str, value: i64) -> Vec<u8> {
        let mut bytes = vec![4];
        bytes.extend(name(key));
        bytes.extend(value.to_be_bytes());
        bytes
    }
    fn compound(key: &str, mut payload: Vec<u8>) -> Vec<u8> {
        let mut bytes = vec![10];
        bytes.extend(name(key));
        payload.push(0);
        bytes.extend(payload);
        bytes
    }
    fn write_world(path: &Path, name_value: &str, seed: i64) {
        fs::create_dir_all(path).unwrap();
        let dims = string_tag("type", "tfc:overworld");
        let generator = compound("generator", dims);
        let overworld = compound("minecraft:overworld", generator);
        let dimensions = compound("dimensions", overworld);
        let mut worldgen = long_tag("seed", seed);
        worldgen.extend(dimensions);
        let mut data = string_tag("LevelName", name_value);
        data.extend(compound("WorldGenSettings", worldgen));
        let raw = compound("Data", data);
        let mut root = vec![10];
        root.extend(name(""));
        root.extend(raw);
        root.push(0);
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(&root).unwrap();
        fs::write(path.join("level.dat"), gzip.finish().unwrap()).unwrap();
        fs::create_dir_all(path.join("data")).unwrap();
        let version = compound("generated", int_tag("minecraft:overworld", 1));
        let version = compound("data", version);
        let mut root = vec![10];
        root.extend(name(""));
        root.extend(version);
        root.push(0);
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(&root).unwrap();
        fs::write(
            path.join("data/tfg_worldgen_versions.dat"),
            gzip.finish().unwrap(),
        )
        .unwrap();
    }
    fn int_tag(key: &str, value: i32) -> Vec<u8> {
        let mut bytes = vec![3];
        bytes.extend(name(key));
        bytes.extend(value.to_be_bytes());
        bytes
    }
    fn scratch() -> PathBuf {
        std::env::temp_dir().join(format!(
            "terralens-source-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    #[test]
    fn opens_world_level_dat_and_folder_with_canonical_stable_unique_ids() {
        let root = scratch();
        let first = root.join("one/shared");
        let second = root.join("two/shared");
        write_world(&first, "Same Name", 123);
        write_world(&second, "Same Name", 456);
        let a = resolve_source(&first.join("level.dat")).unwrap();
        let again = resolve_source(&first).unwrap();
        let b = resolve_source(&second).unwrap();
        assert_eq!(a.worlds[0].id, again.worlds[0].id);
        assert_ne!(a.worlds[0].id, b.worlds[0].id);
        assert_eq!(a.worlds[0].seed, 123);
        assert_eq!(a.worlds[0].path, fs::canonicalize(&first).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn opens_prism_instance_and_saves_directory() {
        let root = scratch();
        let instance = root.join("Portable Prism Profile");
        let saves = instance.join(".minecraft/saves");
        write_world(&saves.join("A"), "A", 11);
        write_world(&saves.join("B"), "B", 22);
        let from_instance = resolve_source(&instance).unwrap();
        let from_saves = resolve_source(&saves).unwrap();
        let direct_saves = root.join("external-backup/saves");
        write_world(&direct_saves.join("A"), "A", 11);
        write_world(&direct_saves.join("B"), "B", 22);
        let from_external_saves = resolve_source(&direct_saves).unwrap();
        assert_eq!(from_instance.worlds.len(), 2);
        assert_eq!(from_saves.worlds.len(), 2);
        assert_eq!(from_instance.worlds[0].id, from_saves.worlds[0].id);
        assert_eq!(from_saves.worlds.len(), from_external_saves.worlds.len());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_or_malformed_source_fails_with_actionable_error() {
        let root = scratch();
        fs::create_dir_all(&root).unwrap();
        assert!(
            resolve_source(&root.join("missing"))
                .unwrap_err()
                .contains("Cannot open")
        );
        fs::write(root.join("level.dat"), b"not nbt").unwrap();
        assert!(
            resolve_source(&root.join("level.dat"))
                .unwrap_err()
                .contains("Could not read level.dat")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
