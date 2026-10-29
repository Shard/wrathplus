use super::*;

fn w(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn q(value: u64) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

const P9: [u8; 2] = [0x01, 0x09]; // packed GUID 9

fn calendar_bytes() -> Vec<u8> {
    let mut b = w(&[1]);
    b.extend(q(10));
    b.extend(q(11));
    b.extend([1, 2, 0]);
    b.extend(P9);
    b.extend(w(&[1]));
    b.extend(q(10));
    b.extend(b"Raid\0");
    b.extend(w(&[3, 0x0a1b_2c3d, 0x40, u32::MAX]));
    b.extend(P9);
    b.extend(w(&[1_700_000_000, 0x0a1b_2c3d]));
    b.extend(w(&[1, 533, 1, 3600]));
    b.extend(q(0x1f40_0000_0000_0007));
    b.extend(w(&[1_135_753_200]));
    b.extend(w(&[1, 533, 604_800, 0]));
    b.extend(w(&[1, 141, 0, 1, 2, 3]));
    b.extend(w(&[7; HOLIDAY_DATES]));
    b.extend(w(&[8; HOLIDAY_DURATIONS]));
    b.extend(w(&[9; HOLIDAY_FLAGS]));
    b.extend(b"Calendar_Fishing\0");
    b
}

fn calendar() -> Calendar {
    Calendar {
        invites: vec![InviteRow {
            event_id: 10,
            invite_id: 11,
            status: 1,
            rank: 2,
            guild_event: 0,
            creator: 9,
        }],
        events: vec![EventRow {
            event_id: 10,
            title: "Raid".into(),
            kind: 3,
            packed_time: 0x0a1b_2c3d,
            flags: 0x40,
            dungeon: -1,
            creator: 9,
        }],
        server_time: 1_700_000_000,
        zone_packed_time: 0x0a1b_2c3d,
        lockouts: vec![Lockout {
            map: 533,
            difficulty: 1,
            seconds_left: 3600,
            instance: 0x1f40_0000_0000_0007,
        }],
        relation_time: 1_135_753_200,
        raid_resets: vec![RaidReset {
            map: 533,
            period: 604_800,
            offset: 0,
        }],
        holidays: vec![Holiday {
            id: 141,
            region: 0,
            looping: 1,
            priority: 2,
            filter_type: 3,
            dates: [7; HOLIDAY_DATES],
            durations: [8; HOLIDAY_DURATIONS],
            flags: [9; HOLIDAY_FLAGS],
            texture: "Calendar_Fishing".into(),
        }],
    }
}

fn empty_calendar_bytes() -> Vec<u8> {
    w(&[0, 0, 5, 6, 0, 7, 0, 0])
}

fn event_bytes() -> Vec<u8> {
    let mut b = vec![1];
    b.extend(P9);
    b.extend(q(10));
    b.extend(b"Raid\0Bring flasks\0");
    b.extend([3, 0]);
    b.extend(w(&[100, u32::MAX, 0x40, 0x0a1b_2c3d, 0, 5, 1]));
    b.extend(P9);
    b.extend([80, 1, 2, 1]);
    b.extend(q(11));
    b.extend(w(&[0x0a1b_2c3d]));
    b.extend(b"note\0");
    b
}

fn fixtures() -> Vec<(u16, Vec<u8>, Packet)> {
    let mut alert = q(10);
    alert.extend(b"Raid\0");
    alert.extend(w(&[0x0a1b_2c3d, 0x40, 3, u32::MAX]));
    alert.extend(q(11));
    alert.extend([1, 2]);
    alert.extend(P9);
    alert.extend([0x01, 0x08]);
    let mut updated = vec![1];
    updated.extend(q(10));
    updated.extend(w(&[1, 0x40, 2]));
    updated.push(3);
    updated.extend(w(&[u32::MAX]));
    updated.extend(b"T\0D\0");
    updated.push(0);
    updated.extend(w(&[100, 0]));
    vec![
        (0x436, calendar_bytes(), Packet::Calendar(calendar())),
        (
            0x436,
            empty_calendar_bytes(),
            Packet::Calendar(Calendar {
                invites: vec![],
                events: vec![],
                server_time: 5,
                zone_packed_time: 6,
                lockouts: vec![],
                relation_time: 7,
                raid_resets: vec![],
                holidays: vec![],
            }),
        ),
        (
            0x437,
            event_bytes(),
            Packet::Event(Event {
                send_type: 1,
                creator: 9,
                event_id: 10,
                title: "Raid".into(),
                description: "Bring flasks".into(),
                kind: 3,
                repeat: 0,
                max_invites: 100,
                dungeon: -1,
                flags: 0x40,
                packed_time: 0x0a1b_2c3d,
                timezone_time: 0,
                guild_id: 5,
                invitees: vec![EventInvitee {
                    guid: 9,
                    level: 80,
                    status: 1,
                    rank: 2,
                    guild_member: 1,
                    invite_id: 11,
                    status_time: 0x0a1b_2c3d,
                    text: "note".into(),
                }],
            }),
        ),
        (
            0x438,
            [w(&[2]), P9.to_vec(), vec![0, 0x01, 0x08, 0]].concat(),
            Packet::FilterGuild {
                members: vec![(9, 0), (8, 0)],
            },
        ),
        (0x439, w(&[0]), Packet::ArenaTeam { members: vec![] }),
        (
            0x43a,
            [
                P9.to_vec(),
                q(10),
                q(11),
                vec![80, 1, 1],
                w(&[0x0a1b_2c3d]),
                vec![1],
            ]
            .concat(),
            Packet::Invite {
                invitee: 9,
                event_id: 10,
                invite_id: 11,
                level: 80,
                status: 1,
                status_time: Some(0x0a1b_2c3d),
                raw_not_signup: 1,
            },
        ),
        (
            0x43a,
            [P9.to_vec(), q(10), q(11), vec![80, 1, 0, 0]].concat(),
            Packet::Invite {
                invitee: 9,
                event_id: 10,
                invite_id: 11,
                level: 80,
                status: 1,
                status_time: None,
                raw_not_signup: 0,
            },
        ),
        (
            0x43b,
            [P9.to_vec(), q(10), w(&[4]), vec![1]].concat(),
            Packet::InviteRemoved {
                invitee: 9,
                event_id: 10,
                flags: 4,
                raw_byte: 1,
            },
        ),
        (
            0x43c,
            [P9.to_vec(), q(10), w(&[1, 0x40]), vec![2, 1], w(&[3])].concat(),
            Packet::Status {
                invitee: 9,
                event_id: 10,
                packed_time: 1,
                flags: 0x40,
                status: 2,
                rank: 1,
                status_time: 3,
            },
        ),
        (
            0x43d,
            [w(&[0]), vec![0], b"Bob\0".to_vec(), w(&[23])].concat(),
            Packet::CommandResult {
                raw_word: 0,
                raw_byte: 0,
                param: "Bob".into(),
                error: 23,
            },
        ),
        (
            0x43d,
            [w(&[0]), vec![0, 0], w(&[1])].concat(),
            Packet::CommandResult {
                raw_word: 0,
                raw_byte: 0,
                param: String::new(),
                error: 1,
            },
        ),
        (
            0x43e,
            [w(&[1, 533, 1, 3600]), q(7)].concat(),
            Packet::RaidLockoutAdded {
                packed_time: 1,
                lockout: Lockout {
                    map: 533,
                    difficulty: 1,
                    seconds_left: 3600,
                    instance: 7,
                },
            },
        ),
        (
            0x43f,
            [w(&[533, 1, 0]), q(7)].concat(),
            Packet::RaidLockoutRemoved {
                lockout: Lockout {
                    map: 533,
                    difficulty: 1,
                    seconds_left: 0,
                    instance: 7,
                },
            },
        ),
        (
            0x471,
            w(&[1, 533, 1, 60, 120]),
            Packet::RaidLockoutUpdated {
                packed_time: 1,
                map: 533,
                difficulty: 1,
                old_seconds: 60,
                new_seconds: 120,
            },
        ),
        (
            0x440,
            alert,
            Packet::InviteAlert {
                event_id: 10,
                title: "Raid".into(),
                packed_time: 0x0a1b_2c3d,
                flags: 0x40,
                kind: 3,
                dungeon: -1,
                invite_id: 11,
                status: 1,
                rank: 2,
                creator: 9,
                sender: 8,
            },
        ),
        (
            0x441,
            [q(10), w(&[1, 0x40]), vec![3]].concat(),
            Packet::InviteRemovedAlert {
                event_id: 10,
                packed_time: 1,
                flags: 0x40,
                status: 3,
            },
        ),
        (
            0x443,
            [vec![1], q(10), w(&[1])].concat(),
            Packet::RemovedAlert {
                raw_byte: 1,
                event_id: 10,
                packed_time: 1,
            },
        ),
        (
            0x444,
            updated,
            Packet::UpdatedAlert {
                raw_byte: 1,
                event_id: 10,
                old_packed_time: 1,
                flags: 0x40,
                packed_time: 2,
                kind: 3,
                dungeon: -1,
                title: "T".into(),
                description: "D".into(),
                repeat: 0,
                max_invites: 100,
                raw_unknown: 0,
            },
        ),
        (
            0x445,
            [P9.to_vec(), q(10), vec![2, 1]].concat(),
            Packet::ModeratorStatusAlert {
                invitee: 9,
                event_id: 10,
                rank: 2,
                raw_byte: 1,
            },
        ),
        (0x448, w(&[3]), Packet::NumPending { count: 3 }),
        (0x4bb, vec![], Packet::ClearPendingAction),
    ]
}

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
