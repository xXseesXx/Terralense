//! Compact RGB palette sampled from the TFG instance's shipped ground textures.
use std::collections::HashMap;
use std::sync::OnceLock;

static COLORS: OnceLock<HashMap<String, [u8; 3]>> = OnceLock::new();
type TreeRgb = [Option<[u8; 3]>; 2];
static TREE_COLORS: OnceLock<HashMap<String, TreeRgb>> = OnceLock::new();

#[derive(serde::Deserialize)]
struct TreeColors {
    leaves: Option<[u8; 3]>,
    log_end: Option<[u8; 3]>,
    stripped_log_end: Option<[u8; 3]>,
    branch: Option<[u8; 3]>,
}

#[derive(serde::Deserialize)]
struct TreePalette {
    textures: HashMap<String, TreeColors>,
    features: HashMap<String, TreeFeatureColors>,
}

#[derive(serde::Deserialize)]
struct TreeFeatureColors {
    leaves: Option<String>,
    wood: Option<String>,
}

/// Texture-average display color for saved ground blocks. Unknown blocks fall back
/// to the sampler's semantic material color.
pub fn color(block: &str) -> Option<[u8; 3]> {
    let colors = COLORS.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/ground-colors/palette.json"))
            .expect("bundled ground texture palette is valid")
    });
    colors.get(block).copied().or_else(|| {
        if let Some(rock) = block.strip_prefix("tfc:rock/hardened/") {
            return colors.get(&format!("tfc:rock/raw/{rock}")).copied();
        }
        if block == "tfc:sea_ice" {
            return colors.get("minecraft:ice").copied();
        }
        let (prefix, soil) = block
            .strip_prefix("tfc:grass/")
            .map(|soil| ("tfc", soil))
            .or_else(|| block.strip_prefix("tfg:grass/").map(|soil| ("tfg", soil)))
            .or_else(|| {
                block
                    .strip_prefix("tfc:clay_grass/")
                    .map(|soil| ("tfc", soil))
            })
            .or_else(|| {
                block
                    .strip_prefix("tfg:clay_grass/")
                    .map(|soil| ("tfg", soil))
            })?;
        colors.get(&format!("{prefix}:dirt/{soil}")).copied()
    })
}

/// Return true for TFC grass-soil blocks whose top face requires the biome climate tint.
pub fn is_grass_soil(block: &str) -> bool {
    block.starts_with("tfc:grass/")
        || block.starts_with("tfc:clay_grass/")
        || block.starts_with("tfg:grass/")
        || block.starts_with("tfg:clay_grass/")
        || block == "minecraft:grass_block"
}

/// Xaero-like RGB modulation of a sampled block texture by its Minecraft tint.
/// Both operands are 8-bit sRGB channel values; texture coverage is handled by
/// the texture sample that produced `texture`.
#[inline]
pub fn multiply_tint(texture: [u8; 3], tint: [u8; 3]) -> [u8; 3] {
    // Xaero's MapPixel multiplies with Java float precision and truncates to int.
    std::array::from_fn(|i| (f32::from(tint[i]) * (f32::from(texture[i]) / 255.0)) as u8)
}

/// The sampled neutral overlay used on the exposed top face of TFC grass blocks.
pub fn grass_tint_overlay() -> Option<[u8; 3]> {
    color("tfc:grass_tint_overlay")
}

/// Sampled texture RGB for an exposed tree block, keyed by TFC tree species.
/// Leaf tint is applied separately with [`multiply_tint`].
pub fn tree_color(species: &str, leaves: bool) -> Option<[u8; 3]> {
    let colors = TREE_COLORS.get_or_init(|| {
        let palette: TreePalette =
            serde_json::from_str(include_str!("../assets/ground-colors/tree_palette.json"))
                .expect("bundled Xaero-compatible tree texture palette is valid");
        // Resolve feature → block → texture once. Map pixels then borrow the
        // canopy's existing species string without allocating keys or vectors.
        let mut features = palette.features.into_iter().collect::<Vec<_>>();
        features.sort_by(|a, b| a.0.cmp(&b.0));
        let mut colors = HashMap::new();
        for (name, feature) in features {
            let leaves = feature
                .leaves
                .as_ref()
                .and_then(|key| palette.textures.get(key))
                .and_then(|t| t.leaves);
            let wood = feature
                .wood
                .as_ref()
                .and_then(|key| palette.textures.get(key))
                .and_then(|t| t.log_end.or(t.stripped_log_end).or(t.branch));
            let rgb = [wood, leaves];
            // Saved chunks carry the bare species; procedural trees carry the
            // full feature ID. Sorted insertion keeps bare aliases deterministic.
            let bare = name.rsplit('/').next().unwrap_or(&name);
            colors.insert(bare.to_owned(), rgb);
            colors.insert(bare.replace('_', " "), rgb);
            if let Some(palm) = name
                .strip_prefix("tfg:palm_tree/")
                .and_then(|palm| palm.rsplit_once('_'))
                .filter(|(_, age)| age.parse::<u8>().is_ok())
                .map(|(palm, _)| palm)
            {
                // NBT feature variants carry an age suffix, but saved leaves
                // are named for the palm species alone.
                colors.insert(palm.to_owned(), rgb);
                colors.insert(palm.replace('_', " "), rgb);
                colors.insert(format!("tfg:tree/{palm}"), rgb);
            }
            colors.insert(name, rgb);
        }
        if let Some(trunk) = palette.textures.get("tfg:palm_tree/trunk_0") {
            colors.insert("tfg:tree/palm".into(), [trunk.log_end, None]);
        }
        colors
    });
    let index = usize::from(leaves);
    colors.get(species).and_then(|rgb| rgb[index]).or_else(|| {
        leaves
            .then(|| {
                ["_dead", "_large", "_krummholz"]
                    .into_iter()
                    .find_map(|suffix| {
                        species
                            .strip_suffix(suffix)
                            .and_then(|base| colors.get(base))
                            .and_then(|rgb| rgb[index])
                    })
            })
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::{multiply_tint, tree_color};

    #[test]
    fn tint_multiplies_sampled_texture_channels() {
        assert_eq!(multiply_tint([148, 195, 127], [93; 3]), [53, 71, 46]);
        assert_eq!(multiply_tint([255, 128, 64], [128, 255, 0]), [128, 128, 0]);
        assert_eq!(multiply_tint([255; 3], [37, 128, 241]), [37, 128, 241]);
    }

    #[test]
    fn tree_species_use_profiled_leaf_and_endgrain_textures() {
        assert!(tree_color("tfc:tree/oak", true).is_some());
        assert!(tree_color("tfc:tree/oak_dead", false).is_some());
    }

    #[test]
    fn saved_species_labels_and_palm_ages_resolve_texture_profiles() {
        assert_eq!(tree_color("acai", true), Some([134; 3]));
        assert_eq!(tree_color("tfg:tree/palm", false), Some([139, 104, 67]));
        assert_eq!(
            tree_color("tfc:tree/douglas_fir", true),
            tree_color("douglas fir", true)
        );
        assert!(tree_color("douglas fir", true).is_some());
    }
}
