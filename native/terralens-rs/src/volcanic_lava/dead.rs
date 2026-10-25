//! RNG advancement for the dead-cinder-cone multiple feature.
//!
//! In the installed TFG data, two deterministic disk features run before the six
//! obsidian rivulets. The center disk's placement offsets and disk configuration use
//! constant integer providers, so they consume no random values. The edge disk has a
//! fixed count of forty and a uniform X/Z offset in [-14, 14]; its fixed Y offset,
//! heightmap placement, disk radius/half-height and simple block-state providers are
//! deterministic. Each edge therefore consumes exactly two `nextInt(29)` draws.

use crate::vegetation::FeatureRandom;

/// Advances the shared worldgen feature random stream past the dead-cone dirt disks,
/// leaving it aligned for `obsidian_rivulet`'s six lazy placements.
pub(super) fn advance_dirt(random: &mut FeatureRandom) {
    for _ in 0..40 {
        let _x = random.next_int_bound(29);
        let _z = random.next_int_bound(29);
    }
}
