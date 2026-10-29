use super::*;

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn debug(event: &SessionEvent) -> String {
    // `SessionEvent` has no `PartialEq`; its `Debug` form is total over the fields.
    format!("{event:?}")
}

fn auction_row_bytes(id: u32) -> Vec<u8> {
    let mut row = words(&[id, 2589]);
    let mut enchants = vec![0u32; 21];
    enchants[0] = 15; // permanent
    enchants[3] = 99; // a temporary enchant the shared row drops
    row.extend(words(&enchants));
    row.extend(words(&[u32::MAX, 7, 20, u32::MAX, 0]));
    row.extend(0x42u64.to_le_bytes());
    row.extend(words(&[100, 5, 500, 60_000]));
    row.extend(0x43u64.to_le_bytes());
    row.extend(words(&[110]));
    assert_eq!(row.len(), AUCTION_ROW_BYTES);
    row
}

fn auction_row(id: u32) -> AuctionListEntry {
    AuctionListEntry {
        auction_id: id,
        item_entry: 2589,
        perm_enchant: 15,
        random_property_id: -1,
        suffix_factor: 7,
        count: 20,
        spell_charges: -1,
        owner_guid: 0x42,
        start_bid: 100,
        min_increment: 5,
        buyout: 500,
        time_left_ms: 60_000,
        bidder_guid: 0x43,
        current_bid: 110,
    }
}

fn trade_row(slot: u8, entry: u32) -> Vec<u8> {
    let mut row = vec![slot];
    row.extend(words(&[entry, 7, 3, 1]));
    row.extend(5u64.to_le_bytes());
    row.extend(words(&[11, 21, 22, 23]));
    row.extend(6u64.to_le_bytes());
    row.extend(words(&[u32::MAX, 4, u32::MAX, 9, 50, 40]));
    assert_eq!(row.len(), 73);
    row
}

