//! Real child process lifecycle and typed probe transport, without initializing a display.
#![cfg(feature = "gui")]
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use terralens::{model::Settings, world::World};

struct Engine(Child);
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn launch() -> Engine {
    Engine(
        Command::new(env!("CARGO_BIN_EXE_terralens"))
            .arg("--engine")
            .env("RAYON_NUM_THREADS", "2")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    )
}
fn command(child: &mut Child, value: Value) {
    let bytes = serde_json::to_vec(&value).unwrap();
    let input = child.stdin.as_mut().unwrap();
    input
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .unwrap();
    input.write_all(&0u32.to_le_bytes()).unwrap();
    input.write_all(&bytes).unwrap();
    input.flush().unwrap();
}
fn reply(child: &mut Child) -> Value {
    let (value, bulk) = reply_frame(child);
    assert!(bulk.is_empty());
    value
}
fn reply_frame(child: &mut Child) -> (Value, Vec<u8>) {
    // Timeout keeps failures reviewable even if the child stops delivering results.
    let mut output = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut header = [0; 8];
        output.read_exact(&mut header).unwrap();
        let size = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        let bulk_size = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        assert!(size < 16 * 1024 * 1024);
        let mut bytes = vec![0; size];
        output.read_exact(&mut bytes).unwrap();
        assert!(bulk_size < 8 * 1024 * 1024);
        let mut bulk = vec![0; bulk_size];
        output.read_exact(&mut bulk).unwrap();
        let _ = tx.send((serde_json::from_slice(&bytes).unwrap(), bulk));
    });
    rx.recv_timeout(Duration::from_secs(15))
        .expect("engine reply timed out")
}

