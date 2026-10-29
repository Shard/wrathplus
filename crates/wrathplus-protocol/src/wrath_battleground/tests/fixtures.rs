use super::super::*;

pub(super) fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

pub(super) fn q(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

pub(super) fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}

pub(super) fn status_head(status: u32) -> Vec<u8> {
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

pub(super) fn status(status: u32, tail: StatusTail) -> Packet {
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

pub(super) fn rewards() -> Rewards {
    Rewards {
        has_win: 1,
        win_honor: 2,
        win_arena: 3,
        loss_honor: 4,
    }
}

pub(super) fn reward_bytes() -> Vec<u8> {
    cat(&[vec![1], w(2), w(3), w(4)])
}

pub(super) fn fixtures() -> Vec<(u16, Vec<u8>, SessionEvent)> {
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
