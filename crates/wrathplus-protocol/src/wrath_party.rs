//! Build-12340 party remainders: leader change, disband, minimap ping, random roll and the raid
//! homebind warning, translated into the shared events their feeds own, plus the party commands
//! with no native writer yet.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: Group.cpp
//! (`ChangeLeader`, `Disband`), MiscPackets.cpp (`MinimapPing`, `RandomRoll`), PlayerUpdates.cpp
//! (`SMSG_RAID_GROUP_ONLY`), GroupHandler.cpp. Strict framing.
use crate::wire::read_cstring;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 5] = [0x0079, 0x007c, 0x01d5, 0x01fb, 0x0286];

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

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let event = match opcode {
        0x0079 => SessionEvent::GroupLeaderChanged {
            name: read_cstring(r)?,
        },
        0x007c => SessionEvent::GroupDestroyed,
        0x01d5 => SessionEvent::MinimapPing {
            guid: u64_(r)?,
            x: f32::from_bits(u32_(r)?),
            y: f32::from_bits(u32_(r)?),
        },
        0x01fb => SessionEvent::RandomRoll {
            min: u32_(r)?,
            max: u32_(r)?,
            roll: u32_(r)?,
            guid: u64_(r)?,
        },
        0x0286 => SessionEvent::RaidGroupOnly {
            delay_ms: u32_(r)?,
            reason: u32_(r)?,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "party opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "party packet extent",
        ));
    }
    Ok(event)
}

/// Party commands.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `CMSG_GROUP_UNINVITE`: by name.
    Uninvite { name: String },
    /// `CMSG_GROUP_UNINVITE_GUID`: GUID and a kick reason (shown for LFG boots).
    UninviteGuid { guid: u64, reason: String },
    /// `CMSG_GROUP_SET_LEADER`.
    SetLeader { guid: u64 },
    /// `MSG_MINIMAP_PING`: map coordinates.
    MinimapPing { x: f32, y: f32 },
    /// `MSG_RANDOM_ROLL`.
    RandomRoll { min: u32, max: u32 },
    /// `MSG_RAID_READY_CHECK_FINISHED`: empty.
    ReadyCheckFinished,
}

fn cstring(text: &str, body: &mut Vec<u8>) {
    body.extend(text.split('\0').next().unwrap_or("").as_bytes());
    body.push(0);
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let mut body = Vec::new();
    let opcode = match command {
        Command::Uninvite { name } => {
            cstring(name, &mut body);
            0x0075
        }
        Command::UninviteGuid { guid, reason } => {
            body.extend(guid.to_le_bytes());
            cstring(reason, &mut body);
            0x0076
        }
        Command::SetLeader { guid } => {
            body.extend(guid.to_le_bytes());
            0x0078
        }
        Command::MinimapPing { x, y } => {
            body.extend(x.to_le_bytes());
            body.extend(y.to_le_bytes());
            0x01d5
        }
        Command::RandomRoll { min, max } => {
            body.extend(min.to_le_bytes());
            body.extend(max.to_le_bytes());
            0x01fb
        }
        Command::ReadyCheckFinished => 0x03c6,
    };
    (opcode, body)
}

#[cfg(test)]
mod tests;
