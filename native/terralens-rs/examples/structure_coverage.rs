//! Read-only count of saved structure starts with and without recorded bounds.
//! Run with `cargo run --release --example structure_coverage`.
use std::collections::BTreeMap;

fn main() {
    let instance = terralens::prism::default_instance();
    let worlds = terralens::prism::inspect_instance(&instance).expect("inspect Prism instance");
    let world = worlds
        .worlds
        .iter()
        .find(|world| world.name == "Ganymede")
        .expect("Ganymede save");
    println!("Ganymede seed={}", world.seed);
    for dimension in world.dimensions.iter().filter(|dimension| {
        dimension.id == "minecraft:overworld" || dimension.id == "minecraft:the_nether"
    }) {
        let scan = terralens::prism::scan_structures(&dimension.path)
            .expect("read saved structure starts");
        let mut counts = BTreeMap::<String, (usize, usize)>::new();
        for marker in &scan.markers {
            let id = if marker.id.contains(':') {
                marker.id.clone()
            } else {
                format!("{}:{}", marker.namespace, marker.id)
            };
            let entry = counts.entry(id).or_default();
            entry.0 += 1;
            entry.1 += usize::from(marker.bounds.is_some());
        }
        let bounded = scan
            .markers
            .iter()
            .filter(|marker| marker.bounds.is_some())
            .count();
        println!(
            "dimension={} generated_chunks={} starts={} with_bounds={} without_bounds={} decode_errors={}",
            dimension.id,
            scan.chunks,
            scan.markers.len(),
            bounded,
            scan.markers.len() - bounded,
            scan.errors.len()
        );
        for (id, (total, with_bounds)) in counts {
            println!("  {id}: {with_bounds}/{total} bounded");
        }
    }
}