#[test]
fn isolated_structure_inspection_reads_vessel_inventory_and_geometry_without_writing_save() {
    use terralens::model::{Marker, MarkerKind};
    fn int_tag(out: &mut Vec<u8>, name: &str, value: i32) {
        out.push(3);
        nbt_name(out, name);
        out.extend_from_slice(&value.to_be_bytes());
    }
    fn bounds_tag(out: &mut Vec<u8>) {
        out.push(11);
        nbt_name(out, "BB");
        out.extend_from_slice(&6_i32.to_be_bytes());
        for n in [0_i32, 64, 0, 4, 68, 4] {
            out.extend_from_slice(&n.to_be_bytes());
        }
    }
    let save = std::env::temp_dir().join(format!(
        "terralens-inspection-engine-{}",
        std::process::id()
    ));
    let mut nbt = vec![10, 0, 0];
    nbt_string_tag(&mut nbt, "Status", "minecraft:full");
    for name in ["structures", "starts", "test:saved_house"] {
        nbt.push(10);
        nbt_name(&mut nbt, name);
    }
    nbt_string_tag(&mut nbt, "id", "test:saved_house");
    bounds_tag(&mut nbt);
    nbt.push(9);
    nbt_name(&mut nbt, "Children");
    nbt.push(10);
    nbt.extend_from_slice(&1_i32.to_be_bytes());
    nbt_string_tag(&mut nbt, "id", "minecraft:jigsaw");
    bounds_tag(&mut nbt);
    nbt.extend_from_slice(&[0, 0, 0, 0]);
    nbt.push(9);
    nbt_name(&mut nbt, "sections");
    nbt.push(10);
    nbt.extend_from_slice(&1_i32.to_be_bytes());
    nbt.push(1);
    nbt_name(&mut nbt, "Y");
    nbt.push(4);
    nbt.push(10);
    nbt_name(&mut nbt, "block_states");
    nbt.push(9);
    nbt_name(&mut nbt, "palette");
    nbt.push(10);
    nbt.extend_from_slice(&1_i32.to_be_bytes());
    nbt_string_tag(&mut nbt, "Name", "tfc:mud_bricks/silt");
    nbt.extend_from_slice(&[0, 0, 0]);
    nbt.push(9);
    nbt_name(&mut nbt, "block_entities");
    nbt.push(10);
    nbt.extend_from_slice(&1_i32.to_be_bytes());
    nbt_string_tag(&mut nbt, "id", "tfc:large_vessel");
    for (name, n) in [("x", 2), ("y", 65), ("z", 3)] {
        int_tag(&mut nbt, name, n);
    }
    nbt.push(10);
    nbt_name(&mut nbt, "inventory");
    nbt.push(9);
    nbt_name(&mut nbt, "Items");
    nbt.push(10);
    nbt.extend_from_slice(&1_i32.to_be_bytes());
    nbt_string_tag(&mut nbt, "id", "minecraft:clay_ball");
    nbt.push(1);
    nbt_name(&mut nbt, "Count");
    nbt.push(18);
    nbt.push(1);
    nbt_name(&mut nbt, "Slot");
    nbt.push(0);
    nbt.extend_from_slice(&[0, 0, 0, 0]);
    write_region(&save, &[(0, nbt)]);
    let file = save.join("region/r.0.0.mca");
    let before = fs::read(&file).unwrap();
    let marker = Marker {
        id: "test:saved_house".into(),
        label: "test:saved_house".into(),
        namespace: "test".into(),
        x: 8.,
        z: 8.,
        y: None,
        kind: MarkerKind::Structure,
        confirmed: true,
        min_y: None,
        max_y: None,
        detail: String::new(),
        color: [0; 3],
        bounds: None,
    };
    let mut engine = launch();
    command(
        &mut engine.0,
        json!({"Reset":{"epoch":7,"search_revision":0}}),
    );
    command(
        &mut engine.0,
        json!({"High":{"InspectStructure":[
            {"seed":0,"settings":Settings::default(),"epoch":7},save,marker,41
        ]}}),
    );
    let (answer, bulk) = reply_frame(&mut engine.0);
    assert_eq!(answer["StructurePreview"][0], 7);
    assert_eq!(answer["StructurePreview"][1], 41);
    let preview = &answer["StructurePreview"][2]["Ok"];
    assert_eq!(preview["source"], "Saved");
    assert_eq!(preview["containers"].as_array().unwrap().len(), 1);
    let container = &preview["containers"][0];
    assert_eq!(container["pos"], json!({"x":2,"y":65,"z":3}));
    assert_eq!(container["items"][0]["id"], "minecraft:clay_ball");
    assert_eq!(container["items"][0]["count"], 18);
    assert_eq!(container["contents_known"], true);
    assert_eq!(&bulk[..4], b"TSB1");
    assert!(
        bulk.len() > 100,
        "real geometry must cross the process boundary"
    );
    assert_eq!(
        fs::read(&file).unwrap(),
        before,
        "save inspection must be read-only"
    );
    fs::remove_dir_all(save).unwrap();
}
fn wait_exit(child: &mut Child) {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "engine did not exit after stdin closed"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn nbt_name(out: &mut Vec<u8>, name: &str) {
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
}
fn nbt_string_tag(out: &mut Vec<u8>, name: &str, value: &str) {
    out.push(8);
    nbt_name(out, name);
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}
fn full_stone_chunk_nbt() -> Vec<u8> {
    full_uniform_block_chunk_nbt(0, "minecraft:stone")
}
fn full_uniform_block_chunk_nbt(section_y: i8, block: &str) -> Vec<u8> {
    let mut out = vec![10, 0, 0]; // unnamed root compound
    nbt_string_tag(&mut out, "Status", "minecraft:full");
    out.push(9); // sections: list of compounds
    nbt_name(&mut out, "sections");
    out.push(10);
    out.extend_from_slice(&1i32.to_be_bytes());
    out.push(1); // byte Y
    nbt_name(&mut out, "Y");
    out.push(section_y as u8); // signed section value
    out.push(10); // block_states compound
    nbt_name(&mut out, "block_states");
    out.push(9); // palette list
    nbt_name(&mut out, "palette");
    out.push(10);
    out.extend_from_slice(&1i32.to_be_bytes());
    nbt_string_tag(&mut out, "Name", block);
    out.push(0); // palette element end
    out.push(0); // block_states end
    out.push(0); // section end
    out.push(0); // root end
    out
}
fn planetary_surface_fixture() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "terralens-planetary-surface-engine-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    write_region(
        &root.join("dimensions/ad_astra/moon"),
        &[(0, full_uniform_block_chunk_nbt(23, "ad_astra:moon_stone"))],
    );
    write_region(
        &root.join("dimensions/ad_astra/mars"),
        &[
            (
                0,
                full_uniform_block_chunk_nbt(-2, "tfg:rock/hardened_mars_stone"),
            ),
            (1, full_uniform_block_chunk_nbt(21, "ad_astra:mars_stone")),
        ],
    );
    root
}
fn write_region(root: &std::path::Path, chunks: &[(usize, Vec<u8>)]) {
    let region_dir = root.join("region");
    fs::create_dir_all(&region_dir).unwrap();
    let mut region = vec![0u8; (2 + chunks.len()) * 4096];
    let mut sector = 2usize;
    for (slot, nbt) in chunks {
        let header = slot * 4;
        region[header..header + 4].copy_from_slice(&[0, (sector >> 8) as u8, sector as u8, 1]);
        let start = sector * 4096;
        let length = (nbt.len() + 1) as u32;
        region[start..start + 4].copy_from_slice(&length.to_be_bytes());
        region[start + 4] = 3; // uncompressed NBT
        region[start + 5..start + 5 + nbt.len()].copy_from_slice(nbt);
        sector += 1;
    }
    fs::write(region_dir.join("r.0.0.mca"), region).unwrap();
}
fn saved_surface_fixture() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "terralens-surface-engine-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let region_dir = root.join("region");
    fs::create_dir_all(&region_dir).unwrap();
    let nbt = full_stone_chunk_nbt();
    let mut region = vec![0u8; 3 * 4096];
    region[0..4].copy_from_slice(&[0, 0, 2, 1]); // slot 0 begins at sector 2
    let length = (nbt.len() + 1) as u32;
    region[8192..8196].copy_from_slice(&length.to_be_bytes());
    region[8196] = 3; // uncompressed NBT
    region[8197..8197 + nbt.len()].copy_from_slice(&nbt);
    fs::write(region_dir.join("r.0.0.mca"), region).unwrap();
    root
}

