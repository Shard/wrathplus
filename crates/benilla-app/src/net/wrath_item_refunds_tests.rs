use super::*;

#[test]
fn walker_slots_map_to_the_binding_keys() {
    assert_eq!(lua_key(255, 0), Some((true, 0, 1)), "head");
    assert_eq!(lua_key(255, 18), Some((true, 0, 19)), "tabard");
    assert_eq!(lua_key(255, 23), Some((false, 0, 1)), "backpack first");
    assert_eq!(lua_key(255, 38), Some((false, 0, 16)), "backpack last");
    assert_eq!(lua_key(19, 0), Some((false, 1, 1)), "first bag");
    assert_eq!(lua_key(22, 35), Some((false, 4, 36)), "last bag");
    for (bag, slot) in [(255, 19), (255, 22), (255, 39), (255, 86), (23, 0)] {
        assert_eq!(lua_key(bag, slot), None, "{bag}/{slot} is no ContainerFrame slot");
    }
}

#[test]
fn refund_replies_leave_the_capture_and_other_receipts_keep_their_order() {
    let cost = RefundCost { money: 1, honor: 2, arena: 3, items: [(4, 5), (0, 0), (0, 0), (0, 0), (0, 0)] };
    let mut packets = std::collections::VecDeque::from([
        Packet::ReadItemOk { item_guid: 1 },
        Packet::RefundInfo { item_guid: 9, cost: cost.clone(), raw_unknown: 0, played_seconds: 600 },
        Packet::EquipmentSetUseResult { result: 0 },
        Packet::RefundResult { item_guid: 8, error: 10, cost: None },
    ]);
    let mut refunds = ItemRefunds::default();
    absorb(&mut packets, &mut refunds, 50.0);
    assert_eq!(
        packets.iter().cloned().collect::<Vec<_>>(),
        vec![Packet::ReadItemOk { item_guid: 1 }, Packet::EquipmentSetUseResult { result: 0 }]
    );
    assert_eq!(refunds.infos.get(&9), Some(&(cost.clone(), 6600.0, 50.0)));
    // A refused refund keeps the row; a successful one removes it.
    packets.push_back(Packet::RefundResult { item_guid: 9, error: 0, cost: Some(cost) });
    absorb(&mut packets, &mut refunds, 60.0);
    assert!(refunds.infos.is_empty());
    assert_eq!(packets.len(), 2, "only the refund replies leave");
}
