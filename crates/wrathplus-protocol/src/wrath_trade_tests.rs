use super::*;

#[test]
fn status_preserves_raw_codes_and_conditional_tails() {
    for code in [0u32, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 21, 24, u32::MAX] {
        assert_eq!(
            decode(0x120, &code.to_le_bytes()).unwrap(),
            Packet::Status(Status {
                code,
                tail: StatusTail::None
            })
        );
    }
    let fixtures = [
        (
            vec![1, 0, 0, 0, 8, 7, 6, 5, 4, 3, 2, 1],
            Status {
                code: 1,
                tail: StatusTail::Trader {
                    guid: 0x0102030405060708,
                },
            },
        ),
        (
            vec![2, 0, 0, 0, 0xef, 0xcd, 0xab, 0x89],
            Status {
                code: 2,
                tail: StatusTail::Open {
                    trade_id: 0x89abcdef,
                },
            },
        ),
        (
            vec![
                12, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xfe, 0x78, 0x56, 0x34, 0x12,
            ],
            Status {
                code: 12,
                tail: StatusTail::Close {
                    result: u32::MAX,
                    target_result: 254,
                    item: 0x12345678,
                },
            },
        ),
        (
            vec![22, 0, 0, 0, 255],
            Status {
                code: 22,
                tail: StatusTail::Slot { slot: 255 },
            },
        ),
        (
            vec![23, 0, 0, 0, 128],
            Status {
                code: 23,
                tail: StatusTail::Slot { slot: 128 },
            },
        ),
    ];
    for (bytes, expected) in fixtures {
        assert_eq!(decode(0x120, &bytes).unwrap(), Packet::Status(expected));
        for end in 0..bytes.len() {
            assert!(decode(0x120, &bytes[..end]).is_err(), "prefix {end}");
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode(0x120, &trailing).is_err());
    }
}

fn header() -> Vec<u8> {
    // Independent count words intentionally disagree with each other and the row count.
    vec![
        0xfe, 0x44, 0x33, 0x22, 0x11, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0x88, 0x77, 0x66, 0x55,
        0xcc, 0xbb, 0xaa, 0x99,
    ]
}

#[test]
fn extended_native_72_byte_items_preserve_every_word_and_empty_rows() {
    let mut bytes = header();
    // 18 words = 72 bytes, including GUID low/high halves. No production encoder used.
    let words = [
        1u32, 2, 3, 0x80000002, 0x11223344, 0x55667788, 7, 8, 9, 10, 0xaabbccdd, 0xeeff0011,
        0xfffffffe, 14, 0x80000000, 16, 17, 18,
    ];
    bytes.push(250);
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.push(250); // Preserve duplicate/out-of-range slots, not an indexed Option projection.
    bytes.extend_from_slice(&[0; 72]);
    assert_eq!(bytes.len(), 21 + 2 * 73);
    let Packet::Extended(packet) = decode(0x121, &bytes).unwrap() else {
        panic!()
    };
    assert_eq!(
        (
            packet.trader,
            packet.trade_id,
            packet.count_word_1,
            packet.count_word_2,
            packet.gold,
            packet.spell
        ),
        (254, 0x11223344, u32::MAX, 0, 0x55667788, 0x99aabbcc)
    );
    assert_eq!(
        packet.items[0],
        Item {
            slot: 250,
            entry: 1,
            display_id: 2,
            count: 3,
            wrapped: 0x80000002,
            gift_creator: 0x5566778811223344,
            permanent_enchant: 7,
            socket_enchants: [8, 9, 10],
            creator: 0xeeff0011aabbccdd,
            charges: 0xfffffffe,
            suffix_factor: 14,
            random_property: i32::MIN,
            lock_id: 16,
            max_durability: 17,
            durability: 18
        }
    );
    assert_eq!(packet.items.len(), 2);
    assert_eq!(packet.items[1].slot, 250);
    assert_eq!(packet.items[1].entry, 0);
    assert_eq!(packet.items[1].socket_enchants, [0; 3]);
    assert_eq!(packet.items[1].creator, 0);
    assert_eq!(packet.items[1].durability, 0);
    for end in 0..bytes.len() {
        // Row boundaries are valid explicit snapshots even when count words disagree.
        assert_eq!(
            decode(0x121, &bytes[..end]).is_ok(),
            end == 21 || end == 94,
            "prefix {end}"
        );
    }
    bytes.push(0);
    assert!(decode(0x121, &bytes).is_err());
}

#[test]
fn extended_empty_and_bad_framing_are_strict_without_count_allocations() {
    let Packet::Extended(empty) = decode(0x121, &header()).unwrap() else {
        panic!()
    };
    assert!(empty.items.is_empty());
    assert_eq!(empty.count_word_1, u32::MAX);
    assert!(decode(0x119, &[]).is_err());
    assert!(decode(0x120, &[]).is_err());
    let mut vanilla_item = header();
    vanilla_item.extend_from_slice(&[0; 61]);
    assert!(decode(0x121, &vanilla_item).is_err());
}

#[test]
fn command_opcodes_and_exact_native_sender_bodies() {
    let cases = [
        (
            Command::Initiate {
                target: 0x0102030405060708,
            },
            0x116,
            vec![8, 7, 6, 5, 4, 3, 2, 1],
        ),
        (Command::Begin, 0x117, vec![]),
        (Command::Busy, 0x118, vec![]),
        (Command::Ignore, 0x119, vec![]),
        (Command::Unaccept, 0x11b, vec![]),
        (Command::Cancel, 0x11c, vec![]),
        (
            Command::SetItem {
                trade_slot: 255,
                bag: 254,
                slot: 253,
            },
            0x11d,
            vec![255, 254, 253],
        ),
        (Command::ClearItem { trade_slot: 255 }, 0x11e, vec![255]),
        (
            Command::SetGold { gold: 0x89abcdef },
            0x11f,
            vec![0xef, 0xcd, 0xab, 0x89],
        ),
    ];
    for (cmd, opcode, bytes) in cases {
        assert_eq!(cmd.opcode(), opcode);
        assert_eq!(cmd.encode(), bytes);
    }
    // Native 00703f50 writes a raw four-byte state word, not a synthesized constant/tradeID.
    for raw in [0u32, 1, 0x89abcdef, u32::MAX] {
        let cmd = Command::Accept { raw };
        assert_eq!(cmd.opcode(), 0x11a);
        let body = cmd.encode();
        assert_eq!(body.len(), 4);
        assert_eq!(u32::from_le_bytes(body.try_into().unwrap()), raw);
    }
}
