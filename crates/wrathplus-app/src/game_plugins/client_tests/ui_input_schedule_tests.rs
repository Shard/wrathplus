//! The UI input feed's schedule: one registration, live in the world and on the stock glue
//! screens. Under the in-world gate the stock login took no pointer or key at all.

use bevy::prelude::*;

use crate::game_plugins::schedule_tests::{census, headless_client, SyncPoints};

/// `feed_ui_input`, the only production feed of pointer and keys into the VM, is registered once,
/// gated on `ui_input_live` (InWorld, or Login/CharSelect/CharCreate under the stock glue), and
/// under no in-world-only set.
#[test]
fn the_ui_input_feed_runs_on_the_stock_glue_screens_and_in_the_world() {
    let mut app = headless_client();
    let update = census(&mut app, Update, SyncPoints::Declared);
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