#[test]
fn isolated_engine_surface_keeps_saved_and_predicted_provenance_and_drops_stale_epoch() {
    let mut engine = launch();
    let save = saved_surface_fixture();
    command(
        &mut engine.0,
        json!({"Reset":{"epoch":8,"search_revision":0}}),
    );
    command(
        &mut engine.0,
        json!({"Low":{"Surface":[
            {"seed":7787531943268589495_i64,"settings":Settings::default(),"epoch":7},
            save,[[99,99]],70
        ]}}),
    );
    command(
        &mut engine.0,
        json!({"Low":{"Surface":[
            {"seed":7787531943268589495_i64,"settings":Settings::default(),"epoch":8},
            save,[[0,0],[1,0]],80
        ]}}),
    );
    let answer = reply(&mut engine.0);
    assert_eq!(answer["Surface"][0], 8);
    assert_eq!(answer["Surface"][1], 80, "stale epoch work was delivered");
    let rows = answer["Surface"][2].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let saved = &rows[0][2]["Ok"];
    assert_eq!(saved["source"], "Saved");
    assert_eq!(saved["chunk"]["columns"].as_array().unwrap().len(), 256);
    assert_eq!(saved["chunk"]["columns"][0]["block"], "minecraft:stone");
    assert_eq!(saved["chunk"]["columns"][0]["y"], 15);
    let predicted = &rows[1][2]["Ok"];
    assert_eq!(predicted["source"], "Predicted");
    assert_eq!(predicted["chunk"]["columns"].as_array().unwrap().len(), 256);
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
    fs::remove_dir_all(save).unwrap();
}
#[test]
fn isolated_beneath_surface_does_not_substitute_overworld_for_absent_chunks() {
    let mut engine = launch();
    let save = saved_surface_fixture();
    command(
        &mut engine.0,
        json!({"Reset":{"epoch":8,"search_revision":0}}),
    );
    command(
        &mut engine.0,
        json!({"Low":{"Surface":[
            {"seed":7787531943268589495_i64,"settings":Settings::default(),"epoch":8,"dimension":"minecraft:the_nether","slice_y":15},
            save,[[0,0],[1,0]],81
        ]}}),
    );
    let answer = reply(&mut engine.0);
    let rows = answer["Surface"][2].as_array().unwrap();
    assert_eq!(rows[0][2]["Ok"]["source"], "Saved");
    assert_eq!(rows[0][2]["Ok"]["chunk"]["columns"][0]["y"], 15);
    assert!(
        rows[1][2]["Ok"].is_null(),
        "Missing Nether chunks must stay unknown, never become overworld vegetation"
    );
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
    fs::remove_dir_all(save).unwrap();
}

