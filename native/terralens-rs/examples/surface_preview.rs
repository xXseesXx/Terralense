//! Development-only seed surface rendering and timing, without a window or Minecraft.
use std::{collections::BTreeMap, time::Instant};
use terralens::{model::Settings, vegetation, world::World};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = args
        .first()
        .map(String::as_str)
        .unwrap_or("surface-preview.png");
    let x: i32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(-2244);
    let z: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(-2316);
    let chunks: i32 = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4)
        .clamp(1, 32);
    let seed: i64 = args
        .get(4)
        .and_then(|s| s.parse().ok())
        .unwrap_or(7787531943268589495);
    let world = World::new(seed, Settings::default());
    let cx = x.div_euclid(16) - chunks / 2;
    let cz = z.div_euclid(16) - chunks / 2;
    let mut image = image::RgbImage::new((chunks * 16) as u32, (chunks * 16) as u32);
    let start = Instant::now();
    let mut trees = BTreeMap::<String, usize>::new();
    let mut crops = BTreeMap::<String, usize>::new();
    for dz in 0..chunks {
        for dx in 0..chunks {
            let tile = vegetation::generate_surface(&world, cx + dx, cz + dz, &|| false);
            assert_eq!(tile.columns.len(), 256);
            for (i, column) in tile.columns.into_iter().enumerate() {
                let color = if let Some(tree) = column.tree {
                    *trees.entry(tree).or_default() += 1;
                    [56, 105, 62]
                } else if let Some(crop) = column.crop {
                    *crops.entry(crop).or_default() += 1;
                    [194, 166, 72]
                } else {
                    column.color
                };
                image.put_pixel(
                    (dx * 16) as u32 + (i % 16) as u32,
                    (dz * 16) as u32 + (i / 16) as u32,
                    image::Rgb(color),
                );
            }
        }
    }
    let elapsed = start.elapsed().as_secs_f64() * 1000.;
    image::imageops::resize(
        &image,
        image.width() * 8,
        image.height() * 8,
        image::imageops::FilterType::Nearest,
    )
    .save(output)
    .unwrap();
    println!(
        "{}",
        serde_json::json!({"seed":seed,"origin":[cx*16,cz*16],"chunks":chunks*chunks,"elapsedMs":elapsed,"canopyColumns":trees,"cropColumns":crops,"file":output,"scope":"seed prediction; generation order and terrain may differ; in-process CPU timing excludes native UI and GPU"})
    );
}
