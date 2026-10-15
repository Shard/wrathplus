//! Lossless 12340 core guild replies. AzerothCore Server/Packets/GuildPackets.cpp
//! writers and Guilds/Guild.h event tags. No Vanilla projection or reducer policy.
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
}
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

#[cfg(test)]
#[path = "wrath_guild_tests.rs"]
mod tests;
