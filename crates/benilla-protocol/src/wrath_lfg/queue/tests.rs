use super::*;

const PLAYER: &[u8] = &[5, 2, 3, 4, 5, 1, 0x44, 0x33, 0x22, 0xa1, b'x', 0];
const PARTY: &[u8] = &[6, 7, 8, 9, 10, 11, 12, 13, 14, 1, 4, 3, 2, 0xfe, 0];
const CHOSEN: &[u8] = &[8, 7, 6, 5, 4, 3, 2, 1, 0xfe, 0xff, 0xff, 0xff, 0xff];
const ROLES: &[u8] = &[
    9, 0, 0, 0, 0xfe, 1, 4, 3, 2, 0xa1, 2, 8, 7, 6, 5, 4, 3, 2, 1, 2, 0xff, 0xff, 0xff, 0xff, 80,
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const JOIN: &[u8] = &[
    6, 0, 0, 0, 7, 0, 0, 0, 1, 8, 7, 6, 5, 4, 3, 2, 1, 1, 0, 0, 0, 4, 3, 2, 0xa1, 0xff, 0xff, 0xff,
    0xff,
];
const QUEUE: &[u8] = &[
    4, 3, 2, 0xa1, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0xff, 0xff, 0x7f, 5, 0,
    0, 0, 1, 2, 3, 0xff, 0xff, 0xff, 0xff,
];
#[test]
fn player_and_party_status_keep_distinct_shapes_and_raw_bytes() {
    assert_eq!(
        decode(SMSG_LFG_UPDATE_PLAYER, PLAYER).unwrap(),
        Packet::UpdatePlayer {
            update_type: 5,
            extra_flag: 2,
            extra: Some(PlayerExtra {
                queued: 3,
                reserved: [4, 5],
                dungeons: vec![0xa1223344],
                comment: "x".into()
            })
        }
    );
    assert_eq!(
        decode(SMSG_LFG_UPDATE_PARTY, PARTY).unwrap(),
        Packet::UpdateParty {
            update_type: 6,
            extra_flag: 7,
            extra: Some(PartyExtra {
                join: 8,
                queued: 9,
                reserved: [10, 11, 12, 13, 14],
                dungeons: vec![0xfe020304],
                comment: String::new()
            })
        }
    );
    assert_eq!(
        decode(SMSG_LFG_UPDATE_PLAYER, &[8, 0]).unwrap(),
        Packet::UpdatePlayer {
            update_type: 8,
            extra_flag: 0,
            extra: None
        }
    );
    assert_eq!(
        decode(SMSG_LFG_UPDATE_PARTY, &[9, 0]).unwrap(),
        Packet::UpdateParty {
            update_type: 9,
            extra_flag: 0,
            extra: None
        }
    );
}
#[test]
fn role_state_preserves_leader_first_order_and_unknown_bits() {
    assert_eq!(
        decode(SMSG_LFG_ROLE_CHOSEN, CHOSEN).unwrap(),
        Packet::RoleChosen {
            guid: 0x0102030405060708,
            ready: 0xfe,
            roles: u32::MAX
        }
    );
    assert_eq!(
        decode(SMSG_LFG_ROLE_CHECK_UPDATE, ROLES).unwrap(),
        Packet::RoleCheck {
            state: 9,
            initializing: 0xfe,
            dungeons: vec![0xa1020304],
            members: vec![
                RoleMember {
                    guid: 0x0102030405060708,
                    ready: 2,
                    roles: u32::MAX,
                    level: 80
                },
                RoleMember {
                    guid: 1,
                    ready: 0,
                    roles: 0,
                    level: 0
                },
            ]
        }
    );
    assert_eq!(
        decode(SMSG_LFG_ROLE_CHECK_UPDATE, &[0, 0, 0, 0, 0, 0, 0]).unwrap(),
        Packet::RoleCheck {
            state: 0,
            initializing: 0,
            dungeons: vec![],
            members: vec![]
        }
    );
}
#[test]
fn join_result_retains_absent_versus_explicit_empty_lock_block() {
    assert_eq!(
        decode(SMSG_LFG_JOIN_RESULT, JOIN).unwrap(),
        Packet::JoinResult {
            result: 6,
            state: 7,
            locks: Some(vec![PartyMemberLocks {
                guid: 0x0102030405060708,
                locks: vec![super::super::DungeonLock {
                    entry: 0xa1020304,
                    reason: u32::MAX
                }]
            }])
        }
    );
    assert_eq!(
        decode(SMSG_LFG_JOIN_RESULT, &[6, 0, 0, 0, 7, 0, 0, 0]).unwrap(),
        Packet::JoinResult {
            result: 6,
            state: 7,
            locks: None
        }
    );
    assert_eq!(
        decode(SMSG_LFG_JOIN_RESULT, &[6, 0, 0, 0, 7, 0, 0, 0, 0]).unwrap(),
        Packet::JoinResult {
            result: 6,
            state: 7,
            locks: Some(vec![])
        }
    );
}
#[test]
fn queue_status_waits_are_signed_but_elapsed_time_is_unsigned() {
    assert_eq!(
        decode(SMSG_LFG_QUEUE_STATUS, QUEUE).unwrap(),
        Packet::QueueStatus {
            dungeon: 0xa1020304,
            average_wait: -1,
            wait: 0,
            tank_wait: i32::MIN,
            healer_wait: i32::MAX,
            dps_wait: 5,
            tanks: 1,
            healers: 2,
            dps: 3,
            queued_time: u32::MAX
        }
    );
}
#[test]
fn every_body_boundary_and_trailing_data_is_checked_transactionally() {
    for (opcode, body) in [
        (SMSG_LFG_UPDATE_PLAYER, PLAYER),
        (SMSG_LFG_UPDATE_PARTY, PARTY),
        (SMSG_LFG_ROLE_CHOSEN, CHOSEN),
        (SMSG_LFG_ROLE_CHECK_UPDATE, ROLES),
        (SMSG_LFG_JOIN_RESULT, JOIN),
        (SMSG_LFG_QUEUE_STATUS, QUEUE),
    ] {
        assert!(super::super::handles(opcode));
        assert_eq!(
            super::super::decode(opcode, body).unwrap(),
            super::super::Packet::Queue(decode(opcode, body).unwrap())
        );
        for end in 0..body.len() {
            // Eight-byte JOIN_RESULT is a legitimate reply without lock block.
            if opcode == SMSG_LFG_JOIN_RESULT && end == 8 {
                continue;
            }
            assert!(
                decode(opcode, &body[..end]).is_err(),
                "{opcode:x} prefix{end}"
            );
        }
        let mut extra = body.to_vec();
        extra.push(0);
        assert!(decode(opcode, &extra).is_err());
    }
    assert!(decode(0xffff, &[]).is_err());
}
#[test]
fn inflated_counts_and_unterminated_comment_are_rejected() {
    let mut player = PLAYER.to_vec();
    player[5] = 255;
    assert!(decode(SMSG_LFG_UPDATE_PLAYER, &player).is_err());
    let mut roles = ROLES.to_vec();
    roles[10] = 255;
    assert!(decode(SMSG_LFG_ROLE_CHECK_UPDATE, &roles).is_err());
    let mut join = JOIN.to_vec();
    join[17..21].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(SMSG_LFG_JOIN_RESULT, &join).is_err());
    let mut player = PLAYER.to_vec();
    *player.last_mut().unwrap() = b'x';
    assert!(decode(SMSG_LFG_UPDATE_PLAYER, &player).is_err());
}
