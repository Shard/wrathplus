//! Whole-client tests of `crate::ui_tooltip`, from `ui_tooltip/tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.
//!
//! The spell-view cell tests, against the real 5875 data, and the feed's place in the schedule.

use crate::ui_tooltip::spell_feed::feed_spell_tooltips;

/// A trainer list that lands this frame is hoverable in this frame's tick: the trainer feed derives
/// the subjects the spell feed pushes.
#[test]
fn the_spell_feed_runs_after_the_trainer_feed() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    assert!(crate::test_support::runs_before(
        &mut app,
        crate::ui_trainer::feed_trainer,
        feed_spell_tooltips
    ));
}

/// A quest panel that opens this frame is hoverable in its tick.
#[test]
fn the_spell_feed_runs_after_the_quest_feed() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    assert!(crate::test_support::runs_before(
        &mut app,
        crate::ui_quest::feed_quest,
        feed_spell_tooltips
    ));
}
