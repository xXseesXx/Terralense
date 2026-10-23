//! Read-only check of installed TFG Beneath chunk decoding and biome prediction.
//! Run with `cargo run --release --example beneath_validation`.
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::Read;
use std::time::Instant;

fn active_chunks(region_dir: &std::path::Path) -> BTreeMap<(i32, i32), Vec<(i32, i32)>> {
    let mut regions = BTreeMap::<(i32, i32), Vec<(i32, i32)>>::new();
    let Ok(entries) = std::fs::read_dir(region_dir) else {
        return regions;
    };
    for entry in entries.flatten() {
        let filename = entry.file_name().to_string_lossy().to_string();
        let Some(coords) = filename
            .strip_prefix("r.")
            .and_then(|name| name.strip_suffix(".mca"))
        else {
            continue;
        };
        let Some((Ok(rx), Ok(rz))) = coords
            .split_once('.')
            .map(|(x, z)| (x.parse::<i32>(), z.parse::<i32>()))
        else {
            continue;
        };
        let Ok(mut file) = File::open(entry.path()) else {
            continue;
        };
        let mut header = [0; 4096];
        if file.read_exact(&mut header).is_err() {
            continue;
        }
        for slot in 0..1024 {
            if header[slot * 4..slot * 4 + 4]
                .iter()
                .all(|value| *value == 0)
            {
                continue;
            }
            regions
                .entry((rx, rz))
                .or_default()
                .push((rx * 32 + (slot % 32) as i32, rz * 32 + (slot / 32) as i32));
        }
    }
    regions
}

fn main() {
    let instance = terralens::prism::default_instance();
    let info = terralens::prism::inspect_instance(&instance).expect("inspect Prism instance");
    let Some(world) = info.worlds.iter().find(|world| world.name == "Ganymede") else {
        panic!("Ganymede save not found")
    };
    let Some(nether) = world
        .dimensions
        .iter()
        .find(|dimension| dimension.id == "minecraft:the_nether")
    else {
        panic!("Beneath not discovered")
    };
    println!(
        "world={} seed={} Beneath generated={} supported={} coverage_center=({}, {})",
        world.name,
        world.seed,
        nether.generated,
        nether.supported,
        nether.center.x,
        nether.center.z
    );

    let mut groups = active_chunks(&nether.path.join("region"))
        .into_iter()
        .collect::<Vec<_>>();
    groups.sort_by_key(|(_, chunks)| {
        let (cx, cz) = chunks[0];
        let center_x = (nether.center.x / 16.0).floor() as i32;
        let center_z = (nether.center.z / 16.0).floor() as i32;
        ((cx - center_x).pow(2) + (cz - center_z).pow(2), cx, cz)
    });
    // Choose one real chunk from each of up to 64 regions, sampling the map broadly.
    let count = groups.len().min(64);
    let selected = if count == 0 {
        vec![]
    } else {
        (0..count)
            .map(|i| i * groups.len() / count)
            .collect::<Vec<_>>()
    };
    let chunks = selected
        .into_iter()
        .map(|index| {
            let (_, candidates) = &groups[index];
            let target_x = (nether.center.x / 16.0).floor() as i32;
            let target_z = (nether.center.z / 16.0).floor() as i32;
            *candidates
                .iter()
                .min_by_key(|(x, z)| (*x - target_x).pow(2) + (*z - target_z).pow(2))
                .unwrap()
        })
        .collect::<Vec<_>>();

    let started = Instant::now();
    let model = terralens::beneath::Beneath::new(world.seed);
    let mut comparisons = 0usize;
    let mut matches = 0usize;
    let mut mismatches = 0usize;
    let mut full_chunks = 0usize;
    let mut examples = Vec::new();
    let mut slice_candidate: Option<(usize, i32, i32, BTreeMap<String, usize>, i32, i32)> = None;
    for (cx, cz) in &chunks {
        let mut chunk_complete = true;
        let mut biome_sets = HashMap::new();
        for y in [20, 96, 195] {
            let Ok(Some(saved)) = terralens::prism::read_biome_chunk(&nether.path, *cx, *cz, y)
            else {
                chunk_complete = false;
                break;
            };
            biome_sets.insert(y, saved);
        }
        if !chunk_complete {
            continue;
        }
        full_chunks += 1;
        for y in [20, 96, 195] {
            for z in 0..4 {
                for x in 0..4 {
                    let at = z * 4 + x;
                    let saved = &biome_sets[&y][at];
                    let predicted = terralens::beneath::biome_name(model.biome_id(
                        *cx * 16 + x as i32 * 4,
                        y,
                        *cz * 16 + z as i32 * 4,
                    ));
                    comparisons += 1;
                    if saved == predicted {
                        matches += 1;
                    } else {
                        mismatches += 1;
                        if examples.len() < 3 {
                            examples.push(format!("chunk=({cx},{cz}) quart=({x},{z}) y={y} saved={saved} predicted={predicted}"));
                        }
                    }
                }
            }
        }
        if slice_candidate.is_none() {
            if let Ok(Some(slice)) =
                terralens::prism::read_surface_slice(&nether.path, *cx, *cz, 96)
            {
                let mut materials = BTreeMap::new();
                let mut min_y = i32::MAX;
                let mut max_y = i32::MIN;
                for column in slice.columns {
                    if column.y == i32::MIN {
                        continue;
                    }
                    *materials.entry(column.block).or_insert(0usize) += 1;
                    min_y = min_y.min(column.y);
                    max_y = max_y.max(column.y);
                }
                slice_candidate =
                    Some((materials.values().sum(), *cx, *cz, materials, min_y, max_y));
            }
        }
    }
    if let Some((columns, cx, cz, materials, min_y, max_y)) = slice_candidate {
        println!(
            "saved_floor_sample chunk=({cx},{cz}) y_limit=96 visible_columns={columns} floor_y={min_y}..{max_y} materials={materials:?}"
        );
    }
    println!(
        "biome_validation full_chunks={full_chunks}/{} comparisons={comparisons} matches={matches} mismatches={mismatches} elapsed_ms={:.1}",
        chunks.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    for example in examples {
        println!("mismatch: {example}");
    }
}
