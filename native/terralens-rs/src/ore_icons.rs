//! Small raw-ore item icons bundled from the local TerraFirmaGreg / GTCEu resource assets.
//! PNGs are embedded into the executable, then uploaded lazily the first time a vein type is shown.
#[cfg(feature = "gui")]
use eframe::egui::{self, TextureHandle};
#[cfg(feature = "gui")]
use std::collections::HashMap;

const ICONS: &[(&str, &[u8])] = &[
    (
        "beneath_aluminium",
        include_bytes!("../assets/ore-icons/beneath_aluminium.png"),
    ),
    (
        "beneath_bismuth",
        include_bytes!("../assets/ore-icons/beneath_bismuth.png"),
    ),
    (
        "beneath_coal_anthracite",
        include_bytes!("../assets/ore-icons/beneath_coal_anthracite.png"),
    ),
    (
        "beneath_coal_bituminous",
        include_bytes!("../assets/ore-icons/beneath_coal_bituminous.png"),
    ),
    (
        "beneath_coal_lignite",
        include_bytes!("../assets/ore-icons/lignite.png"),
    ),
    (
        "beneath_copper_native",
        include_bytes!("../assets/ore-icons/beneath_copper_native.png"),
    ),
    (
        "beneath_copper_sulfide",
        include_bytes!("../assets/ore-icons/beneath_copper_sulfide.png"),
    ),
    (
        "beneath_copper_tetrahedrite",
        include_bytes!("../assets/ore-icons/beneath_copper_tetrahedrite.png"),
    ),
    (
        "beneath_gold",
        include_bytes!("../assets/ore-icons/beneath_gold.png"),
    ),
    (
        "beneath_gold_rich",
        include_bytes!("../assets/ore-icons/beneath_gold_rich.png"),
    ),
    (
        "beneath_graphite",
        include_bytes!("../assets/ore-icons/beneath_graphite.png"),
    ),
    (
        "beneath_iron_banded",
        include_bytes!("../assets/ore-icons/beneath_iron_banded.png"),
    ),
    (
        "beneath_iron_hematite",
        include_bytes!("../assets/ore-icons/beneath_iron_hematite.png"),
    ),
    (
        "beneath_iron_magnetite",
        include_bytes!("../assets/ore-icons/beneath_iron_magnetite.png"),
    ),
    (
        "beneath_lead_silver",
        include_bytes!("../assets/ore-icons/beneath_lead_silver.png"),
    ),
    (
        "beneath_lithium",
        include_bytes!("../assets/ore-icons/beneath_lithium.png"),
    ),
    (
        "beneath_manganese",
        include_bytes!("../assets/ore-icons/beneath_manganese.png"),
    ),
    (
        "beneath_molydbenum",
        include_bytes!("../assets/ore-icons/beneath_molydbenum.png"),
    ),
    (
        "beneath_nickel",
        include_bytes!("../assets/ore-icons/beneath_nickel.png"),
    ),
    (
        "beneath_phlogopite",
        include_bytes!("../assets/ore-icons/beneath_phlogopite.png"),
    ),
    (
        "beneath_phosphate",
        include_bytes!("../assets/ore-icons/beneath_phosphate.png"),
    ),
    (
        "beneath_psg",
        include_bytes!("../assets/ore-icons/beneath_psg.png"),
    ),
    (
        "beneath_quartz",
        include_bytes!("../assets/ore-icons/beneath_quartz.png"),
    ),
    (
        "beneath_redstone",
        include_bytes!("../assets/ore-icons/beneath_redstone.png"),
    ),
    (
        "beneath_salt",
        include_bytes!("../assets/ore-icons/beneath_salt.png"),
    ),
    (
        "beneath_sulfur",
        include_bytes!("../assets/ore-icons/beneath_sulfur.png"),
    ),
    (
        "beneath_tin",
        include_bytes!("../assets/ore-icons/beneath_tin.png"),
    ),
    (
        "beneath_zinc",
        include_bytes!("../assets/ore-icons/beneath_zinc.png"),
    ),
    (
        "bituminous_coal",
        include_bytes!("../assets/ore-icons/bituminous_coal.png"),
    ),
    (
        "gt_diamond_2b2a24_2b1700",
        include_bytes!("../assets/ore-icons/gt_diamond_2b2a24_2b1700.png"),
    ),
    (
        "gt_diamond_ffb777_856f48",
        include_bytes!("../assets/ore-icons/gt_diamond_ffb777_856f48.png"),
    ),
    (
        "gt_fine_d5ffff_5a69d6",
        include_bytes!("../assets/ore-icons/gt_fine_d5ffff_5a69d6.png"),
    ),
    (
        "gt_fine_ecfeff_c2a03c",
        include_bytes!("../assets/ore-icons/gt_fine_ecfeff_c2a03c.png"),
    ),
    (
        "gt_metallic_ff9000_ff0000",
        include_bytes!("../assets/ore-icons/gt_metallic_ff9000_ff0000.png"),
    ),
    (
        "gt_shiny_f3e8fa_331d42",
        include_bytes!("../assets/ore-icons/gt_shiny_f3e8fa_331d42.png"),
    ),
    (
        "normal_bismuth",
        include_bytes!("../assets/ore-icons/normal_bismuthinite.png"),
    ),
    (
        "normal_limonite",
        include_bytes!("../assets/ore-icons/normal_limonite.png"),
    ),
    (
        "normal_tetrahedrite",
        include_bytes!("../assets/ore-icons/normal_tetrahedrite.png"),
    ),
    (
        "normal_tin",
        include_bytes!("../assets/ore-icons/normal_cassiterite.png"),
    ),
    (
        "planet_silver",
        include_bytes!("../assets/ore-icons/normal_native_silver.png"),
    ),
    (
        "planet_graphite",
        include_bytes!("../assets/ore-icons/graphite.png"),
    ),
    (
        "planet_gypsum",
        include_bytes!("../assets/ore-icons/gypsum.png"),
    ),
    (
        "planet_pyrite",
        include_bytes!("../assets/ore-icons/pyrite.png"),
    ),
    (
        "planet_emerald",
        include_bytes!("../assets/ore-icons/emerald.png"),
    ),
    (
        "planet_topaz",
        include_bytes!("../assets/ore-icons/topaz.png"),
    ),
    (
        "planet_lapis",
        include_bytes!("../assets/ore-icons/lapis_lazuli.png"),
    ),
    (
        "beneath_realgar",
        include_bytes!("../assets/ore-icons/beneath_realgar.png"),
    ),
    (
        "deep_apatite",
        include_bytes!("../assets/ore-icons/gt_diamond_06cdf1_701c07.png"),
    ),
    (
        "deep_asbestos",
        include_bytes!("../assets/ore-icons/normal_cassiterite.png"),
    ),
    (
        "deep_banded_iron",
        include_bytes!("../assets/ore-icons/gt_fine_97873a_313131.png"),
    ),
    (
        "deep_bauxite",
        include_bytes!("../assets/ore-icons/gt_diamond_2b2a24_2b1700.png"),
    ),
    (
        "deep_big_redstone",
        include_bytes!("../assets/ore-icons/lapis_lazuli.png"),
    ),
    (
        "deep_bismuth",
        include_bytes!("../assets/ore-icons/normal_bismuthinite.png"),
    ),
    (
        "deep_copper_iron",
        include_bytes!("../assets/ore-icons/gt_fine_96c185_e3af1a.png"),
    ),
    (
        "deep_copper_sulfide",
        include_bytes!("../assets/ore-icons/sulfur.png"),
    ),
    (
        "deep_copper_tin",
        include_bytes!("../assets/ore-icons/gt_fine_96c185_e3af1a.png"),
    ),
    (
        "deep_galena",
        include_bytes!("../assets/ore-icons/gt_shiny_f3e8fa_331d42.png"),
    ),
    (
        "deep_graphite",
        include_bytes!("../assets/ore-icons/graphite.png"),
    ),
    (
        "deep_graphite_volcanic",
        include_bytes!("../assets/ore-icons/graphite.png"),
    ),
    (
        "deep_iron",
        include_bytes!("../assets/ore-icons/gt_fine_97873a_313131.png"),
    ),
    (
        "deep_kyanite",
        include_bytes!("../assets/ore-icons/gt_fine_d5ffff_5a69d6.png"),
    ),
    (
        "deep_lazurite_calcite",
        include_bytes!("../assets/ore-icons/gt_diamond_2836f1_183ca3.png"),
    ),
    (
        "deep_magnetite",
        include_bytes!("../assets/ore-icons/normal_magnetite.png"),
    ),
    (
        "deep_magnetite_olivine",
        include_bytes!("../assets/ore-icons/gt_fine_f3efbb_b8d066.png"),
    ),
    (
        "deep_manganese",
        include_bytes!("../assets/ore-icons/gt_diamond_ffb777_856f48.png"),
    ),
    (
        "deep_molybdenum",
        include_bytes!("../assets/ore-icons/gt_metallic_ff9000_ff0000.png"),
    ),
    (
        "deep_monazite",
        include_bytes!("../assets/ore-icons/gt_diamond_caab60_c8502d.png"),
    ),
    (
        "deep_nickel",
        include_bytes!("../assets/ore-icons/normal_garnierite.png"),
    ),
    (
        "deep_redstone",
        include_bytes!("../assets/ore-icons/lapis_lazuli.png"),
    ),
    (
        "deep_saltpeter",
        include_bytes!("../assets/ore-icons/saltpeter.png"),
    ),
    (
        "deep_sapphire",
        include_bytes!("../assets/ore-icons/sapphire.png"),
    ),
    (
        "deep_scheelite",
        include_bytes!("../assets/ore-icons/gt_fine_d7e8b3_143cae.png"),
    ),
    (
        "deep_sphalerite",
        include_bytes!("../assets/ore-icons/normal_sphalerite.png"),
    ),
    (
        "deep_sulfur",
        include_bytes!("../assets/ore-icons/sulfur.png"),
    ),
    (
        "deep_tantalite",
        include_bytes!("../assets/ore-icons/gt_diamond_ffb777_856f48.png"),
    ),
    (
        "deep_thorianite",
        include_bytes!("../assets/ore-icons/gt_metallic_25411b_051e05.png"),
    ),
    (
        "deep_tin",
        include_bytes!("../assets/ore-icons/normal_cassiterite.png"),
    ),
    (
        "deep_topaz",
        include_bytes!("../assets/ore-icons/gt_diamond_dbfeff_a0c4d7.png"),
    ),
    (
        "diorite_dike",
        include_bytes!("../assets/ore-icons/rock_diorite.png"),
    ),
    (
        "gabbro_dike",
        include_bytes!("../assets/ore-icons/rock_gabbro.png"),
    ),
    (
        "granite_dike",
        include_bytes!("../assets/ore-icons/rock_granite.png"),
    ),
    (
        "gravel",
        include_bytes!("../assets/ore-icons/gravel_andesite.png"),
    ),
    (
        "high_bismuth_mountain",
        include_bytes!("../assets/ore-icons/normal_bismuthinite.png"),
    ),
    (
        "high_coal",
        include_bytes!("../assets/ore-icons/bituminous_coal.png"),
    ),
    (
        "high_gypsum",
        include_bytes!("../assets/ore-icons/gypsum.png"),
    ),
    (
        "high_lead",
        include_bytes!("../assets/ore-icons/gt_dull_7e6f82_290633.png"),
    ),
    (
        "high_realgar",
        include_bytes!("../assets/ore-icons/gt_fine_96c185_e3af1a.png"),
    ),
    (
        "high_sulfur",
        include_bytes!("../assets/ore-icons/sulfur.png"),
    ),
    (
        "high_tetrahedrite",
        include_bytes!("../assets/ore-icons/normal_tetrahedrite.png"),
    ),
    (
        "high_tin",
        include_bytes!("../assets/ore-icons/normal_cassiterite.png"),
    ),
    (
        "kaolin_disc",
        include_bytes!("../assets/ore-icons/kaolin_clay.png"),
    ),
    (
        "normal_amethyst",
        include_bytes!("../assets/ore-icons/amethyst.png"),
    ),
    (
        "normal_asbestos_dry",
        include_bytes!("../assets/ore-icons/normal_cassiterite.png"),
    ),
    (
        "normal_coal",
        include_bytes!("../assets/ore-icons/lignite.png"),
    ),
    (
        "normal_copper",
        include_bytes!("../assets/ore-icons/gt_fine_96c185_e3af1a.png"),
    ),
    (
        "normal_copper_gypsum",
        include_bytes!("../assets/ore-icons/gt_dull_657882_33302e.png"),
    ),
    (
        "normal_galena",
        include_bytes!("../assets/ore-icons/gt_shiny_f3e8fa_331d42.png"),
    ),
    (
        "normal_garnierite",
        include_bytes!("../assets/ore-icons/normal_garnierite.png"),
    ),
    (
        "normal_gold",
        include_bytes!("../assets/ore-icons/normal_native_gold.png"),
    ),
    (
        "normal_iron",
        include_bytes!("../assets/ore-icons/gt_metallic_f5e315_c06f33.png"),
    ),
    (
        "normal_mica",
        include_bytes!("../assets/ore-icons/gt_fine_ecfeff_c2a03c.png"),
    ),
    (
        "normal_quartz",
        include_bytes!("../assets/ore-icons/gt_fine_e8e2d1_f4b74b.png"),
    ),
    (
        "normal_quartz_borax",
        include_bytes!("../assets/ore-icons/borax.png"),
    ),
    (
        "normal_salt",
        include_bytes!("../assets/ore-icons/halite.png"),
    ),
    (
        "normal_sphalerite",
        include_bytes!("../assets/ore-icons/normal_sphalerite.png"),
    ),
    (
        "normal_spodumene",
        include_bytes!("../assets/ore-icons/gt_fine_ffbcbc_c490ff.png"),
    ),
    (
        "normal_sulfide",
        include_bytes!("../assets/ore-icons/gt_shiny_ffe05a_442602.png"),
    ),
    (
        "normal_tarkianite",
        include_bytes!("../assets/ore-icons/gt_diamond_8bb054_3d8021.png"),
    ),
    (
        "sand_basaltic",
        include_bytes!("../assets/ore-icons/gt_fine_5c5c5c_283228.png"),
    ),
    (
        "sand_garnet",
        include_bytes!("../assets/ore-icons/gt_fine_cc4c25_510b04.png"),
    ),
    (
        "sand_granitic",
        include_bytes!("../assets/ore-icons/gt_fine_d69077_71352c.png"),
    ),
    (
        "sand_tin",
        include_bytes!("../assets/ore-icons/gt_fine_89847e_3b3b35.png"),
    ),
    (
        "surface_bismuth",
        include_bytes!("../assets/ore-icons/normal_bismuthinite.png"),
    ),
    (
        "surface_coal",
        include_bytes!("../assets/ore-icons/lignite.png"),
    ),
    (
        "surface_cobaltite",
        include_bytes!("../assets/ore-icons/normal_garnierite.png"),
    ),
    (
        "surface_gold",
        include_bytes!("../assets/ore-icons/normal_native_gold.png"),
    ),
    (
        "surface_hematite",
        include_bytes!("../assets/ore-icons/normal_hematite.png"),
    ),
    (
        "surface_limonite",
        include_bytes!("../assets/ore-icons/gt_metallic_f5e315_c06f33.png"),
    ),
    (
        "surface_magnetite",
        include_bytes!("../assets/ore-icons/normal_magnetite.png"),
    ),
    (
        "surface_malachite",
        include_bytes!("../assets/ore-icons/normal_malachite.png"),
    ),
    (
        "surface_mica",
        include_bytes!("../assets/ore-icons/gt_fine_ecfeff_c2a03c.png"),
    ),
    (
        "surface_native_copper",
        include_bytes!("../assets/ore-icons/normal_native_copper.png"),
    ),
    (
        "surface_nickel",
        include_bytes!("../assets/ore-icons/normal_garnierite.png"),
    ),
    (
        "surface_salpeter",
        include_bytes!("../assets/ore-icons/saltpeter.png"),
    ),
    (
        "surface_sphalerite",
        include_bytes!("../assets/ore-icons/normal_sphalerite.png"),
    ),
    (
        "surface_sylvite_salt",
        include_bytes!("../assets/ore-icons/halite.png"),
    ),
    (
        "surface_tetrahedrite",
        include_bytes!("../assets/ore-icons/normal_tetrahedrite.png"),
    ),
    (
        "surface_tricalcium_phosphate",
        include_bytes!("../assets/ore-icons/gt_fine_fffddb_ffff00.png"),
    ),
];
#[cfg(test)]
const ALL_VEIN_KEYS: &[&str] = &[
    "deep_apatite",
    "deep_asbestos",
    "deep_banded_iron",
    "deep_bauxite",
    "deep_big_redstone",
    "deep_bismuth",
    "deep_copper_iron",
    "deep_copper_sulfide",
    "deep_copper_tin",
    "deep_galena",
    "deep_graphite",
    "deep_graphite_volcanic",
    "deep_iron",
    "deep_kyanite",
    "deep_lazurite_calcite",
    "deep_magnetite",
    "deep_magnetite_olivine",
    "deep_manganese",
    "deep_molybdenum",
    "deep_monazite",
    "deep_nickel",
    "deep_redstone",
    "deep_saltpeter",
    "deep_sapphire",
    "deep_scheelite",
    "deep_sphalerite",
    "deep_sulfur",
    "deep_tantalite",
    "deep_thorianite",
    "deep_tin",
    "deep_topaz",
    "diorite_dike",
    "gabbro_dike",
    "granite_dike",
    "gravel",
    "high_bismuth_mountain",
    "high_coal",
    "high_gypsum",
    "high_lead",
    "high_realgar",
    "high_sulfur",
    "high_tetrahedrite",
    "high_tin",
    "kaolin_disc",
    "normal_amethyst",
    "normal_asbestos_dry",
    "normal_coal",
    "normal_copper",
    "normal_copper_gypsum",
    "normal_galena",
    "normal_garnierite",
    "normal_gold",
    "normal_iron",
    "normal_mica",
    "normal_quartz",
    "normal_quartz_borax",
    "normal_salt",
    "normal_sphalerite",
    "normal_spodumene",
    "normal_sulfide",
    "normal_tarkianite",
    "sand_basaltic",
    "sand_garnet",
    "sand_granitic",
    "sand_tin",
    "surface_bismuth",
    "surface_coal",
    "surface_cobaltite",
    "surface_gold",
    "surface_hematite",
    "surface_limonite",
    "surface_magnetite",
    "surface_malachite",
    "surface_mica",
    "surface_native_copper",
    "surface_nickel",
    "surface_salpeter",
    "surface_sphalerite",
    "surface_sylvite_salt",
    "surface_tetrahedrite",
    "surface_tricalcium_phosphate",
];