#[test]
fn isolated_engine_surface_structure_cutaway_uses_saved_band_and_horizontal_clip() {
    let mut engine = launch();
    let save = saved_surface_fixture();
    command(
        &mut engine.0,
        json!({"Reset":{"epoch":9,"search_revision":0}}),
    );
    command(
        &mut engine.0,
        json!({"Low":{"Surface":[
            {"seed":7787531943268589495_i64,"settings":Settings::default(),"epoch":9,"dimension":"minecraft:the_nether","slice_y":8},
            save,
            [[0,0],[1,0]],
            91,
            {"min_x":2,"min_y":5,"min_z":3,"max_x":5,"max_y":20,"max_z":6}
        ]}}),
    );
    let answer = reply(&mut engine.0);
    assert_eq!(answer["Surface"][0], 9);
    assert_eq!(answer["Surface"][1], 91);
    let rows = answer["Surface"][2].as_array().unwrap();
    let cutaway = &rows[0][2]["Ok"];
    assert_eq!(cutaway["source"], "Saved");
    assert_eq!(cutaway["chunk"]["columns"][50]["y"], 15);
    assert_eq!(cutaway["chunk"]["columns"][50]["block"], "minecraft:stone");
    assert_eq!(cutaway["chunk"]["columns"][0]["y"], i32::MIN);
    assert_eq!(cutaway["chunk"]["columns"][0]["block"], "");
    assert!(
        rows[1][2]["Ok"].is_null(),
        "outside saved chunks stay unknown"
    );
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
    fs::remove_dir_all(save).unwrap();
}

#[test]
fn isolated_engine_structure_cutaway_respects_selected_y_bands_and_absence() {
    let save = saved_surface_fixture();
    let run = |dimension: &str, slice_y: i32, min_y: i32, max_y: i32, serial: u64| {
        let mut engine = launch();
        command(
            &mut engine.0,
            json!({"Reset":{"epoch":10,"search_revision":0}}),
        );
        command(
            &mut engine.0,
            json!({"Low":{"Surface":[
                {"seed":7787531943268589495_i64,"settings":Settings::default(),"epoch":10,"dimension":dimension,"slice_y":slice_y},
                save,[[0,0]],serial,
                {"min_x":2,"min_y":min_y,"min_z":3,"max_x":5,"max_y":max_y,"max_z":6}
            ]}}),
        );
        let answer = reply(&mut engine.0);
        drop(engine.0.stdin.take());
        wait_exit(&mut engine.0);
        answer
    };
    let overworld = run("minecraft:overworld", 64, 0, 7, 101);
    assert_eq!(
        overworld["Surface"][2][0][2]["Ok"]["chunk"]["columns"][50]["y"], 7,
        "overworld cutaway must hide the roof above the selected Y range"
    );

    let beneath_band = run("minecraft:the_nether", 8, 0, 20, 102);
    assert_eq!(
        beneath_band["Surface"][2][0][2]["Ok"]["chunk"]["columns"][50]["y"],
        15
    );

    let absent_band = run("minecraft:the_nether", 24, 0, 20, 103);
    assert!(
        absent_band["Surface"][2][0][2]["Ok"].is_null(),
        "the upper Beneath band has no saved blocks and must remain absent"
    );
    fs::remove_dir_all(save).unwrap();
}

#[test]
fn isolated_engine_reads_planetary_saved_surfaces_at_high_and_negative_y() {
    let save = planetary_surface_fixture();
    assert_eq!(
        terralens::prism::read_surface_tile(&save.join("dimensions/ad_astra/moon"), 0, 0)
            .unwrap()
            .unwrap()
            .columns[0]
            .y,
        383
    );
    let mut engine = launch();
    command(
        &mut engine.0,
        json!({"Reset":{"epoch":11,"search_revision":0}}),
    );
    command(
        &mut engine.0,
        json!({"Low":{"Surface":[
            {"seed":0,"settings":Settings::default(),"epoch":11,"dimension":"ad_astra:moon","slice_y":64},
            save.join("dimensions/ad_astra/moon"),[[0,0]],111
        ]}}),
    );
    let moon = reply(&mut engine.0);
    let column = &moon["Surface"][2][0][2]["Ok"]["chunk"]["columns"][0];
    assert_eq!(column["y"], 383);
    assert_eq!(column["block"], "ad_astra:moon_stone");
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);

    let mut engine = launch();
    command(
        &mut engine.0,
        json!({"Reset":{"epoch":12,"search_revision":0}}),
    );
    command(
        &mut engine.0,
        json!({"Low":{"Surface":[
            {"seed":0,"settings":Settings::default(),"epoch":12,"dimension":"ad_astra:mars","slice_y":88},
            save.join("dimensions/ad_astra/mars"),[[0,0],[1,0]],112
        ]}}),
    );
    let mars = reply(&mut engine.0);
    let low = &mars["Surface"][2][0][2]["Ok"]["chunk"]["columns"][0];
    let high = &mars["Surface"][2][1][2]["Ok"]["chunk"]["columns"][0];
    assert_eq!(low["y"], -17);
    assert_eq!(low["block"], "tfg:rock/hardened_mars_stone");
    assert_eq!(high["y"], 351);
    assert_eq!(high["block"], "ad_astra:mars_stone");

    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
    fs::remove_dir_all(save).unwrap();
}

