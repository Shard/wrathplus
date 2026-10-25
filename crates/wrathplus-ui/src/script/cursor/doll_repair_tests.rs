//! Repair mode against the paper-doll clicks.

use super::*;

#[test]
fn repair_mode_queues_an_equipped_slot_without_picking_it_up() {
    let mut s = UiScript::new().unwrap();
    s.set_inventory_slots(doll_slots());
    s.set_merchant(Some(crate::script::MerchantState {
        can_repair: true,
        ..Default::default()
    }));
    s.run("ShowRepairCursor()").unwrap();

    assert!(!s.eval::<bool>("return PickupInventoryItem(1)").unwrap());
    assert!(!s.eval::<bool>("return PickupInventoryItem(1)").unwrap());
    s.run("UseInventoryItem(1)").unwrap();
    assert!(s.cursor_item().is_none());
    assert_eq!(s.take_inventory_repairs(), vec![1, 1, 1]);
    assert!(s.take_inventory_uses().is_empty(), "no item-use intent");
    assert!(s.take_inventory_repairs().is_empty(), "drained");
}

#[test]
fn repair_mode_follows_the_doll_clicks_order() {
    use crate::script::cursor::CursorMoney;
    let mut s = UiScript::new().unwrap();
    let mut slots = doll_slots();
    if let Some(head) = slots[1].as_mut() {
        head.locked = true;
    }
    s.set_inventory_slots(slots);
    s.model_mut().repair_mode = true;

    s.run("PickupInventoryItem(1)").unwrap();
    assert!(
        s.take_inventory_repairs().is_empty(),
        "a locked item does nothing (`0x4c76d2`)"
    );
    let coins = CursorPayload::Money(CursorMoney { copper: 50 });
    s.model_mut().cursor = Some(coins.clone());
    s.run("PickupInventoryItem(11)").unwrap();
    assert_eq!(
        s.take_inventory_repairs(),
        vec![11],
        "coins do not block it"
    );
    assert_eq!(s.cursor_payload(), Some(coins), "and stay held");

    // A held item takes the click first (`0x4c769a`): the ring goes back, nothing repairs.
    s.model_mut().cursor = None;
    s.model_mut().repair_mode = false;
    s.run("PickupInventoryItem(11)").unwrap();
    s.model_mut().repair_mode = true;
    s.run("PickupInventoryItem(11)").unwrap();
    assert!(s.cursor_payload().is_none());
    assert!(s.take_inventory_repairs().is_empty());
}

#[test]
fn repair_mode_right_click_puts_a_held_item_back_then_repairs() {
    let mut s = UiScript::new().unwrap();
    s.set_inventory_slots(doll_slots());
    s.run("PickupInventoryItem(11)").unwrap();
    assert!(s.cursor_item().is_some());
    s.model_mut().repair_mode = true;

    s.run("UseInventoryItem(19)").unwrap();
    assert!(
        s.cursor_payload().is_none(),
        "the ring went back (`0x4c79a9`)"
    );
    assert!(s.take_container_moves().is_empty(), "never placed");
    assert_eq!(s.take_inventory_repairs(), vec![19]);
    assert!(s.take_inventory_uses().is_empty(), "no use either");
}
