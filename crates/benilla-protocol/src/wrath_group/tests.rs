use super::*;

fn checked(opcode: u16, body: &[u8]) -> Packet {
    let packet = decode(opcode, body).unwrap();
    for len in 0..body.len() {
        assert!(
            decode(opcode, &body[..len]).is_err(),
            "accepted truncation {opcode:x}/{len}"
        );
    }
    let mut trailing = body.to_vec();
    trailing.push(0);
    assert!(decode(opcode, &trailing).is_err());
    packet
}

#[test]
fn invites_declines_uninvite_and_client_commands_are_native() {
    for invited in [0, 1] {
        let mut body = vec![invited];
        body.extend(b"Leader\0");
        body.extend([0; 9]);
        assert_eq!(
            checked(SMSG_GROUP_INVITE, &body),
            Packet::Invite {
                invited: invited != 0,
                inviter: "Leader".into(),
                unknown: 0,
                trailer: 0,
            }
        );
        body[0] = 2;
        assert!(decode(SMSG_GROUP_INVITE, &body).is_err());
        body[0] = 1;
        body[12] = 1; // unknown-row count, not a guessed repeat layout
        assert!(decode(SMSG_GROUP_INVITE, &body).is_err());
    }
    assert_eq!(
        checked(SMSG_GROUP_DECLINE, b"Friend\0"),
        Packet::Decline {
            name: "Friend".into()
        }
    );
    assert_eq!(checked(SMSG_GROUP_UNINVITE, &[]), Packet::Uninvite);
    assert_eq!(encode_invite("Friend").unwrap(), b"Friend\0\0\0\0\0");
    assert!(encode_invite("bad\0name").is_err());
    assert_eq!(encode_accept(), [0; 4]);
    assert!(encode_decline().is_empty());
    assert!(encode_leave().is_empty());
    assert_eq!(
        encode_request_stats(0x0123456789abcdef),
        [0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01]
    );
    assert!(!handles(0xffff));
    assert!(decode(0xffff, &[]).is_err());
}

fn roster(group_type: u8, count: u32) -> Vec<u8> {
    let mut b = vec![group_type, 7, 3, 4];
    if group_type & 8 != 0 {
        b.extend([2, 0x78, 0x56, 0x34, 0x12]);
    }
    b.extend(0x1f50000000000001u64.to_le_bytes());
    b.extend(99u32.to_le_bytes());
    b.extend(count.to_le_bytes());
    for i in 0..count.min(40) {
        b.extend(b"Member\0");
        b.extend((u64::from(i) + 20).to_le_bytes());
        b.extend([1, 6, 2, 8]);
    }
    b.extend(20u64.to_le_bytes());
    if count > 0 {
        b.push(2);
        b.extend(21u64.to_le_bytes());
        b.extend([4, 1, 3, 1]);
    }
    b
}

#[test]
fn roster_keeps_lfg_roles_guid_counter_and_difficulties() {
    let Packet::List(r) = checked(SMSG_GROUP_LIST, &roster(0x0a, 1)) else {
        panic!()
    };
    assert_eq!((r.group_type, r.subgroup, r.flags, r.roles), (10, 7, 3, 4));
    assert_eq!(r.lfg, Some((2, 0x12345678)));
    assert_eq!((r.group_guid, r.counter), (0x1f50000000000001, 99));
    assert_eq!(
        r.members[0],
        Member {
            name: "Member".into(),
            guid: 20,
            status: 1,
            subgroup: 6,
            flags: 2,
            roles: 8
        }
    );
    assert_eq!(r.leader, 20);
    assert_eq!(
        r.loot,
        Some(Loot {
            method: 2,
            master: 21,
            threshold: 4,
            dungeon_difficulty: 1,
            raid_difficulty: 3,
            dynamic_raid_difficulty: 1
        })
    );
    let Packet::List(r) = checked(SMSG_GROUP_LIST, &roster(2, 39)) else {
        panic!()
    };
    assert_eq!(r.members.len(), 39);
    assert_eq!(r.lfg, None);
    assert!(decode(SMSG_GROUP_LIST, &roster(2, 40)).is_err());
    // AC remove/disband sends a 28-byte empty roster, not Vanilla's 14 bytes.
    let mut empty = vec![0x10, 0, 0, 0];
    empty.extend([0; 24]);
    let Packet::List(r) = checked(SMSG_GROUP_LIST, &empty) else {
        panic!()
    };
    assert!(r.members.is_empty());
    assert!(r.loot.is_none());
}