#[test]
fn isolated_planetary_probe_and_export_use_seed_zero_reference_locations() {
    let fixture: Value =
        serde_json::from_str(include_str!("../src/planetary/seed0_biomes.json")).unwrap();
    assert_eq!(fixture["seed"], 0);
    for (epoch, dimension_data) in fixture["dimensions"].as_array().unwrap().iter().enumerate() {
        let dimension = dimension_data["dimension"].as_str().unwrap();
        let sample = &dimension_data["samples"][0];
        let (x, y, z) = (
            sample["x"].as_i64().unwrap() as i32,
            sample["y"].as_i64().unwrap() as i32,
            sample["z"].as_i64().unwrap() as i32,
        );
        let dimension_kind = terralens::dimension::Dimension::from_id(dimension).unwrap();
        let world = World::for_dimension(0, Settings::default(), dimension_kind, y);
        assert_eq!(world.biome_at(x, y, z), sample["biome"].as_str().unwrap());

        let mut engine = launch();
        let epoch = 30 + epoch as u64;
        command(
            &mut engine.0,
            json!({"Reset":{"epoch":epoch,"search_revision":0}}),
        );
        command(
            &mut engine.0,
            json!({"High":{"Probe":[
                {"seed":0,"settings":Settings::default(),"epoch":epoch,"dimension":dimension,"slice_y":y},
                {"x":x,"z":z},false,epoch
            ]}}),
        );
        let answer = reply(&mut engine.0);
        assert_eq!(answer["Probe"][0], epoch);
        assert_eq!(answer["Probe"][2]["biome"], world.biome(x, z));
        assert!(answer["Probe"][2]["height"].is_null());
        drop(engine.0.stdin.take());
        wait_exit(&mut engine.0);

        let mut engine = launch();
        command(
            &mut engine.0,
            json!({"Reset":{"epoch":epoch,"search_revision":0}}),
        );
        command(&mut engine.0, json!({"TaskRevision":{"serial":epoch}}));
        let path = std::env::temp_dir().join(format!(
            "terralens-planetary-export-{}-{epoch}.png",
            std::process::id()
        ));
        let request = json!({"Map":{
            "bounds":{"min_x":x-2,"min_z":z-2,"max_x":x+2,"max_z":z+2},
            "layer":"Biomes","width":1,"height":1,"path":path
        }});
        command(
            &mut engine.0,
            json!({"Low":{"Export":[
                {"seed":0,"settings":Settings::default(),"epoch":epoch},request,epoch
            ]}}),
        );
        let answer = reply(&mut engine.0);
        assert_eq!(answer["Export"][2]["Ok"]["note"], "1 × 1 · Biomes");
        let image = image::open(&path).unwrap();
        assert_eq!((image.width(), image.height()), (1, 1));
        fs::remove_file(path).unwrap();
        drop(engine.0.stdin.take());
        wait_exit(&mut engine.0);
    }
}

