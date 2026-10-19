use benilla_protocol::events::SessionEvent;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEvent, WrathEventAdapter};
use benilla_protocol::wrath_loot::{
    self, CMSG_AUTOSTORE_LOOT_ITEM, CMSG_LOOT, CMSG_LOOT_MONEY, CMSG_LOOT_RELEASE,
    InventoryFailureExtra, LootCodecError, LootResponse, MAX_LOOT_ITEMS,
    SMSG_INVENTORY_CHANGE_FAILURE, SMSG_ITEM_PUSH_RESULT, SMSG_LOOT_CLEAR_MONEY,
    SMSG_LOOT_MONEY_NOTIFY, SMSG_LOOT_RELEASE_RESPONSE, SMSG_LOOT_REMOVED, SMSG_LOOT_RESPONSE,
    ServerLootPacket, decode, decode_loot_response, encode_autostore_loot_item, encode_loot,
    encode_loot_money, encode_loot_release,
};

const CORPSE: u64 = 0xf130_0001_0000_002a;
const PLAYER: u64 = 0x0000_0000_0000_0042;

fn corpse_response() -> Vec<u8> {
    // AzerothCore Player::SendLoot + LootMgr.cpp operator<<(LootView): unpacked GUID, loot type,
    // gold, u8 row count, then slot + five u32 item words + slot type.
    let mut body = Vec::new();
    body.extend(CORPSE.to_le_bytes());
    body.push(1); // LOOT_CORPSE
    body.extend(0x0102_0304u32.to_le_bytes());
    body.push(2);
    body.push(0);
    body.extend(117u32.to_le_bytes());
    body.extend(3u32.to_le_bytes());
    body.extend(654u32.to_le_bytes());
    body.extend(0u32.to_le_bytes());
    body.extend((-7i32).to_le_bytes());
    body.push(0); // LOOT_SLOT_TYPE_ALLOW_LOOT
    body.push(4);
    body.extend(6948u32.to_le_bytes());
    body.extend(1u32.to_le_bytes());
    body.extend(2222u32.to_le_bytes());
    body.extend(0x1122_3344u32.to_le_bytes());
    body.extend(0u32.to_le_bytes());
    body.push(0);
    body
}

fn item_push() -> Vec<u8> {
    // AzerothCore Player::SendNewItem: GUID, three uint32 booleans, bag byte, six uint32 words.
    let mut body = Vec::new();
    body.extend(PLAYER.to_le_bytes());
    body.extend(0u32.to_le_bytes()); // looted, not received from NPC
    body.extend(0u32.to_le_bytes()); // not created
    body.extend(1u32.to_le_bytes()); // print server-authored receipt
    body.push(255); // player inventory
    body.extend(23u32.to_le_bytes());
    body.extend(117u32.to_le_bytes());
    body.extend(0x5566_7788u32.to_le_bytes());
    body.extend((-7i32).to_le_bytes());
    body.extend(3u32.to_le_bytes());
    body.extend(11u32.to_le_bytes());
    body
}

fn every_truncated_prefix_fails<T: std::fmt::Debug>(
    fixture: &[u8],
    decode: impl Fn(&[u8]) -> Result<T, LootCodecError>,
) {
    for end in 0..fixture.len() {
        assert_eq!(
            decode(&fixture[..end]).unwrap_err(),
            LootCodecError::Truncated,
            "prefix {end}"
        );
    }
}

#[test]
fn client_opcodes_and_bodies_are_exact_build_12340_contracts() {
    assert_eq!(CMSG_AUTOSTORE_LOOT_ITEM, 0x108);
    assert_eq!(CMSG_LOOT, 0x15d);
    assert_eq!(CMSG_LOOT_MONEY, 0x15e);
    assert_eq!(CMSG_LOOT_RELEASE, 0x15f);
    assert_eq!(encode_loot(CORPSE), CORPSE.to_le_bytes());
    assert_eq!(encode_autostore_loot_item(4), [4]);
    assert!(encode_loot_money().is_empty());
    assert_eq!(encode_loot_release(CORPSE), CORPSE.to_le_bytes());
}

#[test]
fn solo_corpse_response_decodes_exact_rows_and_retains_native_suffix() {
    let body = corpse_response();
    let LootResponse::Window {
        guid,
        loot_type,
        gold,
        items,
    } = decode_loot_response(&body).unwrap()
    else {
        panic!("expected corpse response")
    };
    assert_eq!(guid, CORPSE);
    assert_eq!(loot_type, 1);
    assert_eq!(gold, 0x0102_0304);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].slot, 0);
    assert_eq!(items[0].item_id, 117);
    assert_eq!(items[0].count, 3);
    assert_eq!(items[0].display_info_id, 654);
    assert_eq!(items[0].random_suffix, 0);
    assert_eq!(items[0].random_property_id, (-7i32) as u32);
    assert_eq!(items[1].slot, 4);
    assert_eq!(items[1].random_suffix, 0x1122_3344);

    every_truncated_prefix_fails(&body, decode_loot_response);
    let mut trailing = body.clone();
    trailing.push(0);
    assert_eq!(
        decode_loot_response(&trailing).unwrap_err(),
        LootCodecError::TrailingBytes(1)
    );
}

