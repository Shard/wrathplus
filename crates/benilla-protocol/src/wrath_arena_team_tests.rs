use super::*;
fn word(b: &mut Vec<u8>, n: u32) {
    b.extend(n.to_le_bytes());
}
fn roster(flag: u8, count: u32) -> Vec<u8> {
    let mut b = 7u32.to_le_bytes().to_vec();
    b.push(flag);
    word(&mut b, count);
    word(&mut b, 255);
    for _ in 0..count {
        b.extend(u64::MAX.to_le_bytes());
        b.extend([254, 255, 0]);
        word(&mut b, u32::MAX);
        b.extend([80, 11]);
        for n in 0..5 {
            word(&mut b, 0x80000000 + n);
        }
        if flag != 0 {
            word(&mut b, 0x7fc01234);
            word(&mut b, 0x80000000);
        }
    }
    b
}
#[test]
fn roster_keeps_unknown_discriminator_optional_float_bits_raw_names_and_order() {
    for flag in [0, 1, 255] {
        for count in [0, 1, 12] {
            let Packet::Roster {
                team_id,
                discriminator,
                team_type,
                members,
            } = decode(0x34e, &roster(flag, count)).unwrap()
            else {
                panic!()
            };
            assert_eq!((team_id, discriminator, team_type), (7, flag, 255));
            assert_eq!(members.len(), count as usize);
            for m in members {
                assert_eq!(
                    m,
                    Member {
                        guid: u64::MAX,
                        online: 254,
                        name: vec![255],
                        captain_flag: u32::MAX,
                        level: 80,
                        class: 11,
                        week_games: 0x80000000,
                        week_wins: 0x80000001,
                        season_games: 0x80000002,
                        season_wins: 0x80000003,
                        personal_rating: 0x80000004,
                        extra_bits: if flag == 0 {
                            None
                        } else {
                            Some([0x7fc01234, 0x80000000])
                        }
                    }
                );
            }
        }
    }
    // Absolute minimum row, including an empty name.
    let mut b = roster(0, 1);
    b.remove(22);
    assert!(decode(0x34e, &b).is_ok());
}
#[test]
fn query_stats_and_invite_retain_native_words_and_bytes() {
    let mut b = 99u32.to_le_bytes().to_vec();
    b.extend([255, 0]);
    for n in 0..6 {
        word(&mut b, u32::MAX - n);
    }
    assert_eq!(
        decode(0x34c, &b).unwrap(),
        Packet::Query {
            team_id: 99,
            name: vec![255],
            team_type: u32::MAX,
            emblem: std::array::from_fn(|i| u32::MAX - 1 - i as u32)
        }
    );
    let mut b = Vec::new();
    for n in 0..7 {
        word(&mut b, 0x80000000 + n);
    }
    assert_eq!(
        decode(0x35b, &b).unwrap(),
        Packet::Stats {
            team_id: 0x80000000,
            rating: 0x80000001,
            week_games: 0x80000002,
            week_wins: 0x80000003,
            season_games: 0x80000004,
            season_wins: 0x80000005,
            rank: 0x80000006
        }
    );
    assert_eq!(
        decode(0x350, &[255, 0, 254, 0]).unwrap(),
        Packet::Invite {
            inviter: vec![255],
            team: vec![254]
        }
    );
}
#[test]
fn strict_truncation_tails_bad_counts_and_unknown_opcode() {
    for (op, b) in [
        (0x34c, vec![0; 29]),
        (0x34e, roster(0, 2)),
        (0x34e, roster(255, 2)),
        (0x350, b"name\0team\0".to_vec()),
        (0x35b, vec![255; 28]),
    ] {
        assert!(decode(op, &b).is_ok());
        for end in 0..b.len() {
            assert!(decode(op, &b[..end]).is_err(), "{op:x}/{end}");
        }
        let mut b = b;
        b.push(0);
        assert!(decode(op, &b).is_err());
    }
    let mut b = roster(0, 1);
    b[5..9].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(0x34e, &b).is_err());
    assert!(decode(0xffff, &[]).is_err());
}
#[test]
fn all_five_commands_encode_exactly_and_invites_reject_nuls() {
    for (c, op, b) in [
        (
            Command::Query {
                team_id: 0x01020304,
            },
            0x34b,
            vec![4, 3, 2, 1],
        ),
        (Command::Roster { team_id: u32::MAX }, 0x34d, vec![255; 4]),
        (
            Command::Invite {
                team_id: 0x01020304,
                name: vec![255],
            },
            0x34f,
            vec![4, 3, 2, 1, 255, 0],
        ),
        (Command::Accept, 0x351, vec![]),
        (Command::Decline, 0x352, vec![]),
    ] {
        assert_eq!(c.opcode(), op);
        assert_eq!(c.encode().unwrap(), b);
    }
    assert!(Command::Invite {
        team_id: 0,
        name: vec![0]
    }
    .encode()
    .is_err());
}
