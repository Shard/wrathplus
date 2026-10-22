//! Transport timetable tests against the nine live transports' server periods; they skip without
//! client data.

use super::*;
use crate::taxi::load_taxi_path_nodes;

struct Golden {
    path_id: u32,
    move_speed: f32,
    accel_rate: f32,
}

/// The nine live transports' periods from vmangos's `transports` table (the build ≤ 5875 row) and
/// their `gameobject_template` `data1`/`data2`.
const GOLDENS: &[Golden] = &[
    Golden {
        path_id: 241,
        move_speed: 30.0,
        accel_rate: 1.0,
    },
    Golden {
        path_id: 302,
        move_speed: 30.0,
        accel_rate: 1.0,
    },
    Golden {
        path_id: 285,
        move_speed: 30.0,
        accel_rate: 1.0,
    },
    Golden {
        path_id: 292,
        move_speed: 30.0,
        accel_rate: 1.0,
    },
    Golden {
        path_id: 293,
        move_speed: 30.0,
        accel_rate: 1.0,
    },
    Golden {
        path_id: 295,
        move_speed: 30.0,
        accel_rate: 1.0,
    },
    Golden {
        path_id: 301,
        move_speed: 30.0,
        accel_rate: 1.0,
    },
    Golden {
        path_id: 303,
        move_speed: 30.0,
        accel_rate: 1.0,
    },
    Golden {
        path_id: 436,
        move_speed: 1.0,
        accel_rate: 1.0,
    },
];

/// Also proves that at least one live transport crosses continents.
#[test]
fn touches_map_matches_the_paths_map_set() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_taxi_path_nodes(&mut chain).expect("load TaxiPathNode");

    let mut any_cross_map = false;
    for g in GOLDENS {
        let nodes = cat
            .path(g.path_id)
            .unwrap_or_else(|| panic!("path {} exists in TaxiPathNode.dbc", g.path_id));
        let tt = TransportTimetable::build(nodes, g.move_speed, g.accel_rate)
            .unwrap_or_else(|| panic!("path {} builds", g.path_id));
        let maps: std::collections::HashSet<u32> = nodes.iter().map(|n| n.map_id).collect();
        any_cross_map |= maps.len() > 1;
        for &m in &maps {
            assert!(tt.touches_map(m), "path {} should touch map {m}", g.path_id);
        }
        assert!(
            !tt.touches_map(9999),
            "path {} touches a map no node lives on",
            g.path_id
        );
    }
    assert!(
        any_cross_map,
        "no golden path crosses continents — the worldport spare predicate has nothing to spare"
    );
}

/// From any instant, the answer samples on the asked-for map, on every cross-continent path:
/// path 241 reaches map 1 only across the cycle wrap.
#[test]
fn first_cycle_on_map_lands_on_that_map() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_taxi_path_nodes(&mut chain).expect("load TaxiPathNode");

    let mut crossed = 0usize;
    for g in GOLDENS {
        let nodes = cat
            .path(g.path_id)
            .unwrap_or_else(|| panic!("path {} exists in TaxiPathNode.dbc", g.path_id));
        let tt = TransportTimetable::build(nodes, g.move_speed, g.accel_rate)
            .unwrap_or_else(|| panic!("path {} builds", g.path_id));
        let maps: std::collections::HashSet<u32> = nodes.iter().map(|n| n.map_id).collect();
        if maps.len() < 2 {
            continue;
        }
        crossed += 1;
        // 64 instants across the cycle, both sides of the wrap included.
        for k in 0..64u32 {
            let from = (u64::from(tt.period_ms) * u64::from(k) / 64) as u32;
            for &m in &maps {
                let at = tt.first_cycle_on_map(from, m).unwrap_or_else(|| {
                    panic!(
                        "path {} touches map {m} but found no cycle on it",
                        g.path_id
                    )
                });
                assert!(
                    at < tt.period_ms,
                    "path {} map {m}: {at} ms is outside the {} ms cycle",
                    g.path_id,
                    tt.period_ms
                );
                assert_eq!(
                    tt.sample(at).map,
                    m,
                    "path {} from {from} ms: first_cycle_on_map({m}) = {at} ms samples map {}",
                    g.path_id,
                    tt.sample(at).map
                );
            }
        }
        // A map the path never visits.
        assert_eq!(tt.first_cycle_on_map(0, 9999), None);
    }
    assert!(
        crossed > 0,
        "no golden path crosses continents — this test proves nothing"
    );
}