#[test]
fn solo_owner_rows_decode_and_keep_their_type_in_both_adapter_surfaces() {
    // OWNER_PERMISSION emits 4, not 0 (AzerothCore LootMgr.cpp::operator<<(LootView)).
    let mut body = corpse_response();
    body[35] = wrath_loot::LOOT_SLOT_OWNER;
    *body.last_mut().unwrap() = wrath_loot::LOOT_SLOT_OWNER;
    let LootResponse::Window { items, .. } = decode_loot_response(&body).unwrap() else {
        panic!("expected owner corpse response")
    };
    assert!(items.iter().all(|item| item.slot_type == 4));
    assert!(items.iter().all(|item| item.neutral().slot_type == 4));
    every_truncated_prefix_fails(&body, decode_loot_response);

    let mut adapter = WrathEventAdapter::new();
    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        replies,
    } = adapter.dispatch(SMSG_LOOT_RESPONSE, &body, 0).unwrap()
    else {
        panic!("expected handled owner response")
    };
    assert!(replies.is_empty());
    assert!(matches!(
        &session_events[..],
        [SessionEvent::LootResponse { items, .. }]
            if items.len() == 2 && items.iter().all(|item| item.slot_type == 4)
    ));
    assert!(matches!(
        &wrath_events[..],
        [WrathEvent::LootResponse { items, .. }]
            if items.len() == 2 && items.iter().all(|item| item.slot_type == 4)
    ));
    body.push(0);
    assert_eq!(
        decode_loot_response(&body).unwrap_err(),
        LootCodecError::TrailingBytes(1)
    );
}

#[test]
fn response_fences_non_solo_shapes_errors_and_oversized_counts() {
    let mut error = CORPSE.to_le_bytes().to_vec();
    error.extend([0, 4]); // LOOT_NONE, LOOT_ERROR_TOO_FAR
    assert_eq!(
        decode_loot_response(&error).unwrap(),
        LootResponse::Error {
            guid: CORPSE,
            error: 4
        }
    );
    every_truncated_prefix_fails(&error, decode_loot_response);
    error.push(0);
    assert_eq!(
        decode_loot_response(&error).unwrap_err(),
        LootCodecError::TrailingBytes(1)
    );

    let mut unsupported_error = CORPSE.to_le_bytes().to_vec();
    unsupported_error.extend([0, 3]);
    assert_eq!(
        decode_loot_response(&unsupported_error).unwrap_err(),
        LootCodecError::UnsupportedLootError(3)
    );

    for unsupported_type in [5, 9, 20, 21, 22, u8::MAX] {
        let mut body = CORPSE.to_le_bytes().to_vec();
        body.push(unsupported_type);
        assert_eq!(
            decode_loot_response(&body).unwrap_err(),
            LootCodecError::UnsupportedLootType(unsupported_type)
        );
    }

    for slot_type in [1, 2, 3, 5, u8::MAX] {
        let mut group_slot = corpse_response();
        *group_slot.last_mut().unwrap() = slot_type;
        assert_eq!(
            decode_loot_response(&group_slot).unwrap_err(),
            LootCodecError::UnsupportedSlotType(slot_type)
        );
    }

    let mut oversized = CORPSE.to_le_bytes().to_vec();
    oversized.push(1);
    oversized.extend(0u32.to_le_bytes());
    oversized.push(MAX_LOOT_ITEMS + 1);
    assert_eq!(
        decode_loot_response(&oversized).unwrap_err(),
        LootCodecError::ItemCountOutOfRange(19)
    );
}

#[test]
fn every_type_send_loot_writes_decodes_and_reaches_the_session_event() {
    // `Player::SendLoot`: pickpocketing, fishing, disenchanting, skinning, prospecting, milling.
    for loot_type in [2u8, 3, 4, 6, 7, 8] {
        let mut body = corpse_response();
        body[8] = loot_type;
        let Ok(LootResponse::Window {
            loot_type: decoded, ..
        }) = decode_loot_response(&body)
        else {
            panic!("type {loot_type} decodes")
        };
        assert_eq!(decoded, loot_type);
        let mut adapter = WrathEventAdapter::new();
        let Ok(WrathDispatch::Handled { session_events, .. }) =
            adapter.dispatch(SMSG_LOOT_RESPONSE, &body, 0)
        else {
            panic!("type {loot_type} dispatches")
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::LootResponse { loot_type: t, .. }] if *t == loot_type
        ));
    }
}