#[cfg(feature = "gui")]
#[derive(Default)]
pub struct IconCache {
    textures: HashMap<&'static str, TextureHandle>,
}
#[cfg(feature = "gui")]
impl IconCache {
    /// Returns a cached GPU texture for the stable part of an ore marker's id.
    pub fn get(&mut self, ctx: &egui::Context, vein_id: &str) -> Option<&TextureHandle> {
        let vein_id = vein_id.split('@').next().unwrap_or(vein_id);
        let lookup = beneath_icon(vein_id)
            .or_else(|| planetary_icon(vein_id))
            .unwrap_or(vein_id);
        let (key, bytes) = ICONS.iter().find(|(key, _)| *key == lookup)?;
        self.textures.entry(*key).or_insert_with(|| {
            let rgba = image::load_from_memory(bytes)
                .expect("bundled ore icon must be valid PNG")
                .to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba.into_raw());
            ctx.load_texture(
                format!("ore-icon:{key}"),
                image,
                egui::TextureOptions::NEAREST,
            )
        });
        self.textures.get(key)
    }
}

/// Bundled PNG bytes for a vein icon, usable by both the viewer and headless exporters.
pub fn icon_png(vein_id: &str) -> Option<&'static [u8]> {
    let id = vein_id.split('@').next().unwrap_or(vein_id);
    let lookup = beneath_icon(id)
        .or_else(|| planetary_icon(id))
        .unwrap_or(id);
    ICONS
        .iter()
        .find(|(key, _)| *key == lookup)
        .map(|(_, bytes)| *bytes)
}

