use super::*;

fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn q(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}

fn status_head(status: u32) -> Vec<u8> {
    cat(&[
        w(1),
        vec![0, 0],
        w(2),
        0x1f90u16.to_le_bytes().to_vec(),
        vec![10, 19],
        w(7),
        vec![0],
        w(status),
    ])
}

fn status(status: u32, tail: StatusTail) -> Packet {
    Packet::Status {
        slot: 1,
        status: Some(Status {
            arena_type: 0,
            raw_arena_flag: 0,
            bg_type: 2,
            raw_word: 0x1f90,
            min_level: 10,
            max_level: 19,
            client_instance: 7,
            rated: 0,
            status,
            tail,
        }),
    }
}

fn rewards() -> Rewards {
    Rewards {
        has_win: 1,
        win_honor: 2,
        win_arena: 3,
        loss_honor: 4,
    }
}

fn reward_bytes() -> Vec<u8> {
    cat(&[vec![1], w(2), w(3), w(4)])
}

fn fixtures() -> Vec<(u16, Vec<u8>, SessionEvent)> {
    let c = |packet| SessionEvent::WrathBattleground { packet };
    let bg_row = cat(&[q(9), w(1), w(2), w(3), w(4), w(5), w(6), w(2), w(7), w(8)]);
    let arena_row = cat(&[q(9), w(1), vec![1], w(5), w(6), w(0)]);
    vec![
        (
            0x2d4,
            cat(&[w(1), q(0)]),
            c(Packet::Status {
                slot: 1,
                status: None,
            }),
        ),
        (
            0x2d4,
            cat(&[status_head(1), w(60_000), w(5_000)]),
            c(status(
                1,
                StatusTail::Queued {
                    average_wait_ms: 60_000,
                    waited_ms: 5_000,
                },
            )),
        ),
        (
            0x2d4,
            cat(&[status_head(2), w(489), q(0), w(80_000)]),
            c(status(
                2,
                StatusTail::Invited {
                    map: 489,
                    raw_unknown: 0,
                    expires_ms: 80_000,
                },
            )),
        ),
        (
            0x2d4,
            cat(&[status_head(3), w(489), q(0), w(0), w(9), vec![1]]),
            c(status(
                3,
                StatusTail::InProgress {
                    map: 489,
                    raw_unknown: 0,
                    auto_leave_ms: 0,
                    elapsed_ms: 9,
                    arena_faction: 1,
                },
            )),
        ),
        (
            0x23d,
            cat(&[
                q(5),
                vec![0],
                w(2),
                vec![0, 0],
                reward_bytes(),
                vec![0],
                w(2),
                w(1),
                w(3),
            ]),
            c(Packet::List(BattlefieldList {
                battlemaster: 5,
                from_where: 0,
                bg_type: 2,
                raw_bytes: [0, 0],
                rewards: rewards(),
                random_rewards: None,
                instances: vec![1, 3],
            })),
        ),
        (
            0x23d,
            cat(&[
                q(5),
                vec![1],
                w(32),
                vec![0, 0],
                reward_bytes(),
                vec![1],
                reward_bytes(),
                w(0),
            ]),
            c(Packet::List(BattlefieldList {
                battlemaster: 5,
                from_where: 1,
                bg_type: 32,
                raw_bytes: [0, 0],
                rewards: rewards(),
                random_rewards: Some(rewards()),
                instances: vec![],
            })),
        ),
        (
            0x2e0,
            cat(&[vec![0], vec![1, 1], w(1), bg_row]),
            c(Packet::PvpLog(PvpLog {
                arena: None,
                winner: Some(1),
                scores: vec![Score {
                    guid: 9,
                    killing_blows: 1,
                    kind: ScoreKind::Battleground {
                        honorable_kills: 2,
                        deaths: 3,
                        bonus_honor: 4,
                    },
                    damage: 5,
                    healing: 6,
                    objectives: vec![7, 8],
                }],
            })),
        ),
        (
            0x2e0,
            cat(&[
                vec![1],
                w(0),
                w(12),
                w(1500),
                w(3),
                w(0),
                w(1400),
                b"Gold\0Blue\0".to_vec(),
                vec![0],
                w(1),
                arena_row,
            ]),
            c(Packet::PvpLog(PvpLog {
                arena: Some(ArenaTeams {
                    ratings: [(0, 12, 1500), (3, 0, 1400)],
                    names: ["Gold".into(), "Blue".into()],
                }),
                winner: None,
                scores: vec![Score {
                    guid: 9,
                    killing_blows: 1,
                    kind: ScoreKind::Arena { team: 1 },
                    damage: 5,
                    healing: 6,
                    objectives: vec![],
                }],
            })),
        ),
        (
            0x2d6,
            cat(&[q(9), vec![3], w(1), w(2), w(3), w(4)]),
            c(Packet::InspectHonor {
                guid: 9,
                raw_points: 3,
                kills: 1,
                today_honor: 2,
                yesterday_honor: 3,
                lifetime_kills: 4,
            }),
        ),
        (
            0x2e8,
            w(u32::MAX - 1),
            c(Packet::GroupJoined {
                result: -2,
                guid: None,
            }),
        ),
        (
            0x2e8,
            cat(&[w(0xfffffff8), q(0)]),
            c(Packet::GroupJoined {
                result: -8,
                guid: Some(0),
            }),
        ),
        (
            0x2e9,
            cat(&[
                w(0),
                w(1),
                q(9),
                1.5f32.to_le_bytes().to_vec(),
                2.5f32.to_le_bytes().to_vec(),
            ]),
            c(Packet::Positions {
                players: vec![],
                carriers: vec![(9, 1.5, 2.5)],
            }),
        ),
        (
            0x349,
            cat(&[w(0), b"Team\0Bob\0".to_vec(), w(3)]),
            c(Packet::ArenaCommandResult {
                action: 0,
                team: "Team".into(),
                player: "Bob".into(),
                error: 3,
            }),
        ),
        (
            0x357,
            cat(&[vec![3, 2], b"Bob\0Team\0".to_vec(), q(9)]),
            c(Packet::ArenaEvent {
                event: 3,
                strings: vec!["Bob".into(), "Team".into()],
                guid: Some(9),
            }),
        ),
        (
            0x357,
            vec![7, 0],
            c(Packet::ArenaEvent {
                event: 7,
                strings: vec![],
                guid: None,
            }),
        ),
        (
            0x376,
            cat(&[w(0), vec![2]]),
            c(Packet::ArenaError {
                raw_word: 0,
                team_type: Some(2),
            }),
        ),
        (
            0x377,
            cat(&[q(9), vec![1], w(5), w(1500), w(10), w(6), w(8), w(1490)]),
            c(Packet::InspectArenaTeam {
                guid: 9,
                slot: 1,
                team_id: 5,
                rating: 1500,
                season_games: 10,
                season_wins: 6,
                member_games: 8,
                personal_rating: 1490,
            }),
        ),
        (0x4c7, q(9), c(Packet::ArenaUnitDestroyed { guid: 9 })),
        (
            0x4de,
            cat(&[w(1), w(4197), w(1_700_000_000)]),
            c(Packet::BfEntryInvite {
                battle_id: 1,
                zone: 4197,
                deadline: 1_700_000_000,
            }),
        ),
        (
            0x4e0,
            cat(&[w(1), vec![1, 1, 0]]),
            c(Packet::BfEntered {
                battle_id: 1,
                raw_bytes: [1, 1],
                clear_afk: 0,
            }),
        ),
        (
            0x4e1,
            cat(&[w(1), vec![1]]),
            c(Packet::BfQueueInvite {
                battle_id: 1,
                raw_warmup: 1,
            }),
        ),
        (
            0x4e4,
            cat(&[w(1), w(4197), vec![1, 1, 1]]),
            c(Packet::BfQueueResponse {
                battle_id: 1,
                zone: 4197,
                accepted: 1,
                logging_in: 1,
                warmup: 1,
            }),
        ),
        (
            0x4e6,
            cat(&[w(1), vec![8, 2, 0]]),
            c(Packet::BfEjected {
                battle_id: 1,
                reason: 8,
                battle_status: 2,
                relocated: 0,
            }),
        ),
        (
            0x2ec,
            q(9),
            SessionEvent::BattlegroundPlayer {
                guid: 9,
                joined: true,
            },
        ),
        (
            0x2ed,
            q(9),
            SessionEvent::BattlegroundPlayer {
                guid: 9,
                joined: false,
            },
        ),
        (
            0x2e4,
            cat(&[q(9), w(30_000)]),
            SessionEvent::AreaSpiritHealerTime {
                healer: 9,
                ms: 30_000,
            },
        ),
        (
            0x28c,
            cat(&[w(14), q(9), w(5)]),
            SessionEvent::PvpCredit(PvpCredit {
                honor: 14,
                victim_guid: 9,
                victim_rank: 5,
            }),
        ),
        // The rank word is signed: all ones is the client's unranked form.
        (
            0x28c,
            cat(&[w(14), q(9), w(u32::MAX)]),
            SessionEvent::PvpCredit(PvpCredit {
                honor: 14,
                victim_guid: 9,
                victim_rank: -1,
            }),
        ),
    ]
}

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
