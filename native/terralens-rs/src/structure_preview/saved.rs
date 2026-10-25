//! Read-only extraction of a confirmed structure from a generated world.
use super::StructurePreview;
use crate::model::Marker;
use std::path::Path;

pub fn read(
    world_path: &Path,
    marker: &Marker,
    cancel: &impl Fn() -> bool,
) -> Result<Option<StructurePreview>, String> {
    crate::prism::read_saved_structure_preview(world_path, marker, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{MarkerKind, StructureBounds},
        structure_preview::BlockPos,
    };
    use flate2::{Compression, write::GzEncoder};
    use std::{fs, io::Write, path::PathBuf};

    fn name(s: &str) -> Vec<u8> {
        let mut out = (s.len() as u16).to_be_bytes().to_vec();
        out.extend(s.as_bytes());
        out
    }
    fn string(name_: &str, value: &str) -> Vec<u8> {
        let mut out = vec![8];
        out.extend(name(name_));
        out.extend((value.len() as u16).to_be_bytes());
        out.extend(value.as_bytes());
        out
    }
    fn int(name_: &str, value: i32) -> Vec<u8> {
        let mut out = vec![3];
        out.extend(name(name_));
        out.extend(value.to_be_bytes());
        out
    }
    fn long(name_: &str, value: i64) -> Vec<u8> {
        let mut out = vec![4];
        out.extend(name(name_));
        out.extend(value.to_be_bytes());
        out
    }
    fn byte(name_: &str, value: i8) -> Vec<u8> {
        let mut out = vec![1];
        out.extend(name(name_));
        out.push(value as u8);
        out
    }
    fn int_array(name_: &str, values: &[i32]) -> Vec<u8> {
        let mut out = vec![11];
        out.extend(name(name_));
        out.extend((values.len() as i32).to_be_bytes());
        for value in values {
            out.extend(value.to_be_bytes());
        }
        out
    }
    fn long_array(name_: &str, values: &[u64]) -> Vec<u8> {
        let mut out = vec![12];
        out.extend(name(name_));
        out.extend((values.len() as i32).to_be_bytes());
        for value in values {
            out.extend(value.to_be_bytes());
        }
        out
    }
    fn compound_payload(mut body: Vec<u8>) -> Vec<u8> {
        body.push(0);
        body
    }
    fn compound(name_: &str, body: Vec<u8>) -> Vec<u8> {
        let mut out = vec![10];
        out.extend(name(name_));
        out.extend(compound_payload(body));
        out
    }
    fn list_compounds(name_: &str, children: Vec<Vec<u8>>) -> Vec<u8> {
        let mut out = vec![9];
        out.extend(name(name_));
        out.push(10);
        out.extend((children.len() as i32).to_be_bytes());
        for child in children {
            out.extend(compound_payload(child));
        }
        out
    }
    fn block_palette() -> Vec<u8> {
        let mut out = vec![9];
        out.extend(name("palette"));
        out.push(10);
        out.extend(4i32.to_be_bytes());
        out.extend(compound_payload(string("Name", "minecraft:air")));
        out.extend(compound_payload(string("Name", "tfc:ceramic/large_vessel")));
        out.extend(compound_payload(string("Name", "minecraft:stone")));
        out.extend(compound_payload(string("Name", "minecraft:chest")));
        out
    }
    fn packed_states() -> Vec<u8> {
        let mut words = vec![0u64; 256];
        words[0] |= 1; // local (0,0,0)
        // local (1,1,1): (y*16 + z)*16 + x = 273
        words[273 / 16] |= 3u64 << 4;
        let mut states = block_palette();
        states.extend(long_array("data", &words));
        compound("block_states", states)
    }
    fn structure_start() -> Vec<u8> {
        let child = |bb: &[i32], template: &str| {
            let mut body = int_array("BB", bb);
            body.extend(compound("pool_element", string("location", template)));
            body
        };
        let mut start = string("id", "tfc:test_site");
        start.extend(int("ChunkX", 0));
        start.extend(int("ChunkZ", 0));
        start.extend(int_array("BB", &[0, 64, 0, 33, 65, 1]));
        start.extend(list_compounds(
            "Children",
            vec![
                child(&[0, 64, 0, 1, 65, 1], "tfc:test/first"),
                child(&[32, 64, 0, 33, 65, 1], "tfc:test/second"),
            ],
        ));
        compound("tfc:test_site", start)
    }
    fn saved_containers() -> Vec<u8> {
        let item = {
            let mut body = string("id", "tfc:straw");
            body.extend(byte("Count", 6));
            body.extend(byte("Slot", 2));
            let mut tag_metadata = int("Damage", 22);
            let mut fluid = string("Fluid", "tfg:compressed_heliox_3");
            fluid.extend(long("Amount", 1000));
            let botarium = compound("BotariumData", list_compounds("StoredFluids", vec![fluid]));
            let enchant = {
                let mut enchant = string("id", "minecraft:efficiency");
                enchant.extend(int("lvl", 3));
                enchant
            };
            let enchants = list_compounds("StoredEnchantments", vec![enchant]);
            let display = compound("display", string("Name", r#"{"text":"Field Tank"}"#));
            tag_metadata.extend(botarium);
            tag_metadata.extend(enchants);
            tag_metadata.extend(display);
            tag_metadata.extend(int("work", 4));
            tag_metadata.extend(string("metal", "tfc:steel"));
            body.extend(compound("tag", tag_metadata));
            body
        };
        let inventory = compound("inventory", list_compounds("Items", vec![item]));
        let mut vessel = string("id", "tfc:large_vessel");
        vessel.extend(int("x", 0));
        vessel.extend(int("y", 64));
        vessel.extend(int("z", 0));
        vessel.extend(inventory);
        let mut chest = string("id", "lootr:lootr_chest");
        chest.extend(int("x", 1));
        chest.extend(int("y", 65));
        chest.extend(int("z", 1));
        chest.extend(int_array(
            "tileId",
            &[
                0x01234567,
                0x89abcdefu32 as i32,
                0x01234567,
                0x89abcdefu32 as i32,
            ],
        ));
        chest.extend(string(
            "specialLootChest_table",
            "minecraft:chests/simple_dungeon",
        ));
        chest.extend(long("specialLootChest_seed", 123456789));
        let mut list = vec![9];
        list.extend(name("block_entities"));
        list.push(10);
        list.extend(2i32.to_be_bytes());
        list.extend(compound_payload(vessel));
        list.extend(compound_payload(chest));
        list
    }
    fn chunk_nbt() -> Vec<u8> {
        let mut section = int("Y", 4);
        section.extend(packed_states());
        let mut body = string("Status", "minecraft:full");
        body.extend(list_compounds("sections", vec![section]));
        body.extend(saved_containers());
        body.extend(compound(
            "structures",
            compound("starts", structure_start()),
        ));
        compound("", body)
    }
    fn write_anvil(path: &Path, chunk_x: i32, chunk_z: i32, raw: &[u8]) {
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(raw).unwrap();
        let compressed = gzip.finish().unwrap();
        let record_len = compressed.len() + 1;
        let sectors = (record_len + 4).div_ceil(4096);
        let slot = chunk_x.rem_euclid(32) as usize + chunk_z.rem_euclid(32) as usize * 32;
        let mut mca = vec![0u8; (2 + sectors) * 4096];
        let sector = 2u32;
        mca[slot * 4] = (sector >> 16) as u8;
        mca[slot * 4 + 1] = (sector >> 8) as u8;
        mca[slot * 4 + 2] = sector as u8;
        mca[slot * 4 + 3] = sectors as u8;
        let base = sector as usize * 4096;
        mca[base..base + 4].copy_from_slice(&(record_len as u32).to_be_bytes());
        mca[base + 4] = 1;
        mca[base + 5..base + 5 + compressed.len()].copy_from_slice(&compressed);
        fs::write(path, mca).unwrap();
    }
    fn test_root() -> PathBuf {
        std::env::temp_dir().join(format!("terralens-preview-saved-{}", std::process::id()))
    }
    fn marker() -> Marker {
        Marker {
            id: "tfc:test_site".into(),
            label: "Test site".into(),
            namespace: "tfc".into(),
            x: 8.,
            z: 8.,
            y: Some(64.),
            kind: MarkerKind::Structure,
            confirmed: true,
            min_y: Some(64.),
            max_y: Some(65.),
            detail: String::new(),
            color: [1, 2, 3],
            bounds: Some(StructureBounds {
                min_x: 0,
                min_y: 64,
                min_z: 0,
                max_x: 33,
                max_y: 65,
                max_z: 1,
            }),
        }
    }

    #[test]
    fn saved_structure_preview_keeps_piece_bounds_nested_inventory_and_lootr_uuid() {
        let root = test_root();
        let world = root.join("world");
        fs::create_dir_all(world.join("region")).unwrap();
        write_anvil(&world.join("region/r.0.0.mca"), 0, 0, &chunk_nbt());
        let preview = read(&world, &marker(), &|| false).unwrap().unwrap();
        assert_eq!(preview.pieces.len(), 2);
        assert_eq!(
            preview.pieces[0].template.as_deref(),
            Some("tfc:test/first")
        );
        assert_eq!(preview.containers.len(), 2);
        let vessel = &preview.containers[0];
        assert_eq!(vessel.block, "tfc:ceramic/large_vessel");
        assert_eq!(vessel.items.len(), 1);
        assert_eq!(
            (
                vessel.items[0].id.as_str(),
                vessel.items[0].count,
                vessel.items[0].slot
            ),
            ("tfc:straw", 6, Some(2))
        );
        assert!(vessel.items[0].description.contains("Name: Field Tank"));
        assert!(
            vessel.items[0]
                .description
                .contains("Fluid: 1000 mB tfg:compressed_heliox_3")
        );
        assert!(
            vessel.items[0]
                .description
                .contains("Enchantment: efficiency 3")
        );
        assert!(vessel.items[0].description.contains("Damage: 22"));
        assert!(vessel.items[0].description.contains("work: 4.00"));
        assert!(vessel.items[0].description.contains("metal: tfc:steel"));
        assert!(vessel.lootr_id.is_none());
        let lootr = &preview.containers[1];
        assert!(lootr.lootr);
        assert_eq!(
            lootr.lootr_id.as_deref(),
            Some("01234567-89ab-cdef-0123-456789abcdef")
        );
        assert_eq!(
            lootr.loot_table.as_deref(),
            Some("minecraft:chests/simple_dungeon")
        );
        assert_eq!(lootr.loot_seed, Some(123456789));
        assert!(
            !lootr.contents_known,
            "an unopened Lootr inventory isn't known"
        );
        assert!(
            preview
                .blocks
                .iter()
                .any(|b| b.pos == BlockPos { x: 0, y: 64, z: 0 })
        );
        assert!(!preview.complete);
        assert!(
            preview
                .notes
                .iter()
                .any(|note| note.contains("not fully generated"))
        );
        fs::remove_dir_all(root).unwrap();
    }
}
