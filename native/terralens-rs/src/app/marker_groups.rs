//! World-anchored clustering: panning never changes a marker's group or centroid.
use crate::model::{Bounds, Marker, MarkerKind, Point};
use std::collections::BTreeMap;

pub(super) fn cell_span(bpp: f64) -> f64 {
    40. * 2_f64.powi(bpp.clamp(0.125, 4096.).log2().round() as i32)
}

pub(super) fn query_bounds(bounds: Bounds, bpp: f64) -> Bounds {
    let padding = cell_span(bpp);
    Bounds {
        min_x: bounds.min_x - padding,
        min_z: bounds.min_z - padding,
        max_x: bounds.max_x + padding,
        max_z: bounds.max_z + padding,
    }
}

pub(super) fn groups(markers: &[&Marker], bpp: f64, selected: Option<&Marker>) -> Vec<Vec<usize>> {
    let span = cell_span(bpp);
    let mut groups = BTreeMap::<(i64, i64), Vec<usize>>::new();
    let mut pinned = Vec::new();
    for (index, marker) in markers.iter().enumerate() {
        if matches!(marker.kind, MarkerKind::Spawn | MarkerKind::Player)
            || selected.is_some_and(|pin| pin.id == marker.id && pin.point() == marker.point())
        {
            pinned.push(index);
        } else {
            groups
                .entry((
                    (marker.x / span).floor() as i64,
                    (marker.z / span).floor() as i64,
                ))
                .or_default()
                .push(index);
        }
    }
    let mut result: Vec<_> = groups.into_values().collect();
    result.extend(pinned.into_iter().map(|index| vec![index]));
    result
}

pub(super) fn center(markers: &[&Marker], members: &[usize]) -> Point {
    let first = markers[members[0]].point();
    let (x, z) = members.iter().fold((0., 0.), |(x, z), index| {
        let point = markers[*index].point();
        (x + point.x - first.x, z + point.z - first.z)
    });
    Point {
        x: first.x + x / members.len() as f64,
        z: first.z + z / members.len() as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MarkerKind;
    fn marker(x: f64, z: f64) -> Marker {
        Marker {
            id: format!("test:{x}:{z}"),
            label: String::new(),
            namespace: "test".into(),
            x,
            z,
            y: None,
            kind: MarkerKind::Ore,
            confirmed: false,
            min_y: None,
            max_y: None,
            detail: String::new(),
            color: [1, 2, 3],
            bounds: None,
        }
    }
    #[test]
    fn spawn_and_players_stay_at_their_positions_outside_feature_clusters() {
        let mut values = [
            marker(36., 12.),
            marker(42., 16.),
            marker(40., 14.),
            marker(41., 15.),
        ];
        values[2].kind = MarkerKind::Spawn;
        values[3].kind = MarkerKind::Player;
        let markers: Vec<_> = values.iter().collect();
        for bpp in [0.125, 2., 32., 4096.] {
            let groups = groups(&markers, bpp, None);
            for index in [2, 3] {
                let group = groups.iter().find(|group| group.contains(&index)).unwrap();
                assert_eq!(group, &[index]);
                assert_eq!(center(&markers, group), markers[index].point());
            }
        }
    }

    #[test]
    fn neighboring_pair_keeps_membership_and_world_center_during_a_pan() {
        // This pair straddled a screen-cell boundary after a tiny pan in the old scheme.
        let values = [marker(36., 12.), marker(42., 16.)];
        let markers: Vec<_> = values.iter().collect();
        let before = groups(&markers, 2., None);
        assert_eq!(before, vec![vec![0, 1]]);
        let anchor = center(&markers, &before[0]);
        assert_eq!(anchor, Point { x: 39., z: 14. });
        let old_screen_cells = |pan: f64| {
            values
                .iter()
                .map(|m| ((m.x - pan) / 2. / 40.).floor() as i64)
                .collect::<Vec<_>>()
        };
        assert_eq!(old_screen_cells(0.), vec![0, 0]);
        assert_eq!(
            old_screen_cells(-40.),
            vec![0, 1],
            "the previous screen grid split this same pair after a 20px pan"
        );
        for pan in [-321., -40., -0.1, 0., 0.1, 40., 321.] {
            let screen_anchor = (anchor.x - pan) / 2.;
            let after = groups(&markers, 2., None);
            assert_eq!(after, before);
            assert_eq!((center(&markers, &after[0]).x - pan) / 2., screen_anchor);
        }
    }
    #[test]
    fn padded_queries_keep_the_full_cluster_when_a_neighbor_is_outside_the_view() {
        let values = [marker(-4., 10.), marker(8., 12.)];
        for left in [-5., 0., 5.] {
            let bounds = query_bounds(
                Bounds {
                    min_x: left,
                    min_z: 0.,
                    max_x: left + 100.,
                    max_z: 100.,
                },
                2.,
            );
            assert!(values.iter().all(|m| bounds.contains(m.x, m.z)));
        }
        let refs: Vec<_> = values.iter().collect();
        // Negative world coordinates use floor; pinning separates only the selected item.
        assert_eq!(groups(&refs, 2., None).len(), 2);
        let values = [marker(4., 10.), marker(8., 12.)];
        let refs: Vec<_> = values.iter().collect();
        assert_eq!(groups(&refs, 2., None).len(), 1);
        assert_eq!(groups(&refs, 2., Some(&values[0])).len(), 2);
    }
}
