use super::*;

mod fixtures;
use fixtures::*;

#[test]
fn every_fixture_decodes_and_rejects_truncation_and_trailing_bytes() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    for (opcode, body, expected) in fixtures {
        assert_eq!(decode(opcode, &body).unwrap(), expected, "{opcode:#x}");
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
fn counts_beyond_the_body_and_unsent_opcodes_are_rejected() {
    assert!(decode(0x436, &w(&[u32::MAX])).is_err());
    assert!(decode(0x438, &w(&[3, 0])).is_err());
    // No AC sender: status alert and the invite-notes pair stay undecoded.
    for opcode in [0x442, 0x460, 0x461] {
        assert!(decode(opcode, &[]).is_err());
    }
}

#[test]
fn dispatch_routes_every_opcode_and_reports_malformed_as_unhandled() {
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures() {
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => assert_eq!(
                format!("{session_events:?}"),
                format!(
                    "{:?}",
                    vec![crate::SessionEvent::WrathCalendar { packet: expected }]
                )
            ),
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
    let rows = vec![
        (Command::GetCalendar, 0x429, vec![]),
        (Command::GetEvent { event_id: 10 }, 0x42a, q(10)),
        (
            Command::GuildFilter {
                min_level: 1,
                max_level: 80,
                min_rank: 9,
            },
            0x42b,
            w(&[1, 80, 9]),
        ),
        (Command::ArenaTeam { team_id: 4 }, 0x42c, w(&[4])),
        (
            Command::AddEvent {
                title: "Raid".into(),
                description: "D".into(),
                kind: 3,
                repeat: 0,
                max_invites: 100,
                dungeon: -1,
                packed_time: 1,
                raw_packed_time: 2,
                flags: 0,
                invites: Some(vec![AddInvitee {
                    guid: 9,
                    status: 1,
                    rank: 2,
                }]),
            },
            0x42d,
            [
                b"Raid\0D\0".to_vec(),
                vec![3, 0],
                w(&[100, u32::MAX, 1, 2, 0, 1]),
                P9.to_vec(),
                vec![1, 2],
            ]
            .concat(),
        ),
        (
            Command::AddEvent {
                title: String::new(),
                description: String::new(),
                kind: 3,
                repeat: 0,
                max_invites: 100,
                dungeon: -1,
                packed_time: 1,
                raw_packed_time: 2,
                flags: 0x40,
                invites: None,
            },
            0x42d,
            [vec![0, 0, 3, 0], w(&[100, u32::MAX, 1, 2, 0x40])].concat(),
        ),
        (
            Command::UpdateEvent {
                event_id: 10,
                invite_id: 11,
                title: "T".into(),
                description: "D".into(),
                kind: 3,
                repeat: 0,
                max_invites: 100,
                dungeon: -1,
                packed_time: 1,
                timezone_time: 2,
                flags: 0x40,
            },
            0x42e,
            [
                q(10),
                q(11),
                b"T\0D\0".to_vec(),
                vec![3, 0],
                w(&[100, u32::MAX, 1, 2, 0x40]),
            ]
            .concat(),
        ),
        (
            Command::RemoveEvent {
                event_id: 10,
                invite_id: 11,
                flags: 0,
            },
            0x42f,
            [q(10), q(11), w(&[0])].concat(),
        ),
        (
            Command::CopyEvent {
                event_id: 10,
                invite_id: 11,
                packed_time: 1,
            },
            0x430,
            [q(10), q(11), w(&[1])].concat(),
        ),
        (
            Command::Invite {
                event_id: 10,
                invite_id: 11,
                name: "Bob".into(),
                pre_invite: 0,
                guild_event: 1,
            },
            0x431,
            [q(10), q(11), b"Bob\0".to_vec(), vec![0, 1]].concat(),
        ),
        (
            Command::Rsvp {
                event_id: 10,
                invite_id: 11,
                status: 1,
            },
            0x432,
            [q(10), q(11), w(&[1])].concat(),
        ),
        (
            Command::RemoveInvite {
                invitee: 9,
                invite_id: 11,
                owner_invite_id: 12,
                event_id: 10,
            },
            0x433,
            [P9.to_vec(), q(11), q(12), q(10)].concat(),
        ),
        (
            Command::Status {
                invitee: 9,
                event_id: 10,
                invite_id: 11,
                owner_invite_id: 12,
                status: 2,
            },
            0x434,
            [P9.to_vec(), q(10), q(11), q(12), vec![2]].concat(),
        ),
        (
            Command::ModeratorStatus {
                invitee: 9,
                event_id: 10,
                invite_id: 11,
                owner_invite_id: 12,
                rank: 1,
            },
            0x435,
            [P9.to_vec(), q(10), q(11), q(12), vec![1]].concat(),
        ),
        (
            Command::Complain {
                event_id: 10,
                guid: 9,
            },
            0x446,
            [q(10), q(9)].concat(),
        ),
        (Command::GetNumPending, 0x447, vec![]),
        (
            Command::Signup {
                event_id: 10,
                tentative: 1,
            },
            0x4ba,
            [q(10), vec![1]].concat(),
        ),
    ];
    for (command, opcode, body) in rows {
        assert_eq!(encode(&command), (opcode, body), "{command:?}");
    }
}