/// Material name represented by the icon; useful in marker lists and tooltips.
pub fn material_name(vein_id: &str) -> Option<&'static str> {
    let vein_id = vein_id.split('@').next().unwrap_or(vein_id);
    if let Some((_, material)) = BENEATH_MATERIALS.iter().find(|(id, _)| *id == vein_id) {
        return Some(material);
    }
    if let Some((_, material)) = PLANETARY_MATERIALS.iter().find(|(id, _)| *id == vein_id) {
        return Some(material);
    }
    match vein_id {
        "deep_apatite" => Some("apatite"),
        "deep_asbestos" => Some("cassiterite"),
        "deep_banded_iron" => Some("goethite"),
        "deep_bauxite" => Some("ilmenite"),
        "deep_big_redstone" => Some("redstone"),
        "deep_bismuth" => Some("bismuth"),
        "deep_copper_iron" => Some("chalcopyrite"),
        "deep_copper_sulfide" => Some("sulfur"),
        "deep_copper_tin" => Some("chalcopyrite"),
        "deep_galena" => Some("galena"),
        "deep_graphite" => Some("graphite"),
        "deep_graphite_volcanic" => Some("graphite"),
        "deep_iron" => Some("goethite"),
        "deep_kyanite" => Some("kyanite"),
        "deep_lazurite_calcite" => Some("lazurite"),
        "deep_magnetite" => Some("magnetite"),
        "deep_magnetite_olivine" => Some("fullers earth"),
        "deep_manganese" => Some("grossular"),
        "deep_molybdenum" => Some("wulfenite"),
        "deep_monazite" => Some("bastnasite"),
        "deep_nickel" => Some("garnierite"),
        "deep_redstone" => Some("redstone"),
        "deep_saltpeter" => Some("saltpeter"),
        "deep_sapphire" => Some("sapphire"),
        "deep_scheelite" => Some("scheelite"),
        "deep_sphalerite" => Some("sphalerite"),
        "deep_sulfur" => Some("sulfur"),
        "deep_tantalite" => Some("grossular"),
        "deep_thorianite" => Some("thorium"),
        "deep_tin" => Some("cassiterite"),
        "deep_topaz" => Some("blue topaz"),
        "diorite_dike" => Some("diorite loose rock"),
        "gabbro_dike" => Some("gabbro loose rock"),
        "granite_dike" => Some("granite loose rock"),
        "gravel" => Some("andesite gravel"),
        "high_bismuth_mountain" => Some("bismuth"),
        "high_coal" => Some("coal"),
        "high_gypsum" => Some("gypsum"),
        "high_lead" => Some("lead"),
        "high_realgar" => Some("chalcopyrite"),
        "high_sulfur" => Some("sulfur"),
        "high_tetrahedrite" => Some("tetrahedrite"),
        "high_tin" => Some("cassiterite"),
        "kaolin_disc" => Some("kaolin clay"),
        "normal_amethyst" => Some("amethyst"),
        "normal_asbestos_dry" => Some("cassiterite"),
        "normal_coal" => Some("lignite"),
        "normal_copper" => Some("chalcopyrite"),
        "normal_copper_gypsum" => Some("chalcocite"),
        "normal_galena" => Some("galena"),
        "normal_garnierite" => Some("garnierite"),
        "normal_gold" => Some("gold"),
        "normal_iron" => Some("yellow limonite"),
        "normal_mica" => Some("mica"),
        "normal_quartz" => Some("barite"),
        "normal_quartz_borax" => Some("borax"),
        "normal_salt" => Some("salt"),
        "normal_sphalerite" => Some("sphalerite"),
        "normal_spodumene" => Some("spodumene"),
        "normal_sulfide" => Some("bornite"),
        "normal_tarkianite" => Some("tarkianite"),
        "sand_basaltic" => Some("basaltic mineral sand"),
        "sand_garnet" => Some("garnet sand"),
        "sand_granitic" => Some("granitic mineral sand"),
        "sand_tin" => Some("cassiterite sand"),
        "surface_bismuth" => Some("bismuth"),
        "surface_coal" => Some("lignite"),
        "surface_cobaltite" => Some("cobaltite"),
        "surface_gold" => Some("gold"),
        "surface_hematite" => Some("hematite"),
        "surface_limonite" => Some("yellow limonite"),
        "surface_magnetite" => Some("magnetite"),
        "surface_malachite" => Some("malachite"),
        "surface_mica" => Some("mica"),
        "surface_native_copper" => Some("copper"),
        "surface_nickel" => Some("garnierite"),
        "surface_salpeter" => Some("saltpeter"),
        "surface_sphalerite" => Some("sphalerite"),
        "surface_sylvite_salt" => Some("salt"),
        "surface_tetrahedrite" => Some("tetrahedrite"),
        "surface_tricalcium_phosphate" => Some("tricalcium phosphate"),
        _ => None,
    }
}

