//! Build-12340 achievement receipts and the inspect query. Capture-only: no achievement state,
//! points, toasts or Lua events are inferred.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03 AchievementMgr.cpp
//! (`CompletedAchievement`, `SendAllAchievementData`, `SendRespondInspectAchievements`,
//! `BuildAllDataPacket`, `Reset`), MiscHandler.cpp (`HandleQueryInspectAchievements`).
//! `SMSG_CRITERIA_UPDATE` stays with `wrath_progress`. Strict framing; raw words preserved.
use crate::wire::{read_cstring, read_packed_guid, write_packed_guid};
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 6] = [0x0468, 0x046c, 0x047d, 0x0498, 0x049e, 0x049f];

/// `BuildAllDataPacket`'s list terminator, written as `int32(-1)`.
const END: u32 = u32::MAX;

/// One criteria progress row, as `SMSG_CRITERIA_UPDATE` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criteria {
    pub id: u32,
    /// Packed like a GUID on the wire.
    pub counter: u64,
    pub player: u64,
    pub raw_flags: u32,
    pub packed_time: u32,
    pub elapsed: u32,
    pub creation_elapsed: u32,
}

/// Completed achievements (id, packed date), then criteria progress, in wire order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub earned: Vec<(u32, u32)>,
    pub criteria: Vec<Criteria>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_ACHIEVEMENT_EARNED`, to everyone in say range, the earner included.
    Earned {
        player: u64,
        achievement: u32,
        packed_time: u32,
        raw_unknown: u32,
    },
    /// `SMSG_SERVER_FIRST_ACHIEVEMENT`: the guild or player name; `raw_link` 1 is a clickable name.
    ServerFirst {
        name: String,
        player: u64,
        achievement: u32,
        raw_link: u32,
    },
    /// `SMSG_ALL_ACHIEVEMENT_DATA`, at login.
    AllData(Snapshot),
    /// `SMSG_RESPOND_INSPECT_ACHIEVEMENTS`.
    Inspect { player: u64, data: Snapshot },
    /// `SMSG_ACHIEVEMENT_DELETED` (`AchievementMgr::Reset`).
    AchievementDeleted { achievement: u32 },
    /// `SMSG_CRITERIA_DELETED`.
    CriteriaDeleted { criteria: u32 },
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

fn snapshot(r: &mut Cursor<&[u8]>) -> io::Result<Snapshot> {
    let mut data = Snapshot::default();
    loop {
        let id = u32_(r)?;
        if id == END {
            break;
        }
        data.earned.push((id, u32_(r)?));
    }
    loop {
        let id = u32_(r)?;
        if id == END {
            break;
        }
        data.criteria.push(Criteria {
            id,
            counter: read_packed_guid(r)?,
            player: read_packed_guid(r)?,
            raw_flags: u32_(r)?,
            packed_time: u32_(r)?,
            elapsed: u32_(r)?,
            creation_elapsed: u32_(r)?,
        });
    }
    Ok(data)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let packet = match opcode {
        0x0468 => Packet::Earned {
            player: read_packed_guid(r)?,
            achievement: u32_(r)?,
            packed_time: u32_(r)?,
            raw_unknown: u32_(r)?,
        },
        0x0498 => Packet::ServerFirst {
            name: read_cstring(r)?,
            player: u64_(r)?,
            achievement: u32_(r)?,
            raw_link: u32_(r)?,
        },
        0x047d => Packet::AllData(snapshot(r)?),
        0x046c => Packet::Inspect {
            player: read_packed_guid(r)?,
            data: snapshot(r)?,
        },
        0x049f => Packet::AchievementDeleted {
            achievement: u32_(r)?,
        },
        0x049e => Packet::CriteriaDeleted { criteria: u32_(r)? },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "achievement opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "achievement packet extent",
        ));
    }
    Ok(packet)
}

/// `CMSG_QUERY_INSPECT_ACHIEVEMENTS` (0x046b): the inspected player's packed GUID.
pub fn encode_inspect_query(player: u64) -> (u32, Vec<u8>) {
    let mut body = Vec::new();
    write_packed_guid(player, &mut body).expect("writing to a Vec");
    (0x046b, body)
}

#[cfg(test)]
mod tests;