#[test]
fn isolated_beneath_probe_obeys_dimension_and_vertical_slice() {
    let fixture: Value =
        serde_json::from_str(include_str!("../src/beneath/real_biome_fixture.json")).unwrap();
    let seed = fixture["seed"].as_str().unwrap().parse::<i64>().unwrap();
    let samples = fixture["samples"].as_array().unwrap();
    for sample in samples
        .iter()
        .filter(|s| s["qy"].as_i64().unwrap().rem_euclid(4) == 2)
        .take(2)
    {
        let mut engine = launch();
        let target = (
            sample["qx"].as_i64().unwrap() as i32,
            sample["qy"].as_i64().unwrap() as i32,
            sample["qz"].as_i64().unwrap() as i32,
        );
        let y = target.1 * 4;
        // The save fixture records raw quart cells. Choose a nearby block whose
        // vanilla fuzzy lookup selects that recorded cell at the band center.
        let hashed_seed = terralens::biome::block::obfuscate_seed(seed);
        let (x, z) = (-4..=8)
            .flat_map(|dx| (-4..=8).map(move |dz| (target.0 * 4 + dx, target.2 * 4 + dz)))
            .find(|&(x, z)| terralens::biome::block::nearest_quart(hashed_seed, x, y, z) == target)
            .expect("recorded quart must own a block at its band center");
        command(
            &mut engine.0,
            json!({"High":{"Probe":[
                {"seed":seed,"settings":Settings::default(),"epoch":1,"dimension":"minecraft:the_nether","slice_y":y},
                {"x":x,"z":z},true,22
            ]}}),
        );
        let answer = reply(&mut engine.0);
        assert_eq!(answer["Probe"][2]["biome"], sample["biome"]);
        assert!(answer["Probe"][2]["height"].is_null());
        drop(engine.0.stdin.take());
        wait_exit(&mut engine.0);
    }
}

#[test]
fn isolated_volcano_search_finds_seeded_vents_as_landmarks_sorted_by_distance() {
    let mut engine = launch();
    let world = World::new(0, Settings::default());
    command(&mut engine.0, json!({"SearchRevision":{"serial":24}}));
    command(
        &mut engine.0,
        json!({"Low":{"Search":[
            {"seed":0,"settings":Settings::default(),"epoch":1},
            {"origin":{"x":-2660,"z":3951},"radius":750.0,"query":"volcanos",
             "predictions":false,"scope":"Volcanoes","serial":24}
        ]}}),
    );
    let answer = reply(&mut engine.0);
    assert_eq!(answer["Search"][0], 1);
    assert_eq!(answer["Search"][1], 24);
    assert_eq!(answer["Search"][3], false);
    let markers = answer["Search"][2].as_array().unwrap();
    assert!(!markers.is_empty());
    assert_eq!(
        markers[0]["id"],
        "terralens:volcano/active_shield@-2660,3951"
    );
    let mut previous = 0.0;
    for marker in markers {
        assert_eq!(marker["kind"], "Volcano");
        assert!(marker.get("bounds").is_none());
        assert_eq!(
            marker["y"].as_f64(),
            world.height(
                marker["x"].as_f64().unwrap() as i32,
                marker["z"].as_f64().unwrap() as i32
            ),
            "volcano pins carry the modeled elevation at their own center"
        );
        let distance =
            (marker["x"].as_f64().unwrap() + 2660.).hypot(marker["z"].as_f64().unwrap() - 3951.);
        assert!(distance >= previous && distance <= 750.);
        previous = distance;
    }
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
}

#[test]
fn isolated_map_probe_resolves_block_biomes_without_changing_generation_source() {
    let world = World::new(0, Settings::default());
    assert_eq!(world.biome(380, -246), "tfg:earth/hills");
    let height = world
        .height(380, -246)
        .expect("surface height is available");
    assert!(!world.is_surface_water(380, -246));
    let mut engine = launch();
    command(
        &mut engine.0,
        json!({"High":{"Probe":[
            {"seed":0,"settings":Settings::default(),"epoch":1},
            {"x":380,"z":-246},true,23
        ]}}),
    );
    let answer = reply(&mut engine.0);
    assert_eq!(answer["Probe"][2]["biome"], "tfg:earth/shore");
    assert_eq!(answer["Probe"][2]["height"], height);
    assert_eq!(answer["Probe"][2]["surface_water"], false);
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
}

#[test]
fn isolated_engine_probe_preserves_seed_and_exact_climate() {
    let mut engine = launch();
    assert_ne!(engine.0.id(), std::process::id());
    let seed = 7787531943268589495_i64;
    command(
        &mut engine.0,
        json!({"Reset":{"epoch":7,"search_revision":0}}),
    );
    command(
        &mut engine.0,
        json!({"High":{"Probe":[
            {"seed":seed,"settings":Settings::default(),"epoch":7},
            {"x":-2244.,"z":-2316.},false,13
        ]}}),
    );
    let answer = reply(&mut engine.0);
    let probe = &answer["Probe"];
    assert_eq!(probe[0], 7);
    assert_eq!(probe[1], 13);
    let expected = World::new(seed, Settings::default()).probe(-2244, -2316);
    assert_eq!(probe[2]["biome"], expected.biome);
    assert_eq!(
        probe[2]["climate"],
        serde_json::to_value(expected.climate).unwrap()
    );
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
}

