//! TFC overworld forest layer at chunk resolution.
use super::area::{self, Area, AreaContext};
use crate::{noise::OpenSimplex2D, random::JavaRandom};
use std::rc::Rc;
pub const FOREST_NONE: i32 = 0;
pub const FOREST_SPARSE: i32 = 1;
pub const FOREST_EDGE: i32 = 2;
pub const FOREST_NORMAL: i32 = 3;
pub const FOREST_OLD: i32 = 4;
fn randomize(v: i32, r: &AreaContext) -> i32 {
    if v == FOREST_NONE {
        let n = r.bounded(16);
        if n <= 2 {
            FOREST_SPARSE
        } else if n == 3 {
            FOREST_NORMAL
        } else {
            v
        }
    } else if v == FOREST_SPARSE {
        if r.bounded(7) <= 3 { FOREST_NORMAL } else { v }
    } else if v == FOREST_NORMAL || v == FOREST_OLD {
        let n = r.bounded(24);
        if n == 1 && v != FOREST_OLD {
            FOREST_SPARSE
        } else if n == 2 {
            FOREST_NONE
        } else if n <= 6 {
            FOREST_OLD
        } else {
            v
        }
    } else {
        v
    }
}
fn full(v: i32) -> bool {
    v == FOREST_NORMAL || v == FOREST_OLD
}
fn edge(n: i32, e: i32, s: i32, w: i32, c: i32) -> i32 {
    if full(c) && (!full(n) || !full(e) || !full(s) || !full(w)) {
        FOREST_EDGE
    } else {
        c
    }
}
fn randomize_small(v: i32, r: &AreaContext) -> i32 {
    if v == FOREST_NORMAL || v == FOREST_OLD {
        let n = r.bounded(if v == FOREST_OLD { 40 } else { 25 });
        if n == 0 {
            FOREST_NONE
        } else if n == 1 {
            FOREST_SPARSE
        } else {
            v
        }
    } else if v == FOREST_SPARSE || v == FOREST_NONE {
        if r.bounded(30) == 0 {
            if v == FOREST_SPARSE {
                FOREST_NORMAL
            } else {
                FOREST_EDGE
            }
        } else {
            v
        }
    } else {
        v
    }
}
pub fn overworld_forest_layer(seed: i64) -> Area {
    let mut random = JavaRandom::new(seed);
    let mut n = OpenSimplex2D::from_int(random.next_int());
    n.spread(0.3f32 as f64);
    let noise = Rc::new(n);
    let init = area::source_area(random.next_long(), move |x, z, _| {
        if (noise.sample(x as f64, z as f64) as f32) < 0.0 {
            FOREST_NONE
        } else {
            FOREST_NORMAL
        }
    });
    let mut layer = area::center_transform(random.next_long(), init, randomize);
    layer = area::fuzzy_zoom_area(random.next_long(), layer);
    layer = area::center_transform(random.next_long(), layer, randomize);
    layer = area::fuzzy_zoom_area(random.next_long(), layer);
    layer = area::zoom_area(random.next_long(), layer);
    // AdjacentTransformLayer consumes a layer seed even though the transform draws no random bits.
    let _edge_seed = random.next_long();
    layer = area::adjacent_transform(layer, edge);
    layer = area::center_transform(random.next_long(), layer, randomize_small);
    for _ in 0..2 {
        layer = area::zoom_area(random.next_long(), layer);
    }
    layer
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forest_layer_matches_typescript_capture() {
        let layer = overworld_forest_layer(9_988_776_655);
        let points = [(-9, -8), (-1, 0), (0, 0), (3, 7), (22, -18), (100, 63)];
        assert_eq!(points.map(|(x, z)| layer(x, z)), [2, 2, 2, 3, 0, 4]);
    }
}
