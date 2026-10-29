use super::*;

mod fixtures;
use fixtures::*;

#[test]
fn every_fixture_decodes_and_rejects_truncation_and_trailing_bytes() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures {
        // `SessionEvent` has no `PartialEq`; its `Debug` form is total over the fields.
        assert_eq!(
            format!("{:?}", decode(opcode, &body).unwrap()),
            format!("{expected:?}"),
            "{opcode:#x}"
        );
        let trailing = [body.as_slice(), &[0]].concat();
        // A cleared status and a group-joined result are distinguished by length, so only their
        // other lengths must fail.
        if opcode != 0x2e8 {
            for end in 0..body.len() {
                // An arena event's GUID is optional: without it the strings alone are whole.
                if opcode == 0x357 && end + 8 == body.len() {
                    continue;
                }
                assert!(
                    decode(opcode, &body[..end]).is_err(),
                    "{opcode:#x} truncated at {end}"
                );
            }
        }
        assert!(decode(opcode, &trailing).is_err(), "{opcode:#x} trailing");
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(
                    format!("{session_events:?}"),
                    format!("{:?}", vec![expected])
                )
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
    }
}

#[test]
fn malformed_flags_counts_and_unsent_opcodes_are_rejected() {
    assert!(
        decode(0x2e0, &[2, 0, 0, 0, 0, 0]).is_err(),
        "score type is 0 or 1"
    );
    assert!(
        decode(0x2e0, &cat(&[vec![0, 2], w(0)])).is_err(),
        "ended flag is 0 or 1"
    );
    assert!(decode(0x2e0, &cat(&[vec![0, 0], w(u32::MAX)])).is_err());
    assert!(decode(0x357, &[1, 4]).is_err());
    assert!(
        decode(0x357, &[1, 0, 1, 2, 3]).is_err(),
        "only a full GUID may trail"
    );
    assert!(
        decode(0x2d4, &cat(&[w(1), q(1)])).is_err(),
        "a cleared slot is all zero"
    );
    // 0x222 is `wrath_world_objects`' (AC sends it from spell_generic.cpp); 0x170 and 0x14b have
    // no AC sender. None is this family's.
    for opcode in [0x222, 0x170, 0x14b] {
        assert!(
            decode(opcode, &q(9)).is_err(),
            "{opcode:#x} is not a battleground reply"
        );
    }
}

#[test]
fn commands_encode_ac_read_order() {
    for (command, opcode, body) in [
        (
            Command::List {
                bg_type: 2,
                from_where: 1,
                can_gain_xp: 1,
            },
            0x23c,
            cat(&[w(2), vec![1, 1]]),
        ),
        (Command::Status, 0x2d3, vec![]),
        (Command::BfQueueRequest { battle_id: 1 }, 0x4e3, w(1)),
        (
            Command::Join {
                battlemaster: 5,
                bg_type: 2,
                instance_id: 0,
                as_group: 1,
            },
            0x2ee,
            cat(&[q(5), w(2), w(0), vec![1]]),
        ),
        (
            Command::Port {
                arena_type: 0,
                raw_byte: 0,
                bg_type: 2,
                raw_word: 0x1f90,
                action: 1,
            },
            0x2d5,
            cat(&[vec![0, 0], w(2), 0x1f90u16.to_le_bytes().to_vec(), vec![1]]),
        ),
        (
            Command::JoinArena {
                battlemaster: 5,
                slot: 1,
                as_group: 1,
                rated: 1,
            },
            0x358,
            cat(&[q(5), vec![1, 1, 1]]),
        ),
        (Command::ReportAfk { player: 9 }, 0x3e4, q(9)),
        (Command::InspectArenaTeams { player: 9 }, 0x377, q(9)),
        (
            Command::BfEntryInviteResponse {
                battle_id: 1,
                accept: 1,
            },
            0x4df,
            cat(&[w(1), vec![1]]),
        ),
        (
            Command::BfQueueInviteResponse {
                battle_id: 1,
                accept: 0,
            },
            0x4e2,
            cat(&[w(1), vec![0]]),
        ),
        (Command::BfExit { battle_id: 1 }, 0x4e7, w(1)),
    ] {
        assert_eq!(encode(&command), (opcode, body), "{command:?}");
    }
    assert_eq!(encode_leave(), (0x2e1, vec![0; 8]));
    use crate::wrath_arena_team::Command as A;
    for (command, opcode, body) in [
        (A::Leave { team_id: 5 }, 0x353, w(5)),
        (A::Disband { team_id: 5 }, 0x355, w(5)),
        (
            A::Remove {
                team_id: 5,
                name: b"Bob".to_vec(),
            },
            0x354,
            cat(&[w(5), b"Bob\0".to_vec()]),
        ),
        (
            A::Leader {
                team_id: 5,
                name: b"Bob".to_vec(),
            },
            0x356,
            cat(&[w(5), b"Bob\0".to_vec()]),
        ),
    ] {
        assert_eq!(
            (command.opcode(), command.encode().unwrap()),
            (opcode, body)
        );
    }
}
