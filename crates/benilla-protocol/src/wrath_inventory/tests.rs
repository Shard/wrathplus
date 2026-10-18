use super::*;
use crate::messages::CorpseTarget;

fn cost_bytes() -> Vec<u8> {
    (1..=13u32).flat_map(u32::to_le_bytes).collect()
}

fn cost() -> RefundCost {
    RefundCost {
        money: 1,
        honor: 2,
        arena: 3,
        items: [(4, 5), (6, 7), (8, 9), (10, 11), (12, 13)],
    }
}

fn set_bytes(index: u8) -> Vec<u8> {
    let mut body = vec![0x01, 0x42, index, 0, 0, 0];
    body.extend(b"Tank\0Icon\0");
    body.extend([0x01, 0x01]); // slot 0 ignored: raw 1
    body.extend([0x81, 0x07, 0x40]); // slot 1: 0x4000000000000007
    body.extend([0; EQUIPMENT_SLOTS - 2]);
    body
}

fn set(index: u32) -> EquipmentSet {
    let mut items = [0; EQUIPMENT_SLOTS];
    items[0] = 1;
    items[1] = 0x4000_0000_0000_0007;
    EquipmentSet {
        set_guid: 0x42,
        index,
        name: "Tank".into(),
        icon: "Icon".into(),
        items,
    }
}

fn fixtures() -> Vec<(u16, Vec<u8>, SessionEvent)> {
    let capture = |packet| SessionEvent::WrathInventory { packet };
    let guid = 0x0400_0000_0000_0102u64.to_le_bytes();
    let with_guid = |tail: &[u8]| {
        let mut body = guid.to_vec();
        body.extend(tail);
        body
    };
    let mut refund_info = with_guid(&cost_bytes());
    refund_info.extend([255; 4]);
    refund_info.extend(60u32.to_le_bytes());
    let mut refund_ok = with_guid(&[0; 4]);
    refund_ok.extend(cost_bytes());
    let mut set_list = 2u32.to_le_bytes().to_vec();
    set_list.extend(set_bytes(0));
    set_list.extend(set_bytes(9));
    let mut enchant_time = with_guid(&[1, 0, 0, 0, 60, 0, 0, 0]);
    enchant_time.extend(guid);
    vec![
        (
            0x05b,
            [7, 0, 0, 0].into_iter().chain(*b"Page\0").chain([8, 0, 0, 0]).collect(),
            SessionEvent::PageText {
                page_id: 7,
                text: "Page".into(),
                next_page_id: 8,
            },
        ),
        (0x0ae, guid.to_vec(), capture(Packet::ReadItemOk { item_guid: u64::from_le_bytes(guid) })),
        (0x0af, guid.to_vec(), capture(Packet::ReadItemFailed { item_guid: u64::from_le_bytes(guid) })),
        (
            0x0b0,
            with_guid(&[0xff, 0, 0, 0]),
            SessionEvent::ItemCooldown {
                item_guid: u64::from_le_bytes(guid),
                spell_id: 255,
            },
        ),
        (
            0x137,
            vec![9, 0, 0, 0, 0x01, 0x42],
            capture(Packet::EquipmentSetSaved {
                index: 9,
                set_guid: 0x42,
            }),
        ),
        (0x1b8, guid.to_vec(), SessionEvent::ShowBank { banker: u64::from_le_bytes(guid) }),
        (0x1ba, vec![3, 0, 0, 0], SessionEvent::BuyBankSlotResult { result: 3 }),
        (
            0x1d7,
            vec![0x01, 0x05, 0x00, 1, 0, 0, 0, 0xff, 0xff, 0, 0],
            capture(Packet::EnchantmentLog {
                target: 5,
                caster: 0,
                item_entry: 1,
                enchant_id: 0xffff,
            }),
        ),
        (
            0x1ea,
            with_guid(&[0, 0, 0, 0]),
            SessionEvent::ItemTime {
                item_guid: u64::from_le_bytes(guid),
                seconds: 0,
            },
        ),
        (
            0x1eb,
            enchant_time,
            SessionEvent::ItemEnchantTime {
                item_guid: u64::from_le_bytes(guid),
                slot: 1,
                seconds: 60,
            },
        ),
        (
            0x244,
            [0].into_iter().chain(guid).chain(*b"Dear\0").collect(),
            capture(Packet::ItemText {
                raw_status: 0,
                text: Some((u64::from_le_bytes(guid), "Dear".into())),
            }),
        ),
        (
            0x244,
            vec![1],
            capture(Packet::ItemText {
                raw_status: 1,
                text: None,
            }),
        ),
        (
            0x2c5,
            [3, 0, 0, 0].into_iter().chain(*b"Set\0").chain([5, 0, 0, 0]).collect(),
            capture(Packet::ItemSetName {
                id: 3,
                name: "Set".into(),
                inventory_type: 5,
            }),
        ),
        (
            0x4b2,
            refund_info,
            capture(Packet::RefundInfo {
                item_guid: u64::from_le_bytes(guid),
                cost: cost(),
                raw_unknown: u32::MAX,
                played_seconds: 60,
            }),
        ),
        (
            0x4b5,
            refund_ok,
            capture(Packet::RefundResult {
                item_guid: u64::from_le_bytes(guid),
                error: 0,
                cost: Some(cost()),
            }),
        ),
        (
            0x4b5,
            with_guid(&[10, 0, 0, 0]),
            capture(Packet::RefundResult {
                item_guid: u64::from_le_bytes(guid),
                error: 10,
                cost: None,
            }),
        ),
        (0x4bc, vec![0; 4], capture(Packet::EquipmentSetList { sets: vec![] })),
        (0x4bc, set_list, capture(Packet::EquipmentSetList { sets: vec![set(0), set(9)] })),
        (0x4d6, vec![4], capture(Packet::EquipmentSetUseResult { result: 4 })),
        (
            0x50b,
            with_guid(&[1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0]),
            capture(Packet::SocketGemsResult {
                item_guid: u64::from_le_bytes(guid),
                enchants: [1, 2, 3, 4],
            }),
        ),
    ]
}

