//! Lossless 12340 core guild replies. AzerothCore Server/Packets/GuildPackets.cpp
//! writers and Guilds/Guild.h event tags; [`session_events`] maps them onto the shared guild
//! events where the meaning matches.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rank {
    pub flags: u32,
    pub money_limit: u32,
    pub tabs: [(u32, u32); 6],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub guid: u64,
    pub status: u8,
    pub name: Vec<u8>,
    pub rank: u32,
    pub level: u8,
    pub class: u8,
    pub gender: u8,
    pub area: u32,
    /// Raw IEEE-754 word; present exactly when status == 0.
    pub last_save_bits: Option<u32>,
    pub note: Vec<u8>,
    pub officer_note: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Query {
        guild_id: u32,
        name: Vec<u8>,
        ranks: [Vec<u8>; 10],
        emblem: [u32; 5],
        rank_count: u32,
    },
    Info {
        name: Vec<u8>,
        packed_date: u32,
        members: u32,
        accounts: u32,
    },
    Roster {
        motd: Vec<u8>,
        info: Vec<u8>,
        ranks: Vec<Rank>,
        members: Vec<Member>,
    },
    Invite {
        inviter: Vec<u8>,
        guild: Vec<u8>,
    },
    Event {
        event: u8,
        params: Vec<Vec<u8>>,
        guid: Option<u64>,
    },
    CommandResult {
        command: u32,
        name: Vec<u8>,
        result: u32,
    },
    /// `MSG_SAVE_GUILD_EMBLEM` from the server: the save's result (`PlayerSaveGuildEmblem`).
    EmblemResult {
        error: u32,
    },
    /// `MSG_TABARDVENDOR_ACTIVATE` from the server: open the tabard designer for this vendor.
    TabardVendor {
        vendor: u64,
    },
    /// `MSG_GUILD_EVENT_LOG_QUERY` from the server, newest first as AC writes it.
    EventLog {
        entries: Vec<EventLogEntry>,
    },
}

/// One guild event-log row (`GuildEventLogQueryResults::Write`): 1 invite, 2 join, 3 promote,
/// 4 demote, 5 uninvite, 6 leave (AC `GuildEventLogTypes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLogEntry {
    pub kind: u8,
    pub player: u64,
    /// Absent for a join or a leave.
    pub other: Option<u64>,
    /// The new rank of a promote or demote.
    pub rank: Option<u8>,
    /// Seconds since the event, at the send.
    pub seconds_ago: u32,
}

/// AC `GuildEventLogTypes` without a second player.
const LOG_JOIN: u8 = 2;
const LOG_LEAVE: u8 = 6;
const LOG_PROMOTE: u8 = 3;
const LOG_DEMOTE: u8 = 4;
fn string(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let b = u8_le(r)?;
        if b == 0 {
            return Ok(bytes);
        }
        bytes.push(b);
    }
}
/// Counts must fit at least the fixed-width portion of their rows before allocation.
fn count(r: &mut Cursor<&[u8]>, min_row: usize) -> io::Result<u32> {
    let n = u32_le(r)?;
    if u64::from(n) > (r.get_ref().len() - r.position() as usize) as u64 / min_row as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "guild count exceeds body",
        ));
    }
    Ok(n)
}
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        0x0055 => {
            let guild_id = u32_le(&mut r)?;
            let name = string(&mut r)?;
            let mut ranks = std::array::from_fn(|_| Vec::new());
            for rank in &mut ranks {
                *rank = string(&mut r)?;
            }
            let mut emblem = [0; 5];
            for word in &mut emblem {
                *word = u32_le(&mut r)?;
            }
            Packet::Query {
                guild_id,
                name,
                ranks,
                emblem,
                rank_count: u32_le(&mut r)?,
            }
        }
        0x0083 => Packet::Invite {
            inviter: string(&mut r)?,
            guild: string(&mut r)?,
        },
        0x0088 => Packet::Info {
            name: string(&mut r)?,
            packed_date: u32_le(&mut r)?,
            members: u32_le(&mut r)?,
            accounts: u32_le(&mut r)?,
        },
        0x008a => {
            let member_count = count(&mut r, 23)?;
            let motd = string(&mut r)?;
            let info = string(&mut r)?;
            let rank_count = count(&mut r, 56)?;
            let mut ranks = Vec::new();
            for _ in 0..rank_count {
                let flags = u32_le(&mut r)?;
                let money_limit = u32_le(&mut r)?;
                let mut tabs = [(0, 0); 6];
                for tab in &mut tabs {
                    *tab = (u32_le(&mut r)?, u32_le(&mut r)?);
                }
                ranks.push(Rank {
                    flags,
                    money_limit,
                    tabs,
                });
            }
            let mut members = Vec::new();
            for _ in 0..member_count {
                let guid = u64_le(&mut r)?;
                let status = u8_le(&mut r)?;
                let name = string(&mut r)?;
                let rank = u32_le(&mut r)?;
                let level = u8_le(&mut r)?;
                let class = u8_le(&mut r)?;
                let gender = u8_le(&mut r)?;
                let area = u32_le(&mut r)?;
                let last_save_bits = if status == 0 {
                    Some(u32_le(&mut r)?)
                } else {
                    None
                };
                let note = string(&mut r)?;
                let officer_note = string(&mut r)?;
                members.push(Member {
                    guid,
                    status,
                    name,
                    rank,
                    level,
                    class,
                    gender,
                    area,
                    last_save_bits,
                    note,
                    officer_note,
                });
            }
            Packet::Roster {
                motd,
                info,
                ranks,
                members,
            }
        }
        0x0092 => {
            let event = u8_le(&mut r)?;
            let n = u8_le(&mut r)?;
            let mut params = Vec::new();
            for _ in 0..n {
                params.push(string(&mut r)?);
            }
            let guid = if matches!(event, 3 | 4 | 12 | 13) {
                Some(u64_le(&mut r)?)
            } else {
                None
            };
            Packet::Event {
                event,
                params,
                guid,
            }
        }
        0x0093 => Packet::CommandResult {
            command: u32_le(&mut r)?,
            name: string(&mut r)?,
            result: u32_le(&mut r)?,
        },
        0x01f1 => Packet::EmblemResult {
            error: u32_le(&mut r)?,
        },
        0x01f2 => Packet::TabardVendor {
            vendor: u64_le(&mut r)?,
        },
        0x03ff => {
            let n = u8_le(&mut r)?;
            let mut entries = Vec::with_capacity(usize::from(n));
            for _ in 0..n {
                let kind = u8_le(&mut r)?;
                let player = u64_le(&mut r)?;
                let other = (kind != LOG_JOIN && kind != LOG_LEAVE)
                    .then(|| u64_le(&mut r))
                    .transpose()?;
                let rank = (kind == LOG_PROMOTE || kind == LOG_DEMOTE)
                    .then(|| u8_le(&mut r))
                    .transpose()?;
                entries.push(EventLogEntry {
                    kind,
                    player,
                    other,
                    rank,
                    seconds_ago: u32_le(&mut r)?,
                });
            }
            Packet::EventLog { entries }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported guild opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "guild trailing bytes",
        ));
    }
    Ok(packet)
}

