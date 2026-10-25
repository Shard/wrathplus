//! Repair mode against the bag clicks, and `ContainerIDToInventoryID`.

use super::*;

#[test]
fn repair_cursor_wins_over_a_sellable_bag_hover() {
    let mut s = UiScript::new().unwrap();
    s.set_container(0, Some(backpack()));
    s.set_merchant(Some(MerchantState {
        can_repair: true,
        ..Default::default()
    }));

    s.run("ShowContainerSellCursor(0, 1)").unwrap();
    assert_eq!(s.ui_cursor(), Some(UiCursorMode::Buy));

    s.run("ShowRepairCursor()").unwrap();
    assert_eq!(
        s.ui_cursor(),
        None,
        "repair replaces the existing Buy hover"
    );
    s.run("ShowContainerSellCursor(0, 1)").unwrap();
    assert_eq!(
        s.ui_cursor(),
        None,
        "a bag hover keeps the repair base cursor"
    );
    s.run("PickupContainerItem(0, 1)").unwrap();
    s.run("PickupContainerItem(0, 1)").unwrap();
    assert_eq!(
        s.take_container_repairs(),
        vec![(0, 1), (0, 1)],
        "repair mode sticks across clicks"
    );
}

#[test]
fn right_click_repairs_bag_item_instead_of_selling_it() {
    let mut s = UiScript::new().unwrap();
    s.set_container(0, Some(backpack()));
    s.set_merchant(Some(MerchantState {
        can_repair: true,
        ..Default::default()
    }));
    s.run("ShowRepairCursor() UseContainerItem(0, 1) UseContainerItem(0, 1)")
        .unwrap();

    assert_eq!(s.take_container_repairs(), vec![(0, 1), (0, 1)]);
    assert!(s.take_container_uses().is_empty(), "no sell/use intent");
}

#[test]
fn right_click_in_repair_mode_puts_a_held_item_back_then_repairs() {
    let mut s = UiScript::new().unwrap();
    s.set_container(0, Some(backpack()));
    s.set_merchant(Some(MerchantState {
        can_repair: true,
        ..Default::default()
    }));
    s.run("PickupContainerItem(0, 1)").unwrap();
    assert!(s.cursor_item().is_some());
    // A held item and repair mode together, as after a cursor-mode change.
    s.model_mut().repair_mode = true;

    s.run("UseContainerItem(0, 4)").unwrap();
    assert!(
        s.cursor_payload().is_none(),
        "the held item went back (`0x4fa198`)"
    );
    assert!(s.take_container_moves().is_empty(), "never placed");
    assert_eq!(s.take_container_repairs(), vec![(0, 4)]);

    s.model_mut().repair_mode = false;
    s.run("PickupContainerItem(0, 1)").unwrap();
    s.model_mut().repair_mode = true;
    s.run("UseContainerItem(0, 2)").unwrap();
    assert!(
        s.cursor_payload().is_none(),
        "an empty slot still clears the cursor"
    );
    assert!(
        s.take_container_repairs().is_empty(),
        "and repairs nothing (`0x4fa1a6`)"
    );
}

#[test]
fn held_vendor_row_buys_into_bag_before_repair_mode() {
    let mut s = UiScript::new().unwrap();
    s.set_container(0, Some(backpack()));
    s.set_merchant(Some(MerchantState {
        items: vec![MerchantItem {
            item_id: 159,
            ..Default::default()
        }],
        can_repair: true,
        ..Default::default()
    }));
    s.run("ShowRepairCursor() PickupMerchantItem(1)").unwrap();
    assert!(
        s.repair_mode(),
        "vendor grab preserves the repair base mode"
    );
    assert!(s.cursor_payload().is_some());

    s.run("PickupContainerItem(0, 1)").unwrap();
    assert_eq!(s.take_merchant_slot_buys(), vec![(0, 1, 159)]);
    assert!(s.cursor_payload().is_none());
    assert!(s.take_container_repairs().is_empty());
    assert!(s.take_sounds().is_empty());

    // The right-click clears the vendor row with no packet and repairs instead.
    s.run("PickupMerchantItem(1) UseContainerItem(0, 1)")
        .unwrap();
    assert!(s.take_merchant_slot_buys().is_empty());
    assert!(s.cursor_payload().is_none());
    assert_eq!(s.take_container_repairs(), vec![(0, 1)]);
    assert!(s.take_container_uses().is_empty());
}

#[test]
fn held_item_places_before_repair_mode() {
    let mut s = UiScript::new().unwrap();
    s.set_container(0, Some(backpack()));
    s.run("PickupContainerItem(0, 1)").unwrap();
    assert!(s.cursor_item().is_some());
    // A held item and repair base mode can coexist after a cursor-mode change.
    s.model_mut().repair_mode = true;

    s.run("PickupContainerItem(0, 2)").unwrap();
    assert_eq!(
        s.take_container_moves(),
        vec![ContainerMove {
            src_bag: 0,
            src_slot: 1,
            dst_bag: 0,
            dst_slot: 2,
            count: None,
        }]
    );
    assert!(s.cursor_payload().is_none());
    assert!(s.take_container_repairs().is_empty());
    assert!(s.take_sounds().is_empty());
}