/// Exact in-game raw mineral art available for planetary veins. Configurations whose dominant
/// output has no matching bundled raw-material sprite intentionally return `None`.
fn planetary_icon(id: &str) -> Option<&'static str> {
    match id {
        "moon:moon_gold" | "mars:mars_gold" => Some("normal_gold"),
        "moon:moon_silver" => Some("planet_silver"),
        "moon:moon_graphite" | "mars:mars_graphite" => Some("planet_graphite"),
        "moon:moon_beryllium" | "mars:mars_beryllium" => Some("planet_emerald"),
        "moon:moon_gypsum" => Some("planet_gypsum"),
        "moon:moon_sphalerite" => Some("normal_sphalerite"),
        "moon:moon_tetrahedrite" | "mars:mars_stibnite" | "mars:mars_surface_tetrahedrite" => {
            Some("normal_tetrahedrite")
        }
        "moon:moon_redstone" => Some("beneath_redstone"),
        "moon:moon_topaz" => Some("planet_topaz"),
        "moon:moon_cassiterite" | "mars:mars_surface_cassiterite" => Some("normal_tin"),
        "mars:mars_coal" => Some("bituminous_coal"),
        "mars:mars_galena" | "mars:mars_surface_nickel_galena" => Some("normal_galena"),
        "mars:mars_hematite" | "mars:mars_surface_hematite" => Some("surface_hematite"),
        "mars:mars_nickel" | "moon:moon_garnierite" => Some("normal_garnierite"),
        "mars:mars_lapis" => Some("planet_lapis"),
        "mars:mars_salt" => Some("normal_spodumene"),
        "mars:mars_sulfur" => Some("planet_pyrite"),
        "mars:mars_surface_bismuthinite" => Some("normal_bismuth"),
        _ => None,
    }
}