#[test]
fn removed_money_clear_and_release_require_exact_ack_shapes() {
    assert_eq!(
        decode(SMSG_LOOT_REMOVED, &[7]).unwrap(),
        ServerLootPacket::Removed { slot: 7 }
    );
    assert!(matches!(
        decode(SMSG_LOOT_MONEY_NOTIFY, &[0x78, 0x56, 0x34, 0x12, 1]).unwrap(),
        ServerLootPacket::Money(money) if money.amount == 0x1234_5678 && money.sole_looter
    ));
    assert_eq!(
        decode(SMSG_LOOT_CLEAR_MONEY, &[]).unwrap(),
        ServerLootPacket::ClearMoney
    );

    let mut release = CORPSE.to_le_bytes().to_vec();
    release.push(1);
    assert_eq!(
        decode(SMSG_LOOT_RELEASE_RESPONSE, &release).unwrap(),
        ServerLootPacket::Release { guid: CORPSE }
    );
    every_truncated_prefix_fails(&release, |body| decode(SMSG_LOOT_RELEASE_RESPONSE, body));
    *release.last_mut().unwrap() = 0;
    assert_eq!(
        decode(SMSG_LOOT_RELEASE_RESPONSE, &release).unwrap_err(),
        LootCodecError::InvalidReleaseResult(0)
    );

    assert_eq!(
        decode(SMSG_LOOT_MONEY_NOTIFY, &[0, 0, 0, 0, 2]).unwrap_err(),
        LootCodecError::InvalidBoolean {
            field: "loot-money chat mode",
            value: 2
        }
    );
    every_truncated_prefix_fails(&[7], |body| decode(SMSG_LOOT_REMOVED, body));
    every_truncated_prefix_fails(&[0x78, 0x56, 0x34, 0x12, 1], |body| {
        decode(SMSG_LOOT_MONEY_NOTIFY, body)
    });
    for (opcode, body) in [
        (SMSG_LOOT_REMOVED, vec![7, 0]),
        (SMSG_LOOT_CLEAR_MONEY, vec![0]),
    ] {
        assert_eq!(
            decode(opcode, &body).unwrap_err(),
            LootCodecError::TrailingBytes(1)
        );
    }
}

#[test]
fn item_push_is_the_only_item_receipt_and_retains_wrath_metadata() {
    let body = item_push();
    let ServerLootPacket::ItemPush(push) = decode(SMSG_ITEM_PUSH_RESULT, &body).unwrap() else {
        panic!("expected item push")
    };
    assert_eq!(push.player_guid, PLAYER);
    assert!(!push.from_npc);
    assert!(!push.created);
    assert!(push.show_in_chat);
    assert_eq!(push.bag_slot, 255);
    assert_eq!(push.item_slot, 23);
    assert_eq!(push.item_entry, 117);
    assert_eq!(push.suffix_factor, 0x5566_7788);
    assert_eq!(push.random_property_id, -7);
    assert_eq!(push.count, 3);
    assert_eq!(push.inventory_count, 11);
    every_truncated_prefix_fails(&body, |body| decode(SMSG_ITEM_PUSH_RESULT, body));

    let mut invalid_bool = body.clone();
    invalid_bool[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        decode(SMSG_ITEM_PUSH_RESULT, &invalid_bool).unwrap_err(),
        LootCodecError::InvalidBoolean {
            field: "received",
            value: 2
        }
    );
    let mut trailing = body;
    trailing.push(0);
    assert_eq!(
        decode(SMSG_ITEM_PUSH_RESULT, &trailing).unwrap_err(),
        LootCodecError::TrailingBytes(1)
    );
}

#[test]
fn inventory_failure_uses_wrath_order_and_reason_specific_tail() {
    let mut body = vec![1]; // EQUIP_ERR_CANT_EQUIP_LEVEL_I
    body.extend(0x1111u64.to_le_bytes());
    body.extend(0x2222u64.to_le_bytes());
    body.push(3); // bag type subclass, not a player inventory slot
    body.extend(20u32.to_le_bytes());
    let ServerLootPacket::InventoryFailure(Some(failure)) =
        decode(SMSG_INVENTORY_CHANGE_FAILURE, &body).unwrap()
    else {
        panic!("expected failure")
    };
    assert_eq!(failure.reason, 1);
    assert_eq!(failure.item_guid, 0x1111);
    assert_eq!(failure.item2_guid, 0x2222);
    assert_eq!(failure.bag_subclass, 3);
    assert_eq!(
        failure.extra,
        Some(InventoryFailureExtra::RequiredLevel(20))
    );
    every_truncated_prefix_fails(&body, |body| decode(SMSG_INVENTORY_CHANGE_FAILURE, body));

    assert_eq!(
        decode(SMSG_INVENTORY_CHANGE_FAILURE, &[0]).unwrap(),
        ServerLootPacket::InventoryFailure(None)
    );
    assert_eq!(
        decode(SMSG_INVENTORY_CHANGE_FAILURE, &[83]).unwrap_err(),
        LootCodecError::UnsupportedInventoryResult(83)
    );

    let mut ordinary = vec![50]; // EQUIP_ERR_INVENTORY_FULL, no conditional tail
    ordinary.extend(0u64.to_le_bytes());
    ordinary.extend(0u64.to_le_bytes());
    ordinary.push(0);
    assert!(matches!(
        decode(SMSG_INVENTORY_CHANGE_FAILURE, &ordinary).unwrap(),
        ServerLootPacket::InventoryFailure(Some(failure)) if failure.reason == 50
    ));
}

