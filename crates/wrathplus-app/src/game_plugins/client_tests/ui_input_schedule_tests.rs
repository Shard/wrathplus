//! The UI input feed's schedule: one registration, live in the world and on the stock glue
//! screens. Under the in-world gate the stock login took no pointer or key at all.

use bevy::prelude::*;

use crate::game_plugins::schedule_tests::{census, headless_client, SyncPoints};

/// Template getters must be populated before synchronous unit-event handlers can read them;
/// loot-list publication must consume this frame's packets, never the previous frame's.
#[test]
fn tooltip_publication_has_declared_packet_and_template_prerequisites() {
    let mut app = headless_client();
    let c = census(&mut app, Update, SyncPoints::Declared);
    if !crate::game_plugins::schedule_tests::type_names_available(&c) {
        eprintln!(
            "skipped: ordering-name contract requires bevy/debug; numeric ratchet still runs"
        );
        return;
    }
    let one = |suffix: &str| {
        let hits: Vec<_> = c
            .systems
            .iter()
            .filter(|(_, s)| s.name.ends_with(suffix))
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(hits.len(), 1, "one system matching {suffix}");
        hits[0]
    };
    let precedes = |a, b| {
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![a];
        while let Some(n) = stack.pop() {
            for (_, next) in c.dependencies.iter().filter(|(from, _)| *from == n) {
                if seen.insert(*next) {
                    stack.push(*next);
                }
            }
        }
        seen.contains(&b)
    };
    let items = one("::ui_items::feed::feed_item_stats");
    for consumer in [
        "::ui_quest_log::feed_quest_log",
        "::ui_tooltip::drive_mouseover_tooltip",
        "::ui_unit_quest_tooltip::publish",
    ] {
        assert!(
            precedes(items, one(consumer)),
            "item templates before {consumer}"
        );
    }
    assert!(precedes(
        one("::net::apply::apply_net_updates"),
        one("::ui_tooltip::feed_unit_looters")
    ));
}

/// `feed_ui_input`, the only production feed of pointer and keys into the VM, is registered once,
/// gated on `ui_input_live` (InWorld, or Login/CharSelect/CharCreate under the stock glue), and
/// under no in-world-only set.
#[test]
fn the_ui_input_feed_runs_on_the_stock_glue_screens_and_in_the_world() {
    let mut app = headless_client();
    let update = census(&mut app, Update, SyncPoints::Declared);
    if !crate::game_plugins::schedule_tests::type_names_available(&update) {
        eprintln!("skipped: this build carries no Bevy system names");
        return;
    }
    let feeds: Vec<_> = update
        .systems
        .values()
        .filter(|s| s.name.ends_with("::feed_ui_input"))
        .collect();
    assert_eq!(feeds.len(), 1, "one registration: {feeds:#?}");
    let feed = feeds[0];
    assert!(
        feed.conditions
            .iter()
            .any(|c| c.ends_with("::ui_input_live")),
        "gated on ui_input_live: {feed:#?}"
    );
    assert!(
        !feed.sets.iter().any(|s| s.contains("InWorldGated")),
        "never under the in-world-only gate: {feed:#?}"
    );
}