fn fixtures() -> Vec<(u16, Vec<u8>, SessionEvent)> {
    let mut extended = vec![1];
    extended.extend(words(&[0, 7, 7, 1000, 13]));
    for slot in 0..7 {
        extended.extend(trade_row(slot, if slot == 2 { 2589 } else { 0 }));
    }
    let mut slots = [None; TRADE_SLOT_COUNT];
    slots[2] = Some(TradeItem {
        entry: 2589,
        display_id: 7,
        count: 3,
        wrapped: true,
        gift_creator: 5,
        perm_enchant: 11,
        creator: 6,
        charges: -1,
        suffix_factor: 4,
        random_prop_id: u32::MAX,
        lock_id: 9,
        max_durability: 50,
        durability: 40,
    });
    let mut list = words(&[2]);
    list.extend(auction_row_bytes(1));
    list.extend(auction_row_bytes(2));
    list.extend(words(&[9, 300]));
    let auctions = vec![auction_row(1), auction_row(2)];
    let mut pending = words(&[1]);
    pending.extend(b"a\0b\0");
    pending.extend(words(&[1, 2]));
    pending.extend(1.5f32.to_le_bytes());
    let mut bidder = words(&[2, 77]);
    bidder.extend(0x42u64.to_le_bytes());
    bidder.extend(words(&[100, 5, 2589, 0]));
    let mut owner = words(&[77, 100, 0]);
    owner.extend(0u64.to_le_bytes());
    owner.extend(words(&[2589, 0, 0]));
    let mut master = vec![2];
    master.extend(1u64.to_le_bytes());
    master.extend(2u64.to_le_bytes());
    let mut hello = 0x42u64.to_le_bytes().to_vec();
    hello.extend(words(&[2]));
    hello.push(1);
    vec![
        (
            0x120,
            words(&[0]),
            SessionEvent::TradeStatus {
                status: TradeStatus::Busy,
            },
        ),
        (
            0x120,
            [words(&[1]), 9u64.to_le_bytes().to_vec()].concat(),
            SessionEvent::TradeStatus {
                status: TradeStatus::BeginTrade { partner: 9 },
            },
        ),
        (
            0x120,
            words(&[2, 0]),
            SessionEvent::TradeStatus {
                status: TradeStatus::OpenWindow,
            },
        ),
        (
            0x120,
            [words(&[12, 3]), vec![1], words(&[4])].concat(),
            SessionEvent::TradeStatus {
                status: TradeStatus::CloseWindow {
                    result: 3,
                    item_limit_category: 4,
                },
            },
        ),
        (
            0x120,
            [words(&[22]), vec![6]].concat(),
            SessionEvent::TradeStatus {
                status: TradeStatus::OnlyConjured { slot: 6 },
            },
        ),
        (
            0x120,
            [words(&[23]), vec![6]].concat(),
            SessionEvent::TradeStatus {
                status: TradeStatus::Unknown(23),
            },
        ),
        (
            0x121,
            extended,
            SessionEvent::TradeStatusExtended {
                state: Box::new(TradeStatusExtended {
                    their_window: true,
                    gold: 1000,
                    enchant_spell_id: 13,
                    slots,
                }),
            },
        ),
        (
            0x239,
            words(&[5, 0, 0]),
            SessionEvent::SendMailResult {
                mail_id: 5,
                action: 0,
                error: 0,
                equip_error: None,
                item: None,
            },
        ),
        (
            0x239,
            words(&[5, 2, 1, 22]),
            SessionEvent::SendMailResult {
                mail_id: 5,
                action: 2,
                error: 1,
                equip_error: Some(22),
                item: None,
            },
        ),
        (
            0x239,
            words(&[5, 2, 0, 0x99, 3]),
            SessionEvent::SendMailResult {
                mail_id: 5,
                action: 2,
                error: 0,
                equip_error: None,
                item: Some((0x99, 3)),
            },
        ),
        (
            0x255,
            hello,
            SessionEvent::AuctionHello {
                auctioneer: 0x42,
                house_id: 2,
            },
        ),
        (
            0x25b,
            words(&[0, 0, 0]),
            SessionEvent::AuctionCommandResult {
                auction_id: 0,
                action: 0,
                error: 0,
                tail: AuctionCommandTail::Empty,
            },
        ),
        (
            0x25b,
            words(&[7, 2, 0, 6]),
            SessionEvent::AuctionCommandResult {
                auction_id: 7,
                action: 2,
                error: 0,
                tail: AuctionCommandTail::BidPlaced { new_min_outbid: 6 },
            },
        ),
        (
            0x25b,
            words(&[7, 1, 0, 0]),
            SessionEvent::AuctionCommandResult {
                auction_id: 7,
                action: 1,
                error: 0,
                tail: AuctionCommandTail::Empty,
            },
        ),
        (
            0x25b,
            words(&[0, 2, 5]),
            SessionEvent::AuctionCommandResult {
                auction_id: 0,
                action: 2,
                error: 5,
                tail: AuctionCommandTail::Empty,
            },
        ),
        (
            0x25c,
            list.clone(),
            SessionEvent::AuctionListResult {
                auctions: auctions.clone(),
                total_count: 9,
            },
        ),
        (
            0x25d,
            list.clone(),
            SessionEvent::AuctionOwnerListResult {
                auctions: auctions.clone(),
                total_count: 9,
            },
        ),
        (
            0x265,
            list,
            SessionEvent::AuctionBidderListResult {
                auctions,
                total_count: 9,
            },
        ),
        (
            0x265,
            words(&[0, 0, 300]),
            SessionEvent::AuctionBidderListResult {
                auctions: vec![],
                total_count: 0,
            },
        ),
        (
            0x25e,
            bidder,
            SessionEvent::AuctionBidderNotification(AuctionBidderNotification {
                house_id: 2,
                auction_id: 77,
                bidder_guid: 0x42,
                bid_or_zero: 100,
                out_bid: 5,
                item_entry: 2589,
                random_property_id: 0,
            }),
        ),
        (
            0x25f,
            owner,
            SessionEvent::AuctionOwnerNotification(AuctionOwnerNotification {
                auction_id: 77,
                bid: 100,
                out_bid: 0,
                bidder_guid: 0,
                item_entry: 2589,
                random_property_id: 0,
            }),
        ),
        (
            0x2a4,
            master,
            SessionEvent::LootMasterList {
                candidates: vec![1, 2],
            },
        ),
        (
            0x490,
            words(&[0]),
            SessionEvent::WrathExchange {
                packet: Packet::AuctionPendingSales { rows: vec![] },
            },
        ),
        (
            0x490,
            pending,
            SessionEvent::WrathExchange {
                packet: Packet::AuctionPendingSales {
                    rows: vec![PendingSale {
                        first: "a".into(),
                        second: "b".into(),
                        raw_words: [1, 2],
                        raw_float: 1.5,
                    }],
                },
            },
        ),
    ]
}