const PLANETARY_MATERIALS: &[(&str, &str)] = &[
    ("moon:moon_gold", "gold"),
    ("mars:mars_gold", "gold"),
    ("moon:moon_silver", "silver"),
    ("moon:moon_graphite", "graphite"),
    ("mars:mars_graphite", "graphite"),
    ("moon:moon_beryllium", "emerald"),
    ("mars:mars_beryllium", "emerald"),
    ("moon:moon_gypsum", "gypsum"),
    ("moon:moon_sphalerite", "sphalerite"),
    ("moon:moon_tetrahedrite", "tetrahedrite"),
    ("mars:mars_stibnite", "tetrahedrite"),
    ("mars:mars_surface_tetrahedrite", "tetrahedrite"),
    ("moon:moon_redstone", "redstone"),
    ("moon:moon_topaz", "topaz"),
    ("moon:moon_cassiterite", "cassiterite"),
    ("mars:mars_surface_cassiterite", "cassiterite"),
    ("mars:mars_coal", "coal"),
    ("mars:mars_galena", "galena"),
    ("mars:mars_surface_nickel_galena", "galena"),
    ("mars:mars_hematite", "hematite"),
    ("mars:mars_surface_hematite", "hematite"),
    ("mars:mars_nickel", "garnierite"),
    ("moon:moon_garnierite", "garnierite"),
    ("mars:mars_lapis", "lazurite"),
    ("mars:mars_salt", "spodumene"),
    ("mars:mars_sulfur", "pyrite"),
    ("mars:mars_surface_bismuthinite", "bismuthinite"),
];

