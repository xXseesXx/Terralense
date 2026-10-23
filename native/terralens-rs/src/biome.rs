//! TFG biome IDs, predicates, selection and the TFC layered area machinery.
pub mod area;
pub mod choose;
pub mod forest;
pub mod ids;
pub mod layers;

pub fn biome_name(id: u16) -> &'static str {
    if id >= 2000 {
        return crate::planetary::biome_name(id);
    }
    if id >= 1000 {
        return crate::beneath::biome_name(id);
    }
    ids::name(id)
}
/// TFG's ocean predicate used by biome coast layers.
pub fn is_ocean(id: u16) -> bool {
    ids::is_ocean(id) || id == ids::id::ICE_SHEET_OCEANIC
}