#[test]
fn every_fixture_maps_to_its_shared_event_and_rejects_bad_extent() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    for (opcode, body, expected) in fixtures {
        assert_eq!(
            debug(&decode(opcode, &body).unwrap()),
            debug(&expected),
            "{opcode:#x}"
        );
        for end in 0..body.len() {
            assert!(
                decode(opcode, &body[..end]).is_err(),
                "{opcode:#x} truncated at {end}"
            );
        }
        let mut trailing = body.clone();
        trailing.push(0);
        assert!(decode(opcode, &trailing).is_err(), "{opcode:#x} trailing");
    }
}

#[test]
fn counts_beyond_the_body_and_foreign_opcodes_are_rejected() {
    assert!(decode(0x25c, &words(&[u32::MAX, 0, 0])).is_err());
    assert!(decode(0x490, &words(&[u32::MAX])).is_err());
    assert!(decode(0x297, &[0; 8]).is_err());
    assert!(decode(0x112, &[0; 8]).is_err());
}

#[test]
fn dispatch_routes_every_opcode_and_reports_malformed_as_unhandled() {
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures() {
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(
                    format!("{session_events:?}"),
                    format!("{:?}", vec![expected])
                )
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
        assert!(matches!(
            adapter.dispatch(opcode, &[body.as_slice(), &[0]].concat(), 0).unwrap(),
            crate::wrath_events::WrathDispatch::Unhandled { opcode: o } if o == opcode
        ));
    }
}

#[test]
fn commands_encode_ac_read_order() {
    let g = 0x0400_0000_0000_0102u64;
    let gb = g.to_le_bytes().to_vec();
    let rows = vec![
        (Command::AuctionHello { auctioneer: g }, 0x255, gb.clone()),
        (
            Command::AuctionSell {
                auctioneer: g,
                items: vec![(9, 20)],
                bid: 100,
                buyout: 500,
                etime_minutes: 720,
            },
            0x256,
            [
                gb.clone(),
                words(&[1]),
                9u64.to_le_bytes().to_vec(),
                words(&[20, 100, 500, 720]),
            ]
            .concat(),
        ),
        (
            Command::AuctionRemove {
                auctioneer: g,
                auction_id: 7,
            },
            0x257,
            [gb.clone(), words(&[7])].concat(),
        ),
        (
            Command::AuctionListItems {
                auctioneer: g,
                list_from: 50,
                searched_name: "Linen".into(),
                level_min: 1,
                level_max: 80,
                slot_id: u32::MAX,
                main_category: 1,
                sub_category: 2,
                quality: u32::MAX,
                usable: 1,
            },
            0x258,
            [
                gb.clone(),
                words(&[50]),
                b"Linen\0".to_vec(),
                vec![1, 80],
                words(&[u32::MAX, 1, 2, u32::MAX]),
                vec![1, 0, 0],
            ]
            .concat(),
        ),
        (
            Command::AuctionListOwner {
                auctioneer: g,
                list_from: 0,
            },
            0x259,
            [gb.clone(), words(&[0])].concat(),
        ),
        (
            Command::AuctionListBidder {
                auctioneer: g,
                list_from: 0,
                auction_ids: vec![3, 4],
            },
            0x264,
            [gb.clone(), words(&[0, 2, 3, 4])].concat(),
        ),
        (
            Command::AuctionPlaceBid {
                auctioneer: g,
                auction_id: 7,
                price: 120,
            },
            0x25a,
            [gb.clone(), words(&[7, 120])].concat(),
        ),
        (
            Command::AuctionListPendingSales { auctioneer: g },
            0x48f,
            gb.clone(),
        ),
        (
            Command::LootMasterGive {
                loot: g,
                slot: 3,
                target: 9,
            },
            0x2a3,
            [gb.clone(), vec![3], 9u64.to_le_bytes().to_vec()].concat(),
        ),
        (Command::OptOutOfLoot { pass: 1 }, 0x409, words(&[1])),
    ];
    for (command, opcode, body) in rows {
        assert_eq!(encode(&command), (opcode, body), "{command:?}");
    }
}