// The normal UI and the Beneath share the bundled, in-game raw mineral sprites. IDs resolve to
// their dominant vein mineral; composite secondary ores remain listed in the marker tooltip.
const BENEATH_MATERIALS: &[(&str, &str)] = &[
    ("beneath_aluminium", "alunite"),
    ("beneath_bismuth", "bismuth"),
    ("beneath_coal_anthracite", "anthracite"),
    ("beneath_coal_bituminous", "coal"),
    ("beneath_coal_lignite", "lignite"),
    ("beneath_copper_native", "copper"),
    ("beneath_copper_sulfide", "chalcocite"),
    ("beneath_copper_tetrahedrite", "tetrahedrite"),
    ("beneath_gold", "gold"),
    ("beneath_gold_rich", "gold"),
    ("beneath_graphite", "graphite"),
    ("beneath_iron_banded", "goethite"),
    ("beneath_iron_hematite", "hematite"),
    ("beneath_iron_magnetite", "magnetite"),
    ("beneath_lead_silver", "galena"),
    ("beneath_lithium", "spodumene"),
    ("beneath_manganese", "pyrolusite"),
    ("beneath_molydbenum", "wulfenite"),
    ("beneath_nickel", "pentlandite"),
    ("beneath_phlogopite", "phlogopite"),
    ("beneath_phosphate", "tricalcium phosphate"),
    ("beneath_psg", "irarsite"),
    ("beneath_quartz", "nether quartz"),
    ("beneath_realgar", "realgar"),
    ("beneath_redstone", "redstone"),
    ("beneath_salt", "salt"),
    ("beneath_sulfur", "sulfur"),
    ("beneath_tin", "cassiterite"),
    ("beneath_zinc", "sphalerite"),
];
fn beneath_icon(vein_id: &str) -> Option<&'static str> {
    ICONS
        .iter()
        .find(|(key, _)| *key == vein_id && key.starts_with("beneath_"))
        .map(|(key, _)| *key)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_embedded_tfg_vein_has_a_bundled_icon() {
        for key in ALL_VEIN_KEYS {
            assert!(
                ICONS.iter().any(|(id, _)| id == key),
                "missing icon for {key}"
            );
            assert!(material_name(key).is_some(), "missing material for {key}");
        }
        for (id, bytes) in ICONS {
            let icon =
                image::load_from_memory(bytes).unwrap_or_else(|e| panic!("invalid {id}: {e}"));
            assert!((1..=64).contains(&icon.width()));
            assert!((1..=64).contains(&icon.height()));
        }
        for (id, _) in BENEATH_MATERIALS {
            let resolved = beneath_icon(id).expect("Beneath icon mapping");
            assert!(
                ICONS.iter().any(|(icon, _)| *icon == resolved),
                "missing bundled icon for {id} -> {resolved}"
            );
        }
    }
    #[cfg(feature = "gui")]
    #[test]
    fn placed_veins_reuse_their_material_texture() {
        let ctx = egui::Context::default();
        let mut cache = IconCache::default();
        let first = cache.get(&ctx, "normal_gold@-20,13").unwrap().id();
        let second = cache.get(&ctx, "normal_gold@900,-400").unwrap().id();
        assert_eq!(first, second);
        assert_eq!(cache.textures.len(), 1);
    }

    #[test]
    fn beneath_labels_are_the_dominant_configured_ore_materials() {
        let expected = [
            ("beneath_aluminium", "alunite"),
            ("beneath_bismuth", "bismuth"),
            ("beneath_coal_anthracite", "anthracite"),
            ("beneath_coal_bituminous", "coal"),
            ("beneath_coal_lignite", "lignite"),
            ("beneath_copper_native", "copper"),
            ("beneath_copper_sulfide", "chalcocite"),
            ("beneath_copper_tetrahedrite", "tetrahedrite"),
            ("beneath_gold", "gold"),
            ("beneath_gold_rich", "gold"),
            ("beneath_graphite", "graphite"),
            ("beneath_iron_banded", "goethite"),
            ("beneath_iron_hematite", "hematite"),
            ("beneath_iron_magnetite", "magnetite"),
            ("beneath_lead_silver", "galena"),
            ("beneath_lithium", "spodumene"),
            ("beneath_manganese", "pyrolusite"),
            ("beneath_molydbenum", "wulfenite"),
            ("beneath_nickel", "pentlandite"),
            ("beneath_phlogopite", "phlogopite"),
            ("beneath_phosphate", "tricalcium phosphate"),
            ("beneath_psg", "irarsite"),
            ("beneath_quartz", "nether quartz"),
            ("beneath_realgar", "realgar"),
            ("beneath_redstone", "redstone"),
            ("beneath_salt", "salt"),
            ("beneath_sulfur", "sulfur"),
            ("beneath_tin", "cassiterite"),
            ("beneath_zinc", "sphalerite"),
        ];
        assert_eq!(BENEATH_MATERIALS, expected);
        for (id, _) in expected {
            let resolved = beneath_icon(id).unwrap();
            assert_eq!(resolved, id);
            assert!(ICONS.iter().any(|(key, _)| *key == resolved));
        }
    }

    #[test]
    fn planetary_ore_icons_use_only_known_raw_material_art() {
        for id in [
            "moon:moon_gold@1,2",
            "moon:moon_silver@1,2",
            "mars:mars_graphite@1,2",
            "mars:mars_hematite@1,2",
        ] {
            assert!(
                icon_png(id).is_some(),
                "missing known planetary art for {id}"
            );
            assert!(material_name(id).is_some(), "missing material for {id}");
        }
        assert!(icon_png("moon:moon_apatite@1,2").is_none());
    }
}