pub const OPCODES: [u16; 9] = [
    0x0055, 0x0083, 0x0088, 0x008a, 0x0092, 0x0093, 0x01f1, 0x01f2, 0x03ff,
];

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The session events a reply becomes. Where 3.3.5 means what 1.12 did, the shared guild events
/// carry it, so the existing guild owners (roster, chat lines, tabard, invite) consume it; the
/// roster also stays raw (`WrathGuild`) for its per-rank bank words, as does the event log.
pub fn session_events(packet: Packet) -> Vec<crate::SessionEvent> {
    use crate::types::{
        GuildCommandResult, GuildEventNotice, GuildInfo, GuildQueryResponse, GuildRoster,
        GuildRosterMember,
    };
    use crate::SessionEvent as E;
    match packet {
        Packet::Query {
            guild_id,
            name,
            ranks,
            emblem,
            ..
        } => vec![E::GuildQueryResponse(GuildQueryResponse {
            guild_id,
            name: text(name),
            rank_names: ranks.map(text),
            emblem_style: emblem[0] as i32,
            emblem_color: emblem[1] as i32,
            border_style: emblem[2] as i32,
            border_color: emblem[3] as i32,
            background_color: emblem[4] as i32,
        })],
        Packet::Info {
            name,
            packed_date,
            members,
            accounts,
        } => vec![E::GuildInfo(GuildInfo {
            name: text(name),
            // `AppendPackedTime`: 0-based day and month, years since 2000.
            created_day: ((packed_date >> 14) & 0x3f) + 1,
            created_month: ((packed_date >> 20) & 0xf) + 1,
            created_year: ((packed_date >> 24) & 0x1f) + 2000,
            member_count: members,
            account_count: accounts,
        })],
        Packet::Roster {
            ref motd,
            ref info,
            ref ranks,
            ref members,
        } => {
            let shared = GuildRoster {
                motd: text(motd.clone()),
                info: text(info.clone()),
                rank_rights: ranks.iter().map(|r| r.flags).collect(),
                members: members
                    .iter()
                    .map(|m| GuildRosterMember {
                        guid: m.guid,
                        presence: m.status,
                        name: text(m.name.clone()),
                        rank_id: m.rank,
                        level: m.level,
                        class: m.class,
                        zone: m.area,
                        last_online_days: m.last_save_bits.map_or(0.0, f32::from_bits),
                        public_note: text(m.note.clone()),
                        officer_note: text(m.officer_note.clone()),
                    })
                    .collect(),
            };
            vec![E::GuildRoster(shared), E::WrathGuild { packet }]
        }
        Packet::Invite { inviter, guild } => vec![E::GuildInvite {
            inviter: text(inviter),
            guild: text(guild),
        }],
        Packet::Event {
            event,
            params,
            guid,
        } => vec![E::GuildEvent(GuildEventNotice {
            event,
            params: params.into_iter().map(text).collect(),
            guid,
        })],
        Packet::CommandResult {
            command,
            name,
            result,
        } => vec![E::GuildCommandResult(GuildCommandResult {
            command,
            name: text(name),
            result,
        })],
        Packet::EmblemResult { error } => vec![E::SaveGuildEmblemResult(error)],
        Packet::TabardVendor { vendor } => vec![E::TabardVendorActivate(vendor)],
        packet @ Packet::EventLog { .. } => vec![E::WrathGuild { packet }],
    }
}

#[cfg(test)]
#[path = "wrath_guild_tests.rs"]
mod tests;
