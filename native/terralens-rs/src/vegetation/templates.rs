//! Installed TFC tree feature metadata and exact horizontal block footprints.
//!
//! The masks are projected from the real structure-template block lists shipped with
//! TFC 3.2.25. Each point is relative to the feature origin after TFC's random mirror
//! and rotation have been applied. The mask includes log/branch/leaf blocks, so crowns
//! retain their authored shape across chunk boundaries.

use super::FeatureRandom;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const DATA: &str = include_str!("assets/tree_templates.json");

#[derive(Debug, Deserialize)]
struct Data {
    #[serde(default)]
    entries: BTreeMap<String, TreeEntry>,
    #[serde(default)]
    feature_order: Vec<String>,
    features: BTreeMap<String, TreeFeature>,
    #[serde(default)]
    structures: BTreeMap<String, TemplateVariant>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TreeEntry {
    pub normal_tree: Option<String>,
    pub dead_tree: Option<String>,
    pub old_growth_tree: Option<String>,
    pub krummholz: Option<String>,
    pub old_growth_chance: Option<u32>,
    pub spoiler_old_growth_chance: Option<u32>,
    pub dead_chance: Option<u32>,
    pub climate: ClimateRange,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ClimateRange {
    pub min_temperature: f32,
    pub max_temperature: f32,
    pub min_rainfall: f32,
    pub max_rainfall: f32,
}

#[derive(Debug, Deserialize)]
struct TreeFeature {
    #[serde(default)]
    feature_type: TreeFeatureType,
    width: u8,
    height: u8,
    trunk: Option<TreeTrunk>,
    root_system: Option<TreeRoots>,
    #[serde(default)]
    krummholz: Option<KrummholzConfig>,
    #[serde(default)]
    layers: Vec<TreeLayer>,
    #[serde(default)]
    overlay: Option<TreeOverlay>,
    variants: Vec<TemplateVariant>,
}

#[derive(Clone, Debug, Deserialize)]
struct TreeLayer {
    min_count: u8,
    max_count: u8,
    templates: Vec<TemplateVariant>,
}

#[derive(Clone, Debug, Deserialize)]
struct TreeOverlay {
    base: TemplateVariant,
    overlay: TemplateVariant,
    integrity: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TreeFeatureType {
    #[default]
    RandomTree,
    Krummholz,
    StackedTree,
    OverlayTree,
    TrunklessStackedTree,
}

#[derive(Clone, Debug, Deserialize)]
pub struct KrummholzConfig {
    pub block: String,
    pub min_height: u8,
    pub max_height: u8,
}

#[derive(Clone, Debug, Deserialize)]
struct TemplateVariant {
    id: String,
    footprint: Vec<[i8; 2]>,
    palette_count: u8,
    #[serde(default)]
    size_y: i16,
    #[serde(default)]
    ordered_blocks: Vec<[i16; 4]>,
    /// `[x, z, top_y, kind]`, with kind 0 for leaves and 1 for wood.
    cells: Vec<[i16; 4]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TreeTrunk {
    pub min_height: i16,
    pub max_height: i16,
    pub wide: bool,
    pub block: String,
}

impl TreeTrunk {
    /// Draw and return the trunk height using TFC's `TrunkConfig.getHeight` rule.
    pub(super) fn draw_height(&self, random: &mut FeatureRandom) -> i16 {
        if self.min_height == self.max_height {
            self.min_height
        } else {
            let bound = 1 + self.max_height - self.min_height;
            self.min_height + random.next_int_bound(bound as i32) as i16
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct TreeRoots {
    pub tries: u16,
    pub width: u16,
    pub height: u16,
    pub required: bool,
    pub special_placer: Option<SpecialRootPlacer>,
    pub blocks: Vec<RootBlockMapping>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SpecialRootPlacer {
    pub skew_chance: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RootBlockMapping {
    pub replace: Vec<String>,
    pub with: Vec<String>,
}

static DATABASE: OnceLock<Data> = OnceLock::new();

fn data() -> &'static Data {
    DATABASE.get_or_init(|| {
        serde_json::from_str(DATA).expect("bundled TFC tree template data must be valid JSON")
    })
}

/// Ordered forest-entry IDs from TFC's `forest_trees` tag.
pub fn forest_entry_order() -> &'static [String] {
    &data().feature_order
}

/// Climate and configured tree-variant references for a forest entry.
pub fn forest_entry(id: &str) -> Option<&'static TreeEntry> {
    data().entries.get(id)
}

/// Resolve the actual configured tree feature selected by a forest-entry variant.
pub fn entry_variant(id: &str, variant: TreeVariant) -> Option<&'static str> {
    let entry = forest_entry(id)?;
    match variant {
        TreeVariant::Normal => entry.normal_tree.as_deref(),
        TreeVariant::Dead => entry.dead_tree.as_deref(),
        TreeVariant::OldGrowth => entry
            .old_growth_tree
            .as_deref()
            .or(entry.normal_tree.as_deref()),
        TreeVariant::Krummholz => entry.krummholz.as_deref(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeVariant {
    Normal,
    Dead,
    OldGrowth,
    Krummholz,
}

/// Exact projected tree-block cells selected by a configured TFC tree feature.
///
/// `offsets` are relative to the tree feature's ground origin and can extend into
/// neighboring chunks. `template_id` identifies the actual chosen structure variant.
#[derive(Clone, Debug)]
pub struct SelectedTemplate {
    pub feature_id: String,
    pub template_id: String,
    pub placement_width: u8,
    pub placement_height: u8,
    /// Highest template block in each projected column, relative to the tree's ground origin.
    /// `y` remains template-relative until `finish_trunk` applies a configured trunk height.
    pub cells: Vec<TemplateCell>,
    pub offsets: Vec<[i8; 2]>,
    /// Number of NBT palettes selected after trunk placement, including a single palette.
    pub palette_count: u8,
    pub feature_type: TreeFeatureType,
    layers: Vec<TreeLayer>,
    overlay: Option<TreeOverlay>,
    pub trunk: Option<TreeTrunk>,
    pub root_system: Option<TreeRoots>,
    rotation: u8,
    mirror: u8,
    trunk_height: i16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateBlockKind {
    Leaves,
    Wood,
    Plant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemplateCell {
    pub x: i16,
    pub z: i16,
    pub y: i16,
    pub kind: TemplateBlockKind,
}

impl SelectedTemplate {
    /// Consume the configured trunk-height draw, shift template cells to the trunk top,
    /// and merge generated trunk cells. Call after root placement has consumed its own
    /// random values.
    pub(super) fn finish_trunk(&mut self, random: &mut FeatureRandom) -> i16 {
        let trunk_height = if let Some(trunk) = self.trunk.clone() {
            let height = trunk.draw_height(random);
            for cell in &mut self.cells {
                cell.y += height;
            }
            if height > 0 {
                let positions: &[(i16, i16)] = if trunk.wide {
                    &[(0, 0), (0, 1), (1, 0), (1, 1)]
                } else {
                    &[(0, 0)]
                };
                for &(x, z) in positions {
                    let (x, z) = transform(x as i8, z as i8, self.mirror, self.rotation);
                    let target = (x as i16, z as i16);
                    if let Some(cell) = self
                        .cells
                        .iter_mut()
                        .find(|cell| (cell.x, cell.z) == target)
                    {
                        if cell.y < height - 1 {
                            cell.y = height - 1;
                            cell.kind = TemplateBlockKind::Wood;
                        }
                    } else {
                        self.cells.push(TemplateCell {
                            x: target.0,
                            z: target.1,
                            y: height - 1,
                            kind: TemplateBlockKind::Wood,
                        });
                    }
                    if !self.offsets.contains(&[x, z]) {
                        self.offsets.push([x, z]);
                    }
                }
            }
            height
        } else {
            0
        };
        self.trunk_height = trunk_height;
        trunk_height
    }

    /// Select authored stacked/overlay templates and consume palette draws after trunk placement.
    pub(super) fn finish_palette(&mut self, random: &mut FeatureRandom) {
        match self.feature_type {
            TreeFeatureType::RandomTree => {
                let _ = random.next_int_bound(self.palette_count as i32);
            }
            TreeFeatureType::StackedTree | TreeFeatureType::TrunklessStackedTree => {
                let mut y = self.trunk_height;
                for layer in self.layers.clone() {
                    let count = if layer.min_count == layer.max_count {
                        layer.min_count
                    } else {
                        layer.min_count
                            + random
                                .next_int_bound(i32::from(layer.max_count - layer.min_count) + 1)
                                as u8
                    };
                    for _ in 0..count {
                        if layer.templates.is_empty() {
                            continue;
                        }
                        let index = random.next_int_bound(layer.templates.len() as i32) as usize;
                        let template = &layer.templates[index];
                        self.append_variant(template, y);
                        let _ = random.next_int_bound(template.palette_count as i32);
                        y += template.size_y;
                    }
                }
            }
            TreeFeatureType::OverlayTree => {
                if let Some(overlay) = self.overlay.clone() {
                    self.append_variant(&overlay.base, self.trunk_height);
                    let _ = random.next_int_bound(overlay.base.palette_count as i32);
                    let _ = random.next_int_bound(overlay.overlay.palette_count as i32);
                    let mut top = std::collections::BTreeMap::<(i16, i16), TemplateCell>::new();
                    for &[x, z, y, kind] in &overlay.overlay.ordered_blocks {
                        let (x, z) = transform(x as i8, z as i8, self.mirror, self.rotation);
                        if random.next_float() > overlay.integrity {
                            continue;
                        }
                        let candidate = TemplateCell {
                            x: x as i16,
                            z: z as i16,
                            y: y + self.trunk_height,
                            kind: match kind {
                                0 => TemplateBlockKind::Leaves,
                                1 => TemplateBlockKind::Wood,
                                _ => TemplateBlockKind::Plant,
                            },
                        };
                        let slot = top.entry((candidate.x, candidate.z)).or_insert(candidate);
                        if candidate.y > slot.y {
                            *slot = candidate;
                        }
                    }
                    for cell in top.into_values() {
                        if let Some(base) = self
                            .cells
                            .iter_mut()
                            .find(|base| base.x == cell.x && base.z == cell.z)
                        {
                            if cell.y > base.y {
                                *base = cell;
                            }
                        } else {
                            self.cells.push(cell);
                        }
                        if !self.offsets.contains(&[cell.x as i8, cell.z as i8]) {
                            self.offsets.push([cell.x as i8, cell.z as i8]);
                        }
                    }
                }
            }
            TreeFeatureType::Krummholz => {}
        }
    }

    fn append_variant(&mut self, variant: &TemplateVariant, y_offset: i16) {
        for &[x, z, y, kind] in &variant.cells {
            let (x, z) = transform(x as i8, z as i8, self.mirror, self.rotation);
            self.cells.push(TemplateCell {
                x: x as i16,
                z: z as i16,
                y: y + y_offset,
                kind: if kind == 0 {
                    TemplateBlockKind::Leaves
                } else {
                    TemplateBlockKind::Wood
                },
            });
        }
        for &[x, z] in &variant.footprint {
            let (x, z) = transform(x, z, self.mirror, self.rotation);
            if !self.offsets.contains(&[x, z]) {
                self.offsets.push([x, z]);
            }
        }
    }
}

/// Select a structure variant and apply TFC's random rotation/mirror for this feature RNG.
///
/// The tree feature consumes two bounded random draws for `Rotation` and `Mirror` before
/// selecting a structure. Roots and trunk height use further draws only after selection,
/// so they do not change the chosen template or its horizontal footprint.
pub fn select(feature_id: &str, seed: i64) -> Option<SelectedTemplate> {
    let mut random = FeatureRandom::new(seed);
    select_with_random(feature_id, &mut random)
}

/// Select a structure using the caller's current TFC `WorldgenRandom` stream.
///
/// Use this from a forest-feature simulation, because it has already consumed random
/// values while choosing the entry, position, and normal/dead/old-growth variant.
pub(super) fn select_with_random(
    feature_id: &str,
    random: &mut FeatureRandom,
) -> Option<SelectedTemplate> {
    let feature = data().features.get(feature_id)?;
    if feature.variants.len() > i32::MAX as usize {
        return None;
    }
    if feature.feature_type == TreeFeatureType::Krummholz {
        let shrub = feature.krummholz.as_ref()?;
        let height = if shrub.min_height == shrub.max_height {
            shrub.min_height
        } else {
            shrub.min_height
                + random.next_int_bound(i32::from(shrub.max_height - shrub.min_height) + 1) as u8
        };
        return Some(SelectedTemplate {
            feature_id: feature_id.to_owned(),
            template_id: shrub.block.clone(),
            placement_width: feature.width,
            placement_height: feature.height,
            cells: vec![TemplateCell {
                x: 0,
                z: 0,
                y: i16::from(height.saturating_sub(1)),
                kind: TemplateBlockKind::Plant,
            }],
            offsets: vec![[0, 0]],
            palette_count: 0,
            feature_type: feature.feature_type,
            layers: Vec::new(),
            overlay: None,
            trunk: None,
            root_system: None,
            rotation: 0,
            mirror: 0,
            trunk_height: 0,
        });
    }
    // TreeHelpers.getPlacementSettings draws rotation and mirror before feature-specific
    // location, roots, trunk, or structure selections.
    let rotation = random.next_int_bound(4) as u8;
    let mirror = random.next_int_bound(3) as u8;
    if matches!(
        feature.feature_type,
        TreeFeatureType::StackedTree
            | TreeFeatureType::TrunklessStackedTree
            | TreeFeatureType::OverlayTree
    ) {
        if feature.feature_type == TreeFeatureType::StackedTree
            || feature.feature_type == TreeFeatureType::TrunklessStackedTree
        {
            if feature.layers.is_empty() {
                return None;
            }
        } else if feature.overlay.is_none() {
            return None;
        }
        return Some(SelectedTemplate {
            feature_id: feature_id.to_owned(),
            template_id: feature_id.to_owned(),
            placement_width: feature.width,
            placement_height: feature.height,
            cells: Vec::new(),
            offsets: Vec::new(),
            palette_count: 0,
            feature_type: feature.feature_type,
            layers: feature.layers.clone(),
            overlay: feature.overlay.clone(),
            trunk: if feature.feature_type == TreeFeatureType::TrunklessStackedTree {
                None
            } else {
                feature.trunk.clone()
            },
            root_system: feature.root_system.clone(),
            rotation,
            mirror,
            trunk_height: 0,
        });
    }
    if feature.variants.is_empty() {
        return None;
    }
    // TFC's RandomTreeFeature receives WorldgenRandom, whose inherited convenience
    // methods use Java's `next(31)` rejection algorithm for non-power-of-two bounds.
    let variant_index = random.next_int_bound(feature.variants.len() as i32) as usize;
    let variant = &feature.variants[variant_index];

    // Keep projection bounded in case a future pack introduces unusually large trees.
    if variant.footprint.len() > 16_384 {
        return None;
    }
    let mut offsets = Vec::with_capacity(variant.footprint.len());
    for &[x, z] in &variant.footprint {
        let (x, z) = transform(x, z, mirror, rotation);
        offsets.push([x, z]);
    }
    if variant.cells.len() > 16_384 || variant.palette_count == 0 {
        return None;
    }
    let mut cells = Vec::with_capacity(variant.cells.len());
    for &[x, z, y, kind] in &variant.cells {
        let (x, z) = transform(x as i8, z as i8, mirror, rotation);
        cells.push(TemplateCell {
            x: x as i16,
            z: z as i16,
            y,
            kind: if kind == 0 {
                TemplateBlockKind::Leaves
            } else {
                TemplateBlockKind::Wood
            },
        });
    }
    Some(SelectedTemplate {
        feature_id: feature_id.to_owned(),
        template_id: variant.id.clone(),
        placement_width: feature.width,
        placement_height: feature.height,
        cells,
        offsets,
        palette_count: variant.palette_count,
        feature_type: feature.feature_type,
        layers: Vec::new(),
        overlay: None,
        trunk: feature.trunk.clone(),
        root_system: feature.root_system.clone(),
        rotation,
        mirror,
        trunk_height: 0,
    })
}

/// Project an installed structure template with a rotation and mirror already drawn
/// from the caller's TFC feature RNG stream.
///
/// This is useful for dynamic configured features: `RandomTreeFeature` draws its
/// placement rotation and mirror before it selects the structure ID, so callers that
/// already consumed those draws should pass them here without advancing the RNG again.
pub(super) fn from_structure(
    structure_id: &str,
    rotation: u8,
    mirror: u8,
) -> Option<SelectedTemplate> {
    let variant = data().structures.get(structure_id)?;
    selected_structure(structure_id, variant, rotation, mirror, 1, 1)
}

fn selected_structure(
    structure_id: &str,
    variant: &TemplateVariant,
    rotation: u8,
    mirror: u8,
    placement_width: u8,
    placement_height: u8,
) -> Option<SelectedTemplate> {
    if variant.cells.is_empty()
        || variant.cells.len() > 16_384
        || variant.footprint.len() > 16_384
        || variant.palette_count == 0
    {
        return None;
    }
    let cells = variant
        .cells
        .iter()
        .map(|&[x, z, y, kind]| {
            let (x, z) = transform(x as i8, z as i8, mirror, rotation);
            TemplateCell {
                x: x as i16,
                z: z as i16,
                y,
                kind: if kind == 0 {
                    TemplateBlockKind::Leaves
                } else {
                    TemplateBlockKind::Wood
                },
            }
        })
        .collect();
    let offsets = variant
        .footprint
        .iter()
        .map(|&[x, z]| {
            let (x, z) = transform(x, z, mirror, rotation);
            [x, z]
        })
        .collect();
    Some(SelectedTemplate {
        feature_id: structure_id.to_owned(),
        template_id: variant.id.clone(),
        placement_width,
        placement_height,
        cells,
        offsets,
        palette_count: variant.palette_count,
        feature_type: TreeFeatureType::RandomTree,
        layers: Vec::new(),
        overlay: None,
        trunk: None,
        root_system: None,
        rotation,
        mirror,
        trunk_height: 0,
    })
}

fn transform(mut x: i8, mut z: i8, mirror: u8, rotation: u8) -> (i8, i8) {
    match mirror {
        1 => z = -z, // Mirror.LEFT_RIGHT
        2 => x = -x, // Mirror.FRONT_BACK
        _ => {}
    }
    match rotation {
        1 => (-z, x), // Rotation.CLOCKWISE_90
        2 => (-x, -z),
        3 => (z, -x), // Rotation.COUNTERCLOCKWISE_90
        _ => (x, z),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(feature_id: &str, template_id: &str) -> &'static TemplateVariant {
        data().features[feature_id]
            .variants
            .iter()
            .find(|variant| variant.id == template_id)
            .expect("golden structure variant exists")
    }

    #[test]
    fn template_top_cells_match_independently_read_nbt_points() {
        // These are direct block-position checks against TFC's oak/1.nbt and
        // oak_dead/1.nbt files, including TreeHelpers.transformCenter's offsets.
        let oak = variant("tfc:tree/oak", "tfc:oak/1");
        assert_eq!(oak.palette_count, 1);
        assert!(oak.cells.contains(&[0, 0, 14, 0]));
        assert!(oak.cells.contains(&[-2, -1, 13, 0]));

        let dead_oak = variant("tfc:tree/oak_dead", "tfc:oak_dead/1");
        assert!(dead_oak.cells.contains(&[0, 0, 4, 1]));
        assert!(dead_oak.cells.contains(&[1, -1, 7, 1]));
    }

    #[test]
    fn installed_tree_templates_have_single_palette_draws() {
        assert!(
            data()
                .features
                .values()
                .flat_map(|feature| &feature.variants)
                .all(|variant| variant.palette_count == 1)
        );
    }

    #[test]
    fn tree_transform_matches_tfc_helper_axes() {
        assert_eq!(transform(2, 3, 0, 0), (2, 3));
        assert_eq!(transform(2, 3, 1, 0), (2, -3));
        assert_eq!(transform(2, 3, 2, 0), (-2, 3));
        assert_eq!(transform(2, 3, 0, 1), (-3, 2));
        assert_eq!(transform(2, 3, 0, 3), (3, -2));
    }

    #[test]
    fn prism_kubejs_palm_templates_match_independently_read_nbt_points() {
        let coconut = data()
            .structures
            .get("tfg:palm_tree/coconut_0")
            .expect("installed KubeJS coconut structure exists");
        assert!(coconut.cells.contains(&[0, 0, 10, 0]));
        assert!(coconut.cells.contains(&[-3, 0, 12, 0]));
        assert_eq!(coconut.palette_count, 1);

        let projected = from_structure("tfg:palm_tree/coconut_0", 1, 0)
            .expect("installed KubeJS coconut structure projects");
        assert!(projected.cells.contains(&TemplateCell {
            x: 0,
            z: -3,
            y: 12,
            kind: TemplateBlockKind::Leaves,
        }));
    }

    #[test]
    fn forest_krummholz_configs_resolve_as_height_sampled_shrubs() {
        let id = "tfc:tree/aspen_krummholz";
        let mut random = FeatureRandom::new(17);
        let selected = select_with_random(id, &mut random).expect("aspen krummholz resolves");
        assert_eq!(selected.feature_type, TreeFeatureType::Krummholz);
        assert_eq!(selected.template_id, "tfc:plant/aspen_krummholz");
        assert_eq!(selected.cells.len(), 1);
        assert_eq!(selected.cells[0].kind, TemplateBlockKind::Plant);
        assert!((0..=3).contains(&selected.cells[0].y));
        assert!(selected.offsets.contains(&[0, 0]));
    }

    #[test]
    fn all_installed_forest_entry_variants_resolve() {
        for entry_id in forest_entry_order() {
            let entry = forest_entry(entry_id).expect("ordered forest entry metadata exists");
            for feature_id in [
                entry.normal_tree.as_deref(),
                entry.dead_tree.as_deref(),
                entry.old_growth_tree.as_deref(),
                entry.krummholz.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                assert!(
                    data().features.contains_key(feature_id),
                    "unresolved configured tree feature {feature_id} from {entry_id}"
                );
            }
        }
        assert_eq!(forest_entry_order().len(), 61);
    }

    #[test]
    fn forest_order_honors_installed_afc_replace_tag() {
        // AFC's installed tfc:forest_trees tag replaces TFC's 19-entry list.
        // These leading IDs distinguish the active order from concatenating
        // the old TFC list before AFC additions.
        assert_eq!(
            &forest_entry_order()[..8],
            [
                "tfc:tree/acacia_entry",
                "tfc:tree/gum_arabic_entry",
                "tfc:tree/acacia_koa_entry",
                "tfc:tree/ash_entry",
                "tfc:tree/aspen_entry",
                "tfc:tree/birch_entry",
                "tfc:tree/blackwood_entry",
                "tfc:tree/mpingo_blackwood_entry",
            ]
        );
        let position = |id: &str| {
            forest_entry_order()
                .iter()
                .position(|entry| entry == id)
                .expect("installed forest entry is ordered")
        };
        assert!(position("tfc:tree/live_oak_entry") < position("tfc:tree/oak_entry"));
    }

    #[test]
    fn layered_tree_configs_produce_authored_surface_cells() {
        for (feature_id, feature_type) in [
            ("tfc:tree/douglas_fir_large", TreeFeatureType::StackedTree),
            (
                "tfc:tree/sequoia_large",
                TreeFeatureType::TrunklessStackedTree,
            ),
            ("tfc:tree/white_cedar", TreeFeatureType::OverlayTree),
        ] {
            let mut random = FeatureRandom::new(0x715);
            let mut selected = select_with_random(feature_id, &mut random)
                .expect("layered configured tree is present");
            assert_eq!(selected.feature_type, feature_type);
            selected.finish_trunk(&mut random);
            selected.finish_palette(&mut random);
            assert!(!selected.cells.is_empty(), "{feature_id} has tree cells");
            assert!(!selected.offsets.is_empty(), "{feature_id} has a footprint");
        }
    }

    #[test]
    fn absent_root_configs_stay_absent_and_prism_sequoia_override_wins() {
        assert!(data().features["tfc:tree/oak"].root_system.is_none());
        assert!(
            data().features["tfc:tree/white_cedar"]
                .root_system
                .is_none()
        );
        assert_eq!(
            data().features["tfc:tree/sequoia_large"].feature_type,
            TreeFeatureType::TrunklessStackedTree
        );
        assert!(
            data().features["tfc:tree/sequoia_large"]
                .root_system
                .is_none()
        );
    }

    #[test]
    fn white_cedar_overlay_consumes_palette_and_processor_random_draws() {
        for (feature_id, expected_blocks) in [
            ("tfc:tree/white_cedar", 25),
            ("tfc:tree/white_cedar_large", 134),
        ] {
            let seed = 0x51ced;
            let mut actual = FeatureRandom::new(seed);
            let mut tree = select_with_random(feature_id, &mut actual).unwrap();
            tree.finish_trunk(&mut actual);
            tree.finish_palette(&mut actual);

            let feature = &data().features[feature_id];
            let overlay = feature.overlay.as_ref().unwrap();
            assert_eq!(overlay.integrity, 0.5);
            assert_eq!(overlay.overlay.ordered_blocks.len(), expected_blocks);

            let mut expected = FeatureRandom::new(seed);
            let _rotation = expected.next_int_bound(4);
            let _mirror = expected.next_int_bound(3);
            let trunk = feature.trunk.as_ref().unwrap();
            let _trunk_height = trunk.draw_height(&mut expected);
            let _base_palette = expected.next_int_bound(overlay.base.palette_count as i32);
            let _overlay_palette = expected.next_int_bound(overlay.overlay.palette_count as i32);
            for _ in 0..expected_blocks {
                let _processor_random = expected.next_float();
            }
            assert_eq!(actual.next_long(), expected.next_long(), "{feature_id}");
        }
    }
}