#[test]
fn every_fixture_decodes_and_rejects_truncation_and_trailing_bytes() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    for (opcode, body, expected) in fixtures {
        // `SessionEvent` has no `PartialEq`; its `Debug` form is total over the fields.
        assert_eq!(format!("{:?}", decode(opcode, &body).unwrap()), format!("{expected:?}"));
        for end in 0..body.len() {
            assert!(decode(opcode, &body[..end]).is_err(), "{opcode:#x} truncated at {end}");
        }
        let mut trailing = body.clone();
        trailing.push(0);
        assert!(decode(opcode, &trailing).is_err(), "{opcode:#x} trailing");
    }
}

#[test]
fn malformed_counts_and_foreign_opcodes_are_rejected() {
    assert!(decode(0x4bc, &u32::MAX.to_le_bytes()).is_err());
    assert!(decode(0x4bc, &1u32.to_le_bytes()).is_err());
    // Status 1 never carries a body; error 10 never carries a cost.
    assert!(decode(0x244, &[1, 0]).is_err());
    let mut refund = vec![0; 8];
    refund.extend(10u32.to_le_bytes());
    refund.extend(cost_bytes());
    assert!(decode(0x4b5, &refund).is_err());
    assert!(decode(0x0112, &[0; 8]).is_err());
}

#[test]
fn dispatch_routes_every_opcode_and_reports_malformed_as_unhandled() {
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures() {
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(format!("{session_events:?}"), format!("{:?}", vec![expected]))
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
        assert!(matches!(
            adapter.dispatch(opcode, &body[..body.len() - 1], 0).unwrap(),
            crate::wrath_events::WrathDispatch::Unhandled { opcode: o } if o == opcode
        ));
    }
}

