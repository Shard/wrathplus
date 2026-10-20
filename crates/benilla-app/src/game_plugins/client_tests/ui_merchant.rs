//! Whole-client tests of `crate::ui_merchant`, from `ui_merchant/mod.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::ui_merchant::*;

/// A repair's `UNIT_INVENTORY_CHANGED` reads this frame's total: the stock handler refreshes
/// the Repair All button from `GetRepairAllCost` (`PaperDollFrame.lua:717-724`).
#[test]
fn the_repair_all_total_is_swept_before_the_inventory_event() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    assert!(crate::test_support::runs_before(
        &mut app,
        feed_repair_costs,
        crate::ui_char::feed_char
    ));
}