#[test]
fn adapter_projects_shared_events_but_preserves_native_metadata() {
    let mut adapter = WrathEventAdapter::new();
    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        replies,
    } = adapter
        .dispatch(SMSG_LOOT_RESPONSE, &corpse_response(), 0)
        .unwrap()
    else {
        panic!("expected handled")
    };
    assert!(replies.is_empty());
    assert!(matches!(
        &session_events[..],
        [SessionEvent::LootResponse { guid, loot_type: 1, gold: 0x0102_0304, items }]
            if *guid == CORPSE && items.len() == 2
    ));
    assert!(matches!(
        &wrath_events[..],
        [WrathEvent::LootResponse { guid, gold: 0x0102_0304, items }]
            if *guid == CORPSE && items[1].random_suffix == 0x1122_3344
    ));

    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        ..
    } = adapter
        .dispatch(SMSG_ITEM_PUSH_RESULT, &item_push(), 0)
        .unwrap()
    else {
        panic!("expected handled")
    };
    assert!(matches!(
        &session_events[..],
        [SessionEvent::ItemPushResult(push)] if push.item_entry == 117 && push.count == 3
    ));
    assert!(matches!(
        &wrath_events[..],
        [WrathEvent::ItemPushResult(push)] if push.inventory_count == 11 && push.random_property_id == -7
    ));
}

#[test]
fn adapter_emits_success_only_after_server_ack() {
    let mut adapter = WrathEventAdapter::new();
    for (opcode, body, expected) in [
        (SMSG_LOOT_REMOVED, vec![4], "removed"),
        (SMSG_LOOT_MONEY_NOTIFY, vec![9, 0, 0, 0, 1], "money"),
        (SMSG_LOOT_CLEAR_MONEY, vec![], "clear"),
        (
            { SMSG_LOOT_RELEASE_RESPONSE },
            {
                let mut body = CORPSE.to_le_bytes().to_vec();
                body.push(1);
                body
            },
            "release",
        ),
    ] {
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &body, 0).unwrap()
        else {
            panic!("expected handled")
        };
        assert_eq!(session_events.len(), 1, "{expected}");
        match (expected, &session_events[0]) {
            ("removed", SessionEvent::LootRemoved { slot: 4 })
            | ("money", SessionEvent::LootMoneyNotify { amount: 9 })
            | ("clear", SessionEvent::LootClearMoney)
            | ("release", SessionEvent::LootReleaseResponse { guid: CORPSE }) => {}
            _ => panic!("wrong event for {expected}: {:?}", session_events[0]),
        }
    }
}

#[test]
fn adapter_uses_native_inventory_event_instead_of_inventing_vanilla_bag_slot() {
    let mut body = vec![50]; // EQUIP_ERR_INVENTORY_FULL
    body.extend(0u64.to_le_bytes());
    body.extend(0u64.to_le_bytes());
    body.push(4); // bag subclass, not an inventory slot
    let mut adapter = WrathEventAdapter::new();
    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        ..
    } = adapter
        .dispatch(SMSG_INVENTORY_CHANGE_FAILURE, &body, 0)
        .unwrap()
    else {
        panic!("expected handled")
    };
    assert!(matches!(
        &session_events[..],
        [SessionEvent::WrathInventoryFailure(failure)]
            if failure.reason == 50 && failure.bag_subclass == 4
    ));
    assert!(matches!(
        &wrath_events[..],
        [WrathEvent::InventoryFailure(failure)]
            if failure.reason == 50 && failure.bag_subclass == 4
    ));
}

#[test]
fn unsupported_opcode_is_rejected_by_codec() {
    assert_eq!(
        decode(0xffff, &[]).unwrap_err(),
        LootCodecError::UnsupportedOpcode(0xffff)
    );
    // Keep constants exercised independently of dispatch registration.
    assert_eq!(wrath_loot::SMSG_LOOT_RESPONSE, 0x160);
}