#[test]
fn all_stats_fields_use_wrath_widths_and_bit_positions() {
    let mut b = vec![0x81, 0x12, 0xf1]; // independently packed guid, not codec output
    b.extend(0x000fffffu32.to_le_bytes());
    b.extend(0x1234u16.to_le_bytes());
    b.extend(123456u32.to_le_bytes());
    b.extend(u32::MAX.to_le_bytes());
    b.push(6);
    b.extend(439u16.to_le_bytes());
    b.extend(1000u16.to_le_bytes());
    b.extend(80u16.to_le_bytes());
    b.extend(4000u16.to_le_bytes());
    b.extend([0xff, 0xff, 0x00, 0x80]);
    b.extend(((1u64 << 63) | 1).to_le_bytes());
    b.extend(123456u32.to_le_bytes());
    b.push(0xa5);
    b.extend(0u32.to_le_bytes());
    b.push(0); // explicit slot-63 removal
    b.extend(0xf140000000000001u64.to_le_bytes());
    b.extend(b"Pet\0");
    b.extend(1234u16.to_le_bytes());
    b.extend(987654u32.to_le_bytes());
    b.extend(1000000u32.to_le_bytes());
    b.push(2);
    b.extend(15u16.to_le_bytes());
    b.extend(120u16.to_le_bytes());
    b.extend((1u64 << 55).to_le_bytes());
    b.extend(90000u32.to_le_bytes());
    b.push(0x39);
    b.extend(1234567u32.to_le_bytes());
    let Packet::Stats(s) = checked(SMSG_PARTY_MEMBER_STATS, &b) else {
        panic!()
    };
    assert_eq!(s.guid, 0xf100000000000012);
    assert_eq!(s.full_prefix, None);
    assert_eq!(s.mask, mask::ALL);
    assert_eq!(s.status, Some(0x1234));
    assert_eq!(s.cur_hp, Some(123456));
    assert_eq!(s.max_hp, Some(u32::MAX));
    assert_eq!(
        (s.power_type, s.cur_power, s.max_power),
        (Some(6), Some(439), Some(1000))
    );
    assert_eq!(
        (s.level, s.zone, s.position),
        (Some(80), Some(4000), Some((65535, 32768)))
    );
    assert_eq!(
        s.auras,
        Some(vec![
            Aura {
                slot: 0,
                spell: 123456,
                flags: 0xa5
            },
            Aura {
                slot: 63,
                spell: 0,
                flags: 0
            }
        ])
    );
    assert_eq!(s.pet_guid, Some(0xf140000000000001));
    assert_eq!(s.pet_name.as_deref(), Some("Pet"));
    assert_eq!(s.pet_model_id, Some(1234));
    assert_eq!((s.pet_cur_hp, s.pet_max_hp), (Some(987654), Some(1000000)));
    assert_eq!(
        (s.pet_power_type, s.pet_cur_power, s.pet_max_power),
        (Some(2), Some(15), Some(120))
    );
    assert_eq!(
        s.pet_auras,
        Some(vec![Aura {
            slot: 55,
            spell: 90000,
            flags: 0x39
        }])
    );
    assert_eq!(s.vehicle_seat, Some(1234567));
    let mut full = vec![0];
    full.extend(b);
    let Packet::Stats(s) = checked(SMSG_PARTY_MEMBER_STATS_FULL, &full) else {
        panic!()
    };
    assert_eq!(s.full_prefix, Some(0));
}

#[test]
fn sparse_zero_offline_full_and_unknown_masks() {
    let Packet::Stats(s) = checked(SMSG_PARTY_MEMBER_STATS_FULL, &[0, 1, 7, 1, 0, 0, 0, 0, 0])
    else {
        panic!()
    };
    assert_eq!(s.status, Some(0));
    assert_eq!(s.cur_hp, None);
    assert_eq!(s.auras, None);
    let Packet::Stats(s) = checked(SMSG_PARTY_MEMBER_STATS, &[1, 7, 2, 0, 0, 0, 0, 0, 0, 0]) else {
        panic!()
    };
    assert_eq!(s.cur_hp, Some(0));
    assert_eq!(s.max_hp, None);
    assert!(decode(SMSG_PARTY_MEMBER_STATS, &[1, 7, 0, 0, 0x10, 0]).is_err());
    // Empty aura block is present-but-empty, not absent.
    let mut b = vec![1, 7, 0, 2, 0, 0];
    b.extend([0; 8]);
    let Packet::Stats(s) = checked(SMSG_PARTY_MEMBER_STATS, &b) else {
        panic!()
    };
    assert_eq!(s.auras, Some(vec![]));
}

#[test]
fn every_single_bit_consumes_only_its_native_field() {
    // Group.h::GroupUpdateLength, with zero-length strings and empty aura masks.
    let widths = [2, 4, 4, 1, 2, 2, 2, 2, 4, 8, 8, 1, 2, 4, 4, 1, 2, 2, 8, 4];
    for (bit, width) in widths.into_iter().enumerate() {
        let mut body = vec![1, 7];
        body.extend((1u32 << bit).to_le_bytes());
        body.extend(vec![0; width]);
        let Packet::Stats(s) = checked(SMSG_PARTY_MEMBER_STATS, &body) else {
            panic!()
        };
        assert_eq!(s.mask, 1 << bit);
    }
}
