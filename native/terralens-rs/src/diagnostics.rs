//! Developer-only read-only CLI, compiled with the diagnostics feature.
use std::{path::Path, time::Instant};
use terralens::{
    atlas::Layer,
    dimension::Dimension,
    model::{Point, Settings},
    prism, search,
    world::World,
};

fn value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter()
        .position(|s| s == key)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}
fn number(args: &[String], key: &str, default: f64) -> Result<f64, String> {
    match value(args, key) {
        Some(v) => {
            let x = v
                .parse::<f64>()
                .map_err(|_| format!("Invalid {key}: {v}"))?;
            if x.is_finite() {
                Ok(x)
            } else {
                Err(format!("{key} must be finite"))
            }
        }
        None => Ok(default),
    }
}
pub(super) fn run(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "TerraLens — native Rust TFG atlas\n\n  terralens                         Open the atlas\n  terralens --info                  Show detected Prism worlds\n  terralens --probe --x X --z Z      Sample one coordinate\n  terralens --render FILE.png        Render an offline PNG\n  terralens --find WORLD             Find atlas locations, nearest first\n\nCommon options: --seed I64 --x X --z Z\nDimension: --dimension overworld|beneath|moon|mars --slice-y 96\nRender: --layer biomes|temperature|rainfall|height|water|geology|forest|hotspots\n        --width 1200 --height 800 --bpp 32\nSearch: --scope structures|all|ores|biomes|rocks|forests|trees\n        --radius 10000 --query ruin --limit 50\nNative smoke: --smoke-test --screenshot FILE.png\nQoL smoke: --qol-smoke --screenshot FILE.png\nResponsiveness: --responsiveness REPORT.json\nSurface stress: --surface-stress REPORT.json\nBeneath stress: --beneath-stress REPORT.json\nViewer position: --view X Z BLOCKS_PER_PIXEL [--seed I64]\nTERRALENS_INSTANCE selects a different Prism instance."
        );
        return Ok(());
    }
    let instance = prism::default_instance();
    if args.iter().any(|a| a == "--info") {
        let info = prism::inspect_instance(&instance)?;
        println!("{}",serde_json::to_string_pretty(&serde_json::json!({"app":"TerraLens","runtime":"native Rust","minecraft":info.minecraft_version,"forge":info.forge_version,"core":info.core_version,"tfc":info.tfc_version,"instance":info.path,"worlds":info.worlds})).map_err(|e|e.to_string())?);
        return Ok(());
    }
    let seed = value(args, "--seed")
        .unwrap_or("7787531943268589495")
        .parse::<i64>()
        .map_err(|_| "Seed must be a signed 64-bit integer")?;
    let dimension = match value(args, "--dimension") {
        Some(id) => Dimension::from_id(id).ok_or_else(|| format!("Unsupported dimension {id}"))?,
        None => Dimension::Overworld,
    };
    let requested_y = number(args, "--slice-y", dimension.default_y() as f64)? as i32;
    let slice_y = if dimension == Dimension::Beneath {
        dimension.band(requested_y).1
    } else {
        dimension.default_y()
    };
    let point = Point {
        x: number(args, "--x", -2244.)?,
        z: number(args, "--z", -2316.)?,
    };
    if point.x.abs() > 30_000_000. || point.z.abs() > 30_000_000. {
        return Err("Coordinates exceed Minecraft's world border".into());
    }
    if let Some(id) = value(args, "--find") {
        let worlds = prism::discover_worlds(&instance)?;
        let world = worlds
            .iter()
            .find(|w| w.id == id || w.name == id)
            .ok_or_else(|| format!("Unknown Prism world {id}"))?;
        let start = Instant::now();
        let radius = number(args, "--radius", 100_000.)?.clamp(1., 100_000.);
        let limit = number(args, "--limit", 50.)?.clamp(1., 4096.) as usize;
        let scope = match value(args, "--scope").unwrap_or("structures") {
            "all" => search::Scope::All,
            "structures" => search::Scope::Structures,
            "ores" => search::Scope::Ores,
            "biomes" => search::Scope::Biomes,
            "rocks" => search::Scope::Geology,
            "forests" => search::Scope::Forests,
            "trees" => search::Scope::Trees,
            v => return Err(format!("Unknown search scope {v}")),
        };
        let query = search::Query {
            origin: point,
            radius,
            text: value(args, "--query").unwrap_or("").to_owned(),
            scope,
            structures_predicted: args.iter().any(|a| a == "--predictions"),
        };
        let scan = if matches!(scope, search::Scope::All | search::Scope::Structures) {
            Some(prism::scan_structures(&dimension.storage(&world.path))?)
        } else {
            None
        };
        if dimension == Dimension::Beneath
            && !world
                .dimensions
                .iter()
                .any(|d| d.id == dimension.id() && d.supported)
        {
            return Err("This save does not use the supported TFG Beneath generator".into());
        }
        let generation =
            World::for_dimension(world.seed, world.settings.clone(), dimension, slice_y);
        let outcome = search::predict(&generation, &query);
        let saved = scan
            .as_ref()
            .into_iter()
            .flat_map(|s| s.markers.iter())
            .filter(|m| scope.accepts(m) && search::matches_query(m, &query.text))
            .cloned();
        let results = search::rank(point, radius, saved.chain(outcome.markers), limit);
        println!("{}",serde_json::to_string_pretty(&serde_json::json!({"dimension":dimension.id(),"sliceY":slice_y,"origin":point,"radius":radius,"scope":scope.label(),"indexedStructures":scan.as_ref().map_or(0,|s|s.markers.len()),"scannedChunks":scan.as_ref().map_or(0,|s|s.chunks),"errors":scan.as_ref().map_or(&[][..],|s|s.errors.as_slice()),"limited":outcome.limited,"coverage":outcome.note,"elapsedMs":start.elapsed().as_millis(),"results":results.iter().map(|m|serde_json::json!({"distance":point.distance(m.point()),"marker":m})).collect::<Vec<_>>()})).map_err(|e|e.to_string())?);
        return Ok(());
    }
    if args.iter().any(|a| a == "--probe") {
        let start = Instant::now();
        let world = World::for_dimension(seed, Settings::default(), dimension, slice_y);
        let probe = world.probe(point.x.floor() as i32, point.z.floor() as i32);
        println!("{}",serde_json::to_string_pretty(&serde_json::json!({"dimension":dimension.id(),"sliceY":slice_y,"seed":seed.to_string(),"elapsedMs":start.elapsed().as_millis(),"probe":probe})).map_err(|e|e.to_string())?);
        return Ok(());
    }
    if let Some(file) = value(args, "--render") {
        let layer = match value(args, "--layer").unwrap_or("biomes") {
            "biomes" => Layer::Biomes,
            "temperature" => Layer::Temperature,
            "rainfall" => Layer::Rainfall,
            "height" => Layer::Height,
            "water" => Layer::Water,
            "geology" => Layer::Geology,
            "forest" => Layer::Forest,
            "hotspots" => Layer::Hotspots,
            unknown => return Err(format!("Unknown layer {unknown}")),
        };
        let width = number(args, "--width", 1200.)?.clamp(32., 4096.) as u32;
        let height = number(args, "--height", 800.)?.clamp(32., 4096.) as u32;
        let bpp = number(args, "--bpp", 32.)?.clamp(0.125, 4096.);
        let start = Instant::now();
        if dimension == Dimension::Beneath && layer != Layer::Biomes {
            return Err("Only the biome layer is supported for Beneath seed rendering".into());
        }
        let image = render(seed, point, bpp, width, height, layer, dimension, slice_y);
        if let Some(parent) = Path::new(file)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        image.save(file).map_err(|e| e.to_string())?;
        println!(
            "{}",
            serde_json::json!({"dimension":dimension.id(),"sliceY":slice_y,"file":file,"seed":seed.to_string(),"width":width,"height":height,"layer":layer.label(),"elapsedMs":start.elapsed().as_millis()})
        );
        return Ok(());
    }
    Err("Missing diagnostic command; use --help".into())
}
fn render(
    seed: i64,
    center: Point,
    bpp: f64,
    width: u32,
    height: u32,
    layer: Layer,
    dimension: Dimension,
    slice_y: i32,
) -> image::RgbaImage {
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, 6);
    let rows = (height as usize).div_ceil(workers);
    let parts = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for start_y in (0..height as usize).step_by(rows) {
            let end = (start_y + rows).min(height as usize);
            handles.push(scope.spawn(move || {
                let world = World::for_dimension(seed, Settings::default(), dimension, slice_y);
                let mut pixels = Vec::with_capacity((end - start_y) * width as usize * 4);
                for y in start_y..end {
                    for x in 0..width {
                        let bx =
                            (center.x + (x as f64 + 0.5 - width as f64 / 2.) * bpp).floor() as i32;
                        let bz =
                            (center.z + (y as f64 + 0.5 - height as f64 / 2.) * bpp).floor() as i32;
                        pixels.extend_from_slice(&terralens::atlas::raster_color(
                            &world, layer, bx, bz,
                        ));
                    }
                }
                pixels
            }));
        }
        handles
            .into_iter()
            .map(|h| h.join().expect("render worker"))
            .collect::<Vec<_>>()
    });
    image::RgbaImage::from_raw(width, height, parts.concat()).expect("exact image dimensions")
}
