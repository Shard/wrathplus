use super::*;

fn fixtures() -> Vec<(u16, Vec<u8>, Packet)> {
    vec![
        (
            0x214,
            vec![0, 0, 0, 0, 0x81, 0x12, 0x34, 254],
            Packet::Encounter {
                kind: 0,
                guid: Some(0x3400000000000012),
                parameters: vec![254],
            },
        ),
        (
            0x214,
            vec![5, 0, 0, 0, 254, 253],
            Packet::Encounter {
                kind: 5,
                guid: None,
                parameters: vec![254, 253],
            },
        ),
        (
            0x214,
            vec![255; 4],
            Packet::Encounter {
                kind: u32::MAX,
                guid: None,
                parameters: vec![],
            },
        ),
        (
            0x147,
            vec![1, 2, 3, 4, 255, 255, 255, 255, 254],
            Packet::LockWarning {
                timeout_ms: 0x04030201,
                encounter_mask: u32::MAX,
                raw_flag: 254,
            },
        ),
        (
            0x2cb,
            vec![255; 4],
            Packet::SaveCreated {
                raw_value: u32::MAX,
            },
        ),
        (
            0x2fa,
            vec![
                4, 0, 0, 0, 1, 2, 3, 4, 255, 255, 255, 255, 0, 0, 0, 0, 254, 253,
            ],
            Packet::RaidMessage {
                kind: 4,
                map: 0x04030201,
                difficulty: u32::MAX,
                seconds: 0,
                welcome_flags: Some([254, 253]),
            },
        ),
        (
            0x2fa,
            vec![255; 16],
            Packet::RaidMessage {
                kind: u32::MAX,
                map: u32::MAX,
                difficulty: u32::MAX,
                seconds: u32::MAX,
                welcome_flags: None,
            },
        ),
        (0x31e, vec![1, 2, 3, 4], Packet::Reset { map: 0x04030201 }),
        (
            0x31f,
            vec![255, 255, 255, 255, 1, 2, 3, 4],
            Packet::ResetFailed {
                reason: u32::MAX,
                map: 0x04030201,
            },
        ),
        (0x320, vec![0; 4], Packet::LastInstance { map: 0 }),
        (
            0x329,
            vec![1, 2, 3, 4, 0, 0, 0, 0, 255, 255, 255, 255],
            Packet::DungeonDifficulty {
                difficulty: 0x04030201,
                unknown: 0,
                in_group: u32::MAX,
            },
        ),
        (
            0x32b,
            vec![255; 4],
            Packet::Ownership {
                raw_value: u32::MAX,
            },
        ),
        (
            0x33b,
            vec![255, 255, 255, 255, 1, 2, 3, 4],
            Packet::InstanceDifficulty {
                difficulty: u32::MAX,
                dynamic_heroic: 0x04030201,
            },
        ),
        (
            0x396,
            vec![1, 2, 3, 4],
            Packet::ResetFailedNotify { map: 0x04030201 },
        ),
        (
            0x4eb,
            vec![1, 2, 3, 4, 0, 0, 0, 0, 255, 255, 255, 255],
            Packet::RaidDifficulty {
                difficulty: 0x04030201,
                unknown: 0,
                in_group: u32::MAX,
            },
        ),
        (0x506, vec![], Packet::CorpseNotInInstance),
    ]
}

#[test]
fn independent_family_fixtures_preserve_raw_values() {
    for (opcode, body, expected) in fixtures() {
        assert_eq!(decode(opcode, &body).unwrap(), expected);
    }
}

#[test]
fn every_fixture_rejects_all_truncations_and_trailing_bytes() {
    for (opcode, body, _) in fixtures() {
        for len in 0..body.len() {
            assert!(decode(opcode, &body[..len]).is_err(), "{opcode:x}/{len}");
        }
        let mut trailing = body;
        trailing.push(0);
        assert!(decode(opcode, &trailing).is_err());
    }
    assert!(decode(0xffff, &[]).is_err());
    // Conditional welcome tail is required only for raw kind 4.
    assert!(decode(0x2fa, &[4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
}

#[test]
fn command_bodies_are_direction_specific_and_do_not_invent_authority() {
    for (command, expected) in [
        (Command::ResetInstances, (0x31d, vec![])),
        (
            Command::SetDungeonDifficulty { mode: u32::MAX },
            (0x329, vec![255; 4]),
        ),
        (
            Command::SetRaidDifficulty { mode: 0x04030201 },
            (0x4eb, vec![1, 2, 3, 4]),
        ),
        (
            Command::LockResponse { raw_accept: 254 },
            (0x13f, vec![254]),
        ),
        (
            Command::SetSavedInstanceExtend {
                map: 0x04030201,
                difficulty: u32::MAX,
                raw_extend: 253,
            },
            (0x292, vec![1, 2, 3, 4, 255, 255, 255, 255, 253]),
        ),
    ] {
        assert_eq!(encode(&command), expected);
    }
}

#[test]
fn encounter_discriminators_and_packed_guid_bounds() {
    for kind in 0..=9u32 {
        let mut body = kind.to_le_bytes().to_vec();
        match kind {
            0..=2 => body.extend([0, 255]),
            3 | 4 | 6 => body.push(255),
            5 => body.extend([255, 254]),
            _ => {}
        }
        assert!(decode(0x214, &body).is_ok());
    }
    for mask in 0..=255u8 {
        let mut body = vec![2, 0, 0, 0, mask];
        body.extend(vec![255; mask.count_ones() as usize]);
        body.push(254);
        assert!(decode(0x214, &body).is_ok());
        for len in 4..body.len() {
            assert!(decode(0x214, &body[..len]).is_err());
        }
    }
}
