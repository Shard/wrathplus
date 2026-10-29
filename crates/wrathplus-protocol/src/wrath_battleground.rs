//! Build-12340 battlegrounds, arenas and the battlefield manager (Wintergrasp).
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: BattlegroundMgr.cpp
//! (`BuildBattlegroundStatusPacket`, `BuildBattlegroundListPacket`, `BuildGroupJoined…`,
//! `BuildPlayer{Joined,Left}…`, `SendAreaSpiritHealerQueryOpcode`), Battleground.cpp
//! (`BuildPvPLogDataPacket`, `BattlegroundScore::AppendToPacket`), Arena.cpp (scores, rating and
//! team blocks), the zone `BuildObjectivesBlock`s, BattleGroundHandler.cpp (reads, positions),
//! ArenaTeamHandler.cpp, ArenaTeam.cpp (`BroadcastEvent`, `Inspect`), Object.cpp
//! (`SMSG_ARENA_UNIT_DESTROYED`), MiscHandler.cpp (honor inspect), Player.cpp (`SMSG_PVP_CREDIT`),
//! BattlefieldHandler.cpp.
//!
//! This build keys queues on the BattlemasterList bg type rather than 1.12's map, adds arena type,
//! level range, rated flag, random rewards and arena ratings, so status, list, score, honor and
//! arena replies are capture-only native [`Packet`]s. Player joined/left, spirit-healer time and PvP
//! credit keep 1.12's shapes and feed the shared events. Strict framing; raw words kept.
use crate::types::PvpCredit;
use crate::wire::read_cstring;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 20] = [
    0x023d, 0x028c, 0x02d4, 0x02d6, 0x02e0, 0x02e4, 0x02e8, 0x02e9, 0x02ec, 0x02ed, 0x0349, 0x0357,
    0x0376, 0x0377, 0x04c7, 0x04de, 0x04e0, 0x04e1, 0x04e4, 0x04e6,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusTail {
    /// `STATUS_WAIT_QUEUE`: average wait and time queued, ms.
    Queued {
        average_wait_ms: u32,
        waited_ms: u32,
    },
    /// `STATUS_WAIT_JOIN`: the invite's map and its expiry, ms.
    Invited {
        map: u32,
        raw_unknown: u64,
        expires_ms: u32,
    },
    /// `STATUS_IN_PROGRESS`: auto-leave and elapsed time, ms; arena faction 1 Alliance.
    InProgress {
        map: u32,
        raw_unknown: u64,
        auto_leave_ms: u32,
        elapsed_ms: u32,
        arena_faction: u8,
    },
    /// Any other status, whose tail AC never writes.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub arena_type: u8,
    /// 0x0E for an arena, else 0.
    pub raw_arena_flag: u8,
    /// BattlemasterList.dbc id.
    pub bg_type: u32,
    pub raw_word: u16,
    pub min_level: u8,
    pub max_level: u8,
    pub client_instance: u32,
    pub rated: u8,
    pub status: u32,
    pub tail: StatusTail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewards {
    pub has_win: u8,
    pub win_honor: u32,
    pub win_arena: u32,
    pub loss_honor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattlefieldList {
    pub battlemaster: u64,
    pub from_where: u8,
    pub bg_type: u32,
    pub raw_bytes: [u8; 2],
    pub rewards: Rewards,
    /// Present for the random battleground queue.
    pub random_rewards: Option<Rewards>,
    pub instances: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreKind {
    Battleground {
        honorable_kills: u32,
        deaths: u32,
        bonus_honor: u32,
    },
    Arena {
        team: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    pub guid: u64,
    pub killing_blows: u32,
    pub kind: ScoreKind,
    pub damage: u32,
    pub healing: u32,
    /// The battleground's own objective counters (flags, bases, towers, gates …).
    pub objectives: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArenaTeams {
    /// (rating lost, rating won, matchmaker rating) per team.
    pub ratings: [(u32, u32, u32); 2],
    pub names: [String; 2],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PvpLog {
    pub arena: Option<ArenaTeams>,
    /// The winner when the match has ended.
    pub winner: Option<u8>,
    pub scores: Vec<Score>,
}

/// Capture-only receipts, in arrival order.
#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    /// 02D4 for queue `slot`; `None` clears it.
    Status { slot: u32, status: Option<Status> },
    /// 023D.
    List(BattlefieldList),
    /// 02E0.
    PvpLog(PvpLog),
    /// 02D6.
    InspectHonor {
        guid: u64,
        raw_points: u8,
        kills: u32,
        today_honor: u32,
        yesterday_honor: u32,
        lifetime_kills: u32,
    },
    /// 02E8: `guid` only on a timeout or failure (AC writes zero).
    GroupJoined { result: i32, guid: Option<u64> },
    /// 02E9: tracked players, then flag carriers, each GUID and map x/y.
    Positions {
        players: Vec<(u64, f32, f32)>,
        carriers: Vec<(u64, f32, f32)>,
    },
    /// 0349.
    ArenaCommandResult {
        action: u32,
        team: String,
        player: String,
        error: u32,
    },
    /// 0357: up to three strings and the GUID AC appends when it has one.
    ArenaEvent {
        event: u8,
        strings: Vec<String>,
        guid: Option<u64>,
    },
    /// 0376: the team type follows when the leading word is 0.
    ArenaError {
        raw_word: u32,
        team_type: Option<u8>,
    },
    /// 0377.
    InspectArenaTeam {
        guid: u64,
        slot: u8,
        team_id: u32,
        rating: u32,
        season_games: u32,
        season_wins: u32,
        member_games: u32,
        personal_rating: u32,
    },
    /// 04C7.
    ArenaUnitDestroyed { guid: u64 },
    /// 04DE: accept by `deadline` (server time).
    BfEntryInvite {
        battle_id: u32,
        zone: u32,
        deadline: u32,
    },
    /// 04E0.
    BfEntered {
        battle_id: u32,
        raw_bytes: [u8; 2],
        clear_afk: u8,
    },
    /// 04E1.
    BfQueueInvite { battle_id: u32, raw_warmup: u8 },
    /// 04E4.
    BfQueueResponse {
        battle_id: u32,
        zone: u32,
        accepted: u8,
        logging_in: u8,
        warmup: u8,
    },
    /// 04E6.
    BfEjected {
        battle_id: u32,
        reason: u8,
        battle_status: u8,
        relocated: u8,
    },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u8_(r: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u16_(r: &mut Cursor<&[u8]>) -> io::Result<u16> {
    let mut b = [0; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn u64_(r: &mut Cursor<&[u8]>) -> io::Result<u64> {
    let mut b = [0; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn left(r: &Cursor<&[u8]>) -> usize {
    r.get_ref().len() - r.position() as usize
}

/// A wire count bounded by the bytes left at `min_row` bytes a row.
fn count(r: &mut Cursor<&[u8]>, min_row: usize) -> io::Result<u32> {
    let n = u32_(r)?;
    if n as usize > left(r) / min_row {
        return Err(invalid("battleground count"));
    }
    Ok(n)
}

fn rewards(r: &mut Cursor<&[u8]>) -> io::Result<Rewards> {
    Ok(Rewards {
        has_win: u8_(r)?,
        win_honor: u32_(r)?,
        win_arena: u32_(r)?,
        loss_honor: u32_(r)?,
    })
}

fn positions(r: &mut Cursor<&[u8]>) -> io::Result<Vec<(u64, f32, f32)>> {
    (0..count(r, 16)?)
        .map(|_| Ok((u64_(r)?, f32::from_bits(u32_(r)?), f32::from_bits(u32_(r)?))))
        .collect()
}

fn status(r: &mut Cursor<&[u8]>) -> io::Result<Status> {
    let (arena_type, raw_arena_flag, bg_type, raw_word) = (u8_(r)?, u8_(r)?, u32_(r)?, u16_(r)?);
    let (min_level, max_level, client_instance, rated) = (u8_(r)?, u8_(r)?, u32_(r)?, u8_(r)?);
    let status = u32_(r)?;
    let tail = match status {
        1 => StatusTail::Queued {
            average_wait_ms: u32_(r)?,
            waited_ms: u32_(r)?,
        },
        2 => StatusTail::Invited {
            map: u32_(r)?,
            raw_unknown: u64_(r)?,
            expires_ms: u32_(r)?,
        },
        3 => StatusTail::InProgress {
            map: u32_(r)?,
            raw_unknown: u64_(r)?,
            auto_leave_ms: u32_(r)?,
            elapsed_ms: u32_(r)?,
            arena_faction: u8_(r)?,
        },
        _ => StatusTail::Other,
    };
    Ok(Status {
        arena_type,
        raw_arena_flag,
        bg_type,
        raw_word,
        min_level,
        max_level,
        client_instance,
        rated,
        status,
        tail,
    })
}

fn pvp_log(r: &mut Cursor<&[u8]>) -> io::Result<PvpLog> {
    let arena = match u8_(r)? {
        0 => false,
        1 => true,
        _ => return Err(invalid("score log type")),
    };
    let arena = if arena {
        let mut ratings = [(0, 0, 0); 2];
        for rating in &mut ratings {
            *rating = (u32_(r)?, u32_(r)?, u32_(r)?);
        }
        Some(ArenaTeams {
            ratings,
            names: [read_cstring(r)?, read_cstring(r)?],
        })
    } else {
        None
    };
    let winner = match u8_(r)? {
        0 => None,
        1 => Some(u8_(r)?),
        _ => return Err(invalid("score log ended flag")),
    };
    // An arena row is the shortest: GUID, kills, team, damage, healing, objective count.
    let rows = count(r, 25)?;
    let scores = (0..rows)
        .map(|_| {
            let (guid, killing_blows) = (u64_(r)?, u32_(r)?);
            let kind = if arena.is_some() {
                ScoreKind::Arena { team: u8_(r)? }
            } else {
                ScoreKind::Battleground {
                    honorable_kills: u32_(r)?,
                    deaths: u32_(r)?,
                    bonus_honor: u32_(r)?,
                }
            };
            Ok(Score {
                guid,
                killing_blows,
                kind,
                damage: u32_(r)?,
                healing: u32_(r)?,
                objectives: (0..count(r, 4)?)
                    .map(|_| u32_(r))
                    .collect::<io::Result<_>>()?,
            })
        })
        .collect::<io::Result<_>>()?;
    Ok(PvpLog {
        arena,
        winner,
        scores,
    })
}

/// Decode one receive opcode of this family.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let capture = |packet| SessionEvent::WrathBattleground { packet };
    let event = match opcode {
        0x02d4 => {
            let slot = u32_(r)?;
            // A cleared slot is the slot and a zero word pair, 12 bytes in all.
            let status = if body.len() == 12 {
                if u64_(r)? != 0 {
                    return Err(invalid("cleared status"));
                }
                None
            } else {
                Some(status(r)?)
            };
            capture(Packet::Status { slot, status })
        }
        0x023d => {
            let (battlemaster, from_where, bg_type) = (u64_(r)?, u8_(r)?, u32_(r)?);
            let raw_bytes = [u8_(r)?, u8_(r)?];
            let first_rewards = rewards(r)?;
            let random_rewards = match u8_(r)? {
                0 => None,
                1 => Some(rewards(r)?),
                _ => return Err(invalid("random flag")),
            };
            let instances = (0..count(r, 4)?)
                .map(|_| u32_(r))
                .collect::<io::Result<_>>()?;
            capture(Packet::List(BattlefieldList {
                battlemaster,
                from_where,
                bg_type,
                raw_bytes,
                rewards: first_rewards,
                random_rewards,
                instances,
            }))
        }
        0x02e0 => capture(Packet::PvpLog(pvp_log(r)?)),
        0x02d6 => capture(Packet::InspectHonor {
            guid: u64_(r)?,
            raw_points: u8_(r)?,
            kills: u32_(r)?,
            today_honor: u32_(r)?,
            yesterday_honor: u32_(r)?,
            lifetime_kills: u32_(r)?,
        }),
        0x02e8 => {
            let result = u32_(r)? as i32;
            let guid = if left(r) == 8 { Some(u64_(r)?) } else { None };
            capture(Packet::GroupJoined { result, guid })
        }
        0x02e9 => capture(Packet::Positions {
            players: positions(r)?,
            carriers: positions(r)?,
        }),
        0x0349 => capture(Packet::ArenaCommandResult {
            action: u32_(r)?,
            team: read_cstring(r)?,
            player: read_cstring(r)?,
            error: u32_(r)?,
        }),
        0x0357 => {
            let (event, count) = (u8_(r)?, u8_(r)?);
            if count > 3 {
                return Err(invalid("arena event string count"));
            }
            let strings = (0..count)
                .map(|_| read_cstring(r))
                .collect::<io::Result<_>>()?;
            let guid = match left(r) {
                0 => None,
                8 => Some(u64_(r)?),
                _ => return Err(invalid("arena event tail")),
            };
            capture(Packet::ArenaEvent {
                event,
                strings,
                guid,
            })
        }
        0x0376 => {
            let raw_word = u32_(r)?;
            let team_type = if raw_word == 0 { Some(u8_(r)?) } else { None };
            capture(Packet::ArenaError {
                raw_word,
                team_type,
            })
        }
        0x0377 => capture(Packet::InspectArenaTeam {
            guid: u64_(r)?,
            slot: u8_(r)?,
            team_id: u32_(r)?,
            rating: u32_(r)?,
            season_games: u32_(r)?,
            season_wins: u32_(r)?,
            member_games: u32_(r)?,
            personal_rating: u32_(r)?,
        }),
        0x04c7 => capture(Packet::ArenaUnitDestroyed { guid: u64_(r)? }),
        0x04de => capture(Packet::BfEntryInvite {
            battle_id: u32_(r)?,
            zone: u32_(r)?,
            deadline: u32_(r)?,
        }),
        0x04e0 => capture(Packet::BfEntered {
            battle_id: u32_(r)?,
            raw_bytes: [u8_(r)?, u8_(r)?],
            clear_afk: u8_(r)?,
        }),
        0x04e1 => capture(Packet::BfQueueInvite {
            battle_id: u32_(r)?,
            raw_warmup: u8_(r)?,
        }),
        0x04e4 => capture(Packet::BfQueueResponse {
            battle_id: u32_(r)?,
            zone: u32_(r)?,
            accepted: u8_(r)?,
            logging_in: u8_(r)?,
            warmup: u8_(r)?,
        }),
        0x04e6 => capture(Packet::BfEjected {
            battle_id: u32_(r)?,
            reason: u8_(r)?,
            battle_status: u8_(r)?,
            relocated: u8_(r)?,
        }),
        0x02ec => SessionEvent::BattlegroundPlayer {
            guid: u64_(r)?,
            joined: true,
        },
        0x02ed => SessionEvent::BattlegroundPlayer {
            guid: u64_(r)?,
            joined: false,
        },
        0x02e4 => SessionEvent::AreaSpiritHealerTime {
            healer: u64_(r)?,
            ms: u32_(r)?,
        },
        0x028c => SessionEvent::PvpCredit(PvpCredit {
            honor: u32_(r)? as i32,
            victim_guid: u64_(r)?,
            victim_rank: u32_(r)? as i32,
        }),
        _ => return Err(invalid("battleground opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("battleground packet extent"));
    }
    Ok(event)
}

/// Battleground, arena and battlefield-manager commands with no shared producer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_BATTLEFIELD_LIST`: from 0 battlemaster, 1 the UI; can-gain-XP byte.
    List {
        bg_type: u32,
        from_where: u8,
        can_gain_xp: u8,
    },
    /// `CMSG_BATTLEMASTER_JOIN`: instance 0 is first available.
    Join {
        battlemaster: u64,
        bg_type: u32,
        instance_id: u32,
        as_group: u8,
    },
    /// `CMSG_BATTLEFIELD_PORT`: action 1 enters, 0 leaves the queue.
    Port {
        arena_type: u8,
        raw_byte: u8,
        bg_type: u32,
        raw_word: u16,
        action: u8,
    },
    /// `CMSG_BATTLEMASTER_JOIN_ARENA`: slot 0..2 for 2v2/3v3/5v5.
    JoinArena {
        battlemaster: u64,
        slot: u8,
        as_group: u8,
        rated: u8,
    },
    /// `CMSG_REPORT_PVP_AFK`.
    ReportAfk { player: u64 },
    /// `MSG_INSPECT_ARENA_TEAMS`.
    InspectArenaTeams { player: u64 },
    /// `CMSG_BATTLEFIELD_MGR_ENTRY_INVITE_RESPONSE`.
    BfEntryInviteResponse { battle_id: u32, accept: u8 },
    /// `CMSG_BATTLEFIELD_MGR_QUEUE_INVITE_RESPONSE`.
    BfQueueInviteResponse { battle_id: u32, accept: u8 },
    /// `CMSG_BATTLEFIELD_MGR_EXIT_REQUEST`.
    BfExit { battle_id: u32 },
    /// `CMSG_BATTLEFIELD_STATUS`, empty: the status of every queue (AC
    /// `HandleBattlefieldStatusOpcode`).
    Status,
    /// `CMSG_BATTLEFIELD_MGR_QUEUE_REQUEST` (`0x54d590`): a battle id. AC registers it
    /// `STATUS_NEVER`/`Handle_NULL`, so it is sent and ignored.
    BfQueueRequest { battle_id: u32 },
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let mut b = Vec::new();
    let opcode = match command {
        Command::List {
            bg_type,
            from_where,
            can_gain_xp,
        } => {
            b.extend(bg_type.to_le_bytes());
            b.extend([*from_where, *can_gain_xp]);
            0x023c
        }
        Command::Join {
            battlemaster,
            bg_type,
            instance_id,
            as_group,
        } => {
            b.extend(battlemaster.to_le_bytes());
            b.extend(bg_type.to_le_bytes());
            b.extend(instance_id.to_le_bytes());
            b.push(*as_group);
            0x02ee
        }
        Command::Port {
            arena_type,
            raw_byte,
            bg_type,
            raw_word,
            action,
        } => {
            b.extend([*arena_type, *raw_byte]);
            b.extend(bg_type.to_le_bytes());
            b.extend(raw_word.to_le_bytes());
            b.push(*action);
            0x02d5
        }
        Command::JoinArena {
            battlemaster,
            slot,
            as_group,
            rated,
        } => {
            b.extend(battlemaster.to_le_bytes());
            b.extend([*slot, *as_group, *rated]);
            0x0358
        }
        Command::Status => 0x02d3,
        Command::BfQueueRequest { battle_id } => {
            b.extend(battle_id.to_le_bytes());
            0x04e3
        }
        Command::ReportAfk { player } => {
            b.extend(player.to_le_bytes());
            0x03e4
        }
        Command::InspectArenaTeams { player } => {
            b.extend(player.to_le_bytes());
            0x0377
        }
        Command::BfEntryInviteResponse { battle_id, accept } => {
            b.extend(battle_id.to_le_bytes());
            b.push(*accept);
            0x04df
        }
        Command::BfQueueInviteResponse { battle_id, accept } => {
            b.extend(battle_id.to_le_bytes());
            b.push(*accept);
            0x04e2
        }
        Command::BfExit { battle_id } => {
            b.extend(battle_id.to_le_bytes());
            0x04e7
        }
    };
    (opcode, b)
}

/// `CMSG_LEAVE_BATTLEFIELD`: AC skips the whole body (two bytes, a bg type, a word) and leaves the
/// current battleground.
pub fn encode_leave() -> (u32, Vec<u8>) {
    (0x02e1, vec![0; 8])
}

#[cfg(test)]
mod tests;
