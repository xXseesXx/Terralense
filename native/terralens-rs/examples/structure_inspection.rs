//! Read-only inspection probe. Not compiled into the normal application.
use terralens::{
    dimension::Dimension,
    model::{Marker, MarkerKind, Settings},
    structure_preview,
    world::World,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 7 {
        return Err(
            "usage: structure_inspection WORLD SEED DIMENSION STRUCTURE CHUNK_X CHUNK_Z saved|seed"
                .into(),
        );
    }
    let seed = args[1].parse::<i64>()?;
    let dimension = Dimension::from_id(&args[2]).ok_or("unsupported dimension")?;
    let (cx, cz) = (args[4].parse::<i32>()?, args[5].parse::<i32>()?);
    let saved = match args[6].as_str() {
        "saved" => true,
        "seed" => false,
        _ => return Err("choose saved or seed".into()),
    };
    let marker = Marker {
        id: args[3].clone(),
        label: args[3].clone(),
        namespace: args[3].split(':').next().unwrap_or("minecraft").into(),
        x: cx as f64 * 16. + 8.,
        z: cz as f64 * 16. + 8.,
        y: None,
        kind: MarkerKind::Structure,
        confirmed: saved,
        min_y: None,
        max_y: None,
        detail: String::new(),
        color: [0; 3],
        bounds: None,
    };
    let mut world =
        World::for_dimension(seed, Settings::default(), dimension, dimension.default_y());
    let storage = dimension.storage(std::path::Path::new(&args[0]));
    let mut preview = structure_preview::inspect(
        &mut world,
        &marker,
        saved.then_some(storage.as_path()),
        &|| false,
    )?;
    let geometry_blocks = preview.blocks.len();
    preview.blocks.clear();
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({ "geometry_blocks": geometry_blocks, "preview": preview })
        )?
    );
    Ok(())
}