#[test]
fn commands_encode_ac_read_order() {
    let g = 0x0400_0000_0000_0102u64;
    let gb = g.to_le_bytes().to_vec();
    let cat = |a: &[u8], b: &[u8]| [a, b].concat();
    let mut items = [0; EQUIPMENT_SLOTS];
    items[0] = 1;
    items[1] = 0x4000_0000_0000_0007;
    let mut save = vec![0x00, 2, 0, 0, 0];
    save.extend(b"Tank\0Icon\0");
    save.extend([0x01, 0x01, 0x81, 0x07, 0x40]);
    save.extend([0; EQUIPMENT_SLOTS - 2]);
    let mut uses = [(0, 255, 255); EQUIPMENT_SLOTS];
    uses[0] = (1, 255, 255);
    uses[18] = (0x42, 19, 3);
    let mut use_body = vec![0x01, 0x01, 255, 255];
    for _ in 1..18 {
        use_body.extend([0, 255, 255]);
    }
    use_body.extend([0x01, 0x42, 19, 3]);
    let rows = vec![
        (Command::ReadItem { bag: 255, slot: 23 }, 0x0ad, vec![255, 23]),
        (Command::ItemSetNameQuery { id: 3, guid: g }, 0x2c4, cat(&[3, 0, 0, 0], &gb)),
        (Command::ItemTextQuery { item_guid: g }, 0x243, gb.clone()),
        (Command::AutoEquipItemSlot { item_guid: g, dst_slot: 15 }, 0x10f, cat(&gb, &[15])),
        (Command::RefundInfo { item_guid: g }, 0x4b3, gb.clone()),
        (Command::Refund { item_guid: g }, 0x4b4, gb.clone()),
        (
            Command::SocketGems { item_guid: g, gems: [1, 0, 2] },
            0x347,
            [gb.clone(), 1u64.to_le_bytes().to_vec(), vec![0; 8], 2u64.to_le_bytes().to_vec()].concat(),
        ),
        (Command::CancelTempEnchantment { slot: 15 }, 0x379, vec![15, 0, 0, 0]),
        (
            Command::EquipmentSetSave {
                set_guid: 0,
                index: 2,
                name: "Tank".into(),
                icon: "Icon".into(),
                items,
            },
            0x4bd,
            save,
        ),
        (Command::EquipmentSetDelete { set_guid: 0x42 }, 0x13e, vec![0x01, 0x42]),
        (Command::EquipmentSetUse { items: uses }, 0x4d5, use_body),
        (Command::OpenItem { bag: 255, slot: 23 }, 0x0ac, vec![255, 23]),
        (
            Command::WrapItem { gift_bag: 1, gift_slot: 2, item_bag: 3, item_slot: 4 },
            0x1d3,
            vec![1, 2, 3, 4],
        ),
        (Command::AutoEquipItem { bag: 19, slot: 2 }, 0x10a, vec![19, 2]),
        (Command::SetAmmo { entry: 2512 }, 0x268, 2512u32.to_le_bytes().to_vec()),
        (Command::SwapInvItem { src_slot: 23, dst_slot: 24 }, 0x10d, vec![24, 23]),
        (
            Command::SwapItem { dst_bag: 1, dst_slot: 2, src_bag: 3, src_slot: 4 },
            0x10c,
            vec![1, 2, 3, 4],
        ),
        (Command::AutoStoreBagItem { src_bag: 255, src_slot: 23, dst_bag: 19 }, 0x10b, vec![255, 23, 19]),
        (
            Command::SplitItem { src_bag: 1, src_slot: 2, dst_bag: 3, dst_slot: 4, count: 300 },
            0x10e,
            vec![1, 2, 3, 4, 44, 1, 0, 0],
        ),
        (Command::DestroyItem { bag: 255, slot: 23, count: 0 }, 0x111, vec![255, 23, 0, 0, 0, 0]),
        (Command::BankerActivate { banker: g }, 0x1b7, gb.clone()),
        (Command::BuyBankSlot { banker: g }, 0x1b9, gb.clone()),
        (Command::AutoBankItem { bag: 255, slot: 23 }, 0x283, vec![255, 23]),
        (Command::AutoStoreBankItem { bag: 255, slot: 40 }, 0x282, vec![255, 40]),
        (Command::PageTextQuery { page_id: 7, guid: g }, 0x05a, cat(&[7, 0, 0, 0], &gb)),
    ];
    for (command, opcode, body) in rows {
        assert_eq!(encode(&command), (opcode, body), "{command:?}");
    }
}

#[test]
fn use_item_carries_header_guid_glyph_and_each_target_block() {
    let head = |count: u8| {
        let mut body = vec![255, 23, count];
        body.extend(8690u32.to_le_bytes());
        body.extend(0x0400_0000_0000_0102u64.to_le_bytes());
        body.extend([0, 0, 0, 0, 0]);
        body
    };
    let at = [1.0f32, 2.0, 3.0];
    let coords: Vec<u8> = at.iter().flat_map(|v| v.to_le_bytes()).collect();
    let rows = [
        (UseItemTarget::SelfImplicit, vec![0, 0, 0, 0]),
        (UseItemTarget::Unit(0x42), vec![0x02, 0, 0, 0, 0x01, 0x42]),
        (UseItemTarget::Object(0x42), vec![0x00, 0x08, 0, 0, 0x01, 0x42]),
        (UseItemTarget::Item(0x42), vec![0x10, 0, 0, 0, 0x01, 0x42]),
        (UseItemTarget::Corpse(CorpseTarget::Ally, 0x42), vec![0, 0x80, 0, 0, 0x01, 0x42]),
        (UseItemTarget::Corpse(CorpseTarget::Enemy, 0x42), vec![0, 0x02, 0, 0, 0x01, 0x42]),
        (UseItemTarget::Dest(at), [vec![0x40, 0, 0, 0, 0], coords.clone()].concat()),
        (UseItemTarget::Source(at), [vec![0x20, 0, 0, 0, 0], coords.clone()].concat()),
    ];
    for (target, tail) in rows {
        let (opcode, body) = encode_use_item(9, 255, 23, 8690, 0x0400_0000_0000_0102, 0, &target);
        assert_eq!(opcode, 0x0ab);
        assert_eq!(body, [head(9), tail].concat(), "{target:?}");
    }
}