#[test]
fn repair_mode_repairs_under_held_coins_and_keeps_them() {
    use crate::script::cursor::{CursorMoney, CursorPayload};
    let mut s = UiScript::new().unwrap();
    s.set_container(0, Some(backpack()));
    s.model_mut().repair_mode = true;
    let coins = CursorPayload::Money(CursorMoney { copper: 50 });
    s.model_mut().cursor = Some(coins.clone());

    s.run("PickupContainerItem(0, 1)").unwrap();
    assert_eq!(
        s.take_container_repairs(),
        vec![(0, 1)],
        "coins pass both payload tests (`0x4f9c38`, `0x4f9c43`)"
    );
    assert_eq!(s.cursor_payload(), Some(coins), "and stay held");
    s.run("PickupContainerItem(0, 2)").unwrap();
    assert!(
        s.take_container_repairs().is_empty(),
        "an empty slot does nothing (`0x4f9c4e`)"
    );
}

#[test]
fn removing_a_bag_empties_it() {
    let mut s = UiScript::new().unwrap();
    s.set_container(2, Some(backpack()));
    s.set_container(2, None);
    assert_eq!(s.eval::<i64>("return GetContainerNumSlots(2)").unwrap(), 0);
}

#[test]
fn container_item_cooldown_reads_the_gettime_triple() {
    let mut s = UiScript::new().unwrap();
    s.tick(100.0); // an arbitrary clock epoch
    let mut state = backpack();
    // Slot 1's item is 4 s into a 10 s use-cooldown: started at GetTime 96.
    state.slots.get_mut(&1).unwrap().cooldown = Some((96_000, 10_000, true));
    s.set_container(0, Some(state));

    assert!(s
        .eval::<bool>(
            "local s, d, e = GetContainerItemCooldown(0, 1)\n\
             return s == 96 and d == 10 and e == 1"
        )
        .unwrap());
    assert!(s
        .eval::<bool>(
            "local s, d, e = GetContainerItemCooldown(0, 3)\n\
             return s == 0 and d == 0 and e == 1"
        )
        .unwrap());
    // Past expiry, the same stored triple reads cold.
    s.tick(7.0);
    assert!(s
        .eval::<bool>(
            "local s, d, e = GetContainerItemCooldown(0, 1)\n\
             return s == 0 and d == 0 and e == 1"
        )
        .unwrap());
    // A re-push without the cooldown clears the stored triple.
    s.set_container(0, Some(backpack()));
    assert!(s
        .eval::<bool>(
            "local s, d = GetContainerItemCooldown(0, 1)\n\
             return s == 0 and d == 0"
        )
        .unwrap());
}

// ── `ContainerIDToInventoryID` (`0x4f94e0`) ─────────────────────────────────────

#[test]
fn container_id_to_inventory_id_is_two_lines_with_no_clamp() {
    let s = UiScript::new().unwrap();
    let slot = |id: &str| {
        s.eval::<i64>(&format!("return ContainerIDToInventoryID({id})"))
            .unwrap()
    };
    // The `id <= 4` line: the keyring, -1, the backpack, the four worn bags.
    assert_eq!(slot("-2"), 17, "keyring — an ordinary point, not a case");
    assert_eq!(slot("-1"), 18);
    assert_eq!(slot("0"), 19, "backpack — likewise");
    assert_eq!(
        (slot("1"), slot("2"), slot("3"), slot("4")),
        (20, 21, 22, 23)
    );
    // The `id >= 5` line: the bank bags.
    // Build 12340: the bank bags start at 68, so the line is +63.
    assert_eq!(slot("5"), 68, "the jump — +63, not +19");
    assert_eq!((slot("6"), slot("11")), (69, 74));
    // No range check, no clamp, and never nil: out of range is an ordinary number.
    assert_eq!(
        slot("99"),
        162,
        "the value `SetInventoryItem` would receive"
    );
    assert_eq!(slot("-100"), -81, "negatives run off the line too");
    assert_eq!(
        s.arity("ContainerIDToInventoryID(99)").unwrap(),
        1,
        "one value on every non-raising path"
    );
    // `0x40a2b0` truncates toward zero, a C cast, not `floor`.
    assert_eq!(slot("2.9"), 21, "2.9 -> 2");
    assert_eq!(slot("-2.9"), 17, "-2.9 -> -2, NOT -3");
}

/// A missing or non-number argument raises: the type test's error `0x6f4940` does not return.
#[test]
fn container_id_to_inventory_id_raises_on_a_bad_argument() {
    let s = UiScript::new().unwrap();
    for call in ["ContainerIDToInventoryID()", "ContainerIDToInventoryID({})"] {
        let err = s
            .eval::<mlua::Value>(&format!("return {call}"))
            .unwrap_err();
        assert!(
            format!("{err}").contains("Usage: ContainerIDToInventoryID(containerID)"),
            "{call} must raise, got {err}"
        );
    }
}