#[test]
fn isolated_engine_resolves_import_sources_off_the_command_path_and_returns_errors_by_serial() {
    let mut engine = launch();
    let path = std::env::temp_dir().join(format!(
        "terralens-source-does-not-exist-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    command(
        &mut engine.0,
        json!({"OpenSource":{"path":path,"serial":77}}),
    );
    let answer = reply(&mut engine.0);
    assert_eq!(answer["OpenedSource"][0], 77);
    assert!(
        answer["OpenedSource"][1]["Err"]
            .as_str()
            .unwrap()
            .contains("Cannot open")
    );
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
}

#[test]
fn isolated_engine_composes_png_without_a_display_and_returns_atomic_output() {
    use terralens::{
        atlas::Layer,
        exports::{Composition, Request},
        model::Bounds,
    };
    let mut engine = launch();
    let path =
        std::env::temp_dir().join(format!("terralens-composed-ipc-{}.png", std::process::id()));
    command(
        &mut engine.0,
        json!({"Reset":{"epoch":17,"search_revision":0}}),
    );
    command(&mut engine.0, json!({"TaskRevision":{"serial":31}}));
    let request = Request::ComposedMap {
        bounds: Bounds {
            min_x: -2400.,
            min_z: -2400.,
            max_x: -2200.,
            max_z: -2200.,
        },
        layer: Layer::Biomes,
        width: 160,
        height: 100,
        path: path.clone(),
        markers: Vec::new(),
        composition: Composition {
            coordinates: true,
            scale_bar: true,
            ..Default::default()
        },
    };
    command(
        &mut engine.0,
        json!({"Low":{"Export":[
            {"seed":7787531943268589495_i64,"settings":Settings::default(),"epoch":17}, request,31
        ]}}),
    );
    let answer = reply(&mut engine.0);
    assert_eq!(answer["Export"][0], 17);
    assert_eq!(answer["Export"][1], 31);
    assert_eq!(answer["Export"][2]["Ok"]["path"], path.to_str().unwrap());
    let image = image::open(&path).unwrap();
    assert_eq!((image.width(), image.height()), (160, 100));
    fs::remove_file(path).unwrap();
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
}
#[test]
fn engine_exits_when_parent_pipe_closes_with_work_outstanding() {
    let mut engine = launch();
    command(&mut engine.0, json!({"SearchRevision":{"serial":1}}));
    command(
        &mut engine.0,
        json!({"Low":{"Search":[
            {"seed":7787531943268589495_i64,"settings":Settings::default(),"epoch":1},
            {"origin":{"x":100000.,"z":-100000.},"radius":100000.,"query":"copper", "predictions":false,"scope":"Ores","serial":1}
        ]}}),
    );
    std::thread::sleep(Duration::from_millis(100));
    assert!(engine.0.try_wait().unwrap().is_none());
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
}

#[test]
fn isolated_engine_delivers_regional_analysis_and_atomic_session_export() {
    let mut engine = launch();
    command(&mut engine.0, json!({"TaskRevision":{"serial":1}}));
    command(
        &mut engine.0,
        json!({"Low":{"Analyze":[
            {"seed":7787531943268589495_i64,"settings":Settings::default(),"epoch":1},
            {"bounds":{"min_x":-2300.,"min_z":-2400.,"max_x":-2100.,"max_z":-2200.},"sample_budget":64},1
        ]}}),
    );
    let answer = reply(&mut engine.0);
    assert!(
        answer["Analysis"][2]["Ok"]["sample_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
    let mut engine = launch();
    let path = std::env::temp_dir().join(format!(
        "terralens-engine-export-{}.json",
        std::process::id()
    ));
    command(&mut engine.0, json!({"TaskRevision":{"serial":1}}));
    command(
        &mut engine.0,
        json!({"Low":{"Export":[
            {"seed":1,"settings":Settings::default(),"epoch":1},
            {"Data":{"value":{"center":{"x":-1,"z":512}},"path":path}},1
        ]}}),
    );
    let answer = reply(&mut engine.0);
    assert_eq!(answer["Export"][2]["Ok"]["note"], "JSON");
    let data: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(data["center"]["x"], -1);
    std::fs::remove_file(path).unwrap();
    drop(engine.0.stdin.take());
    wait_exit(&mut engine.0);
}
