use std::collections::BTreeMap;
use std::path::Path;

fn sample(world: &Path, label: &str, block_x: f64, block_z: f64) {
    let center_x = (block_x / 16.0).floor() as i32;
    let center_z = (block_z / 16.0).floor() as i32;
    let mut blocks = BTreeMap::<String, usize>::new();
    let mut generated = 0usize;
    let mut missing = 0usize;
    let mut tree = 0usize;
    let mut crop = 0usize;
    for cz in center_z - 1..=center_z + 1 {
        for cx in center_x - 1..=center_x + 1 {
            match terralens::prism::read_surface_tile(world, cx, cz) {
                Ok(Some(chunk)) => {
                    generated += 1;
                    for col in chunk.columns {
                        if !col.block.is_empty() {
                            *blocks.entry(col.block).or_default() += 1;
                        }
                        tree += usize::from(col.tree.is_some());
                        crop += usize::from(col.crop.is_some());
                    }
                }
                Ok(None) => missing += 1,
                Err(error) => eprintln!("{label} chunk ({cx},{cz}): {error}"),
            }
        }
    }
    let mut top = blocks.into_iter().collect::<Vec<_>>();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.truncate(8);
    println!(
        "{label}: center=({center_x},{center_z}) generated_chunks={generated}/9 missing={missing} top_tree_columns={tree} top_crop_columns={crop} most_common_ids={top:?}"
    );
}

fn main() {
    let worlds = terralens::prism::discover_worlds(&terralens::prism::default_instance()).unwrap();
    for world in worlds.iter().filter(|w| w.name == "Ganymede") {
        sample(
            &world.path,
            "Ganymede saved-player center",
            world.center.x,
            world.center.z,
        );
        sample(
            &world.path,
            "Ganymede map base (-2244,-2316)",
            -2244.0,
            -2316.0,
        );
    }
}
