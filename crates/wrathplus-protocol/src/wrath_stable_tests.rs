use super::*;

pub fn body(count: u8) -> Vec<u8> {
    let mut b = 0x8877665544332211u64.to_le_bytes().to_vec();
    b.extend([count, 255]);
    for i in 0..count {
        for word in [u32::MAX, i as u32, 0x80000000] {
            b.extend(word.to_le_bytes());
        }
        b.extend([255, b'x', 0, i]);
    }
    b
}
#[test]
fn list_preserves_order_duplicates_raw_names_flags_and_counts() {
    for count in [0, 1, 5, 255] {
        let Packet::List {
            npc,
            purchased_slots,
            pets,
        } = decode(MSG_LIST_STABLED_PETS, &body(count)).unwrap()
        else {
            panic!()
        };
        assert_eq!(npc, 0x8877665544332211);
        assert_eq!(purchased_slots, 255);
        assert_eq!(pets.len(), count as usize);
        for (i, pet) in pets.iter().enumerate() {
            assert_eq!(
                pet,
                &Pet {
                    pet_number: u32::MAX,
                    creature_entry: i as u32,
                    level: 0x80000000,
                    name: vec![255, b'x'],
                    flags: i as u8
                }
            );
        }
    }
}
#[test]
fn strict_framing_rejects_truncation_counts_missing_terminator_and_tails() {
    let b = body(2);
    for end in 0..b.len() {
        assert!(decode(MSG_LIST_STABLED_PETS, &b[..end]).is_err());
    }
    let mut b = b;
    b.push(0);
    assert!(decode(MSG_LIST_STABLED_PETS, &b).is_err());
    let mut b = body(1);
    b[8] = 2;
    assert!(decode(MSG_LIST_STABLED_PETS, &b).is_err());
    let mut b = body(1);
    b[24] = 255;
    assert!(decode(MSG_LIST_STABLED_PETS, &b).is_err());
    assert!(decode(0xffff, &[]).is_err());
    assert!(decode(SMSG_STABLE_RESULT, &[]).is_err());
    assert!(decode(SMSG_STABLE_RESULT, &[0, 0]).is_err());
    for result in 0..=255 {
        assert_eq!(
            decode(SMSG_STABLE_RESULT, &[result]).unwrap(),
            Packet::Result { result }
        );
    }
}
#[test]
fn all_five_commands_encode_exact_native_bodies() {
    for (command, opcode, pet) in [
        (
            Command::List {
                npc: 0x0102030405060708,
            },
            0x26f,
            None,
        ),
        (
            Command::Stable {
                npc: 0x0102030405060708,
            },
            0x270,
            None,
        ),
        (
            Command::Unstable {
                npc: 0x0102030405060708,
                pet_number: u32::MAX,
            },
            0x271,
            Some(u32::MAX),
        ),
        (
            Command::BuySlot {
                npc: 0x0102030405060708,
            },
            0x272,
            None,
        ),
        (
            Command::Swap {
                npc: 0x0102030405060708,
                pet_number: 0,
            },
            0x275,
            Some(0),
        ),
    ] {
        let mut expected = vec![8, 7, 6, 5, 4, 3, 2, 1];
        if let Some(pet) = pet {
            expected.extend(pet.to_le_bytes());
        }
        assert_eq!(command.opcode(), opcode);
        assert_eq!(command.encode(), expected);
    }
}
