//! Whole-client tests of `crate::ui_unit::held`, from `ui_unit/held.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use bevy::prelude::*;

/// The chain feed reads the resolver inputs the aura feed pushes in the same frame, so the
/// declared order puts the feed after it: read off the schedule graph.
#[test]
fn the_chain_feed_runs_after_the_aura_feed_it_reads() {
    use crate::game_plugins::schedule_tests::{census, headless_client, SyncPoints};

    let mut app = headless_client();
    let c = census(&mut app, Update, SyncPoints::Declared);
    if !c.systems.values().any(|s| s.name.contains("benilla_app::")) {
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
    for feed in [
        "::ui_unit::held::feed_chain_units",
        "::spell_targeting::feed_targeting_to_vm",
    ] {
        assert!(
            c.dependencies.contains(&(aura, one(feed))),
            "{feed} is declared after feed_auras"
        );
    }
}
