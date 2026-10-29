//! Whole-client tests of `crate::ui_unit`, from `ui_unit.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use bevy::prelude::*;

/// The reach feed measures the guids the aura feed pushes, so a frame's feeds must run in that
/// order: read off the declared graph, the aura feed's set comes before the reach feed.
#[test]
fn the_reach_feed_runs_after_the_aura_feed_it_reads() {
    use crate::game_plugins::schedule_tests::{census, headless_client, SyncPoints};

    let mut app = headless_client();
    let c = census(&mut app, Update, SyncPoints::Declared);
    if !c
        .systems
        .values()
        .any(|s| s.name.contains("wrathplus_app::"))
    {
        eprintln!("skipped: this build carries no type names");
        return;
    }
    let one = |suffix: &str| {
        let mut hits = c.systems.iter().filter(|(_, s)| s.name.ends_with(suffix));
        match (hits.next(), hits.next()) {
            (Some((k, _)), None) => *k,
            _ => panic!("exactly one system named `…{suffix}` in Update"),
        }
    };
    let aura = one("::ui_aura::feed_auras");
    let reach = one("::ui_unit::feed_unit_reach");
    assert!(
        c.dependencies.contains(&(aura, reach)),
        "feed_unit_reach is declared after feed_auras"
    );
}
