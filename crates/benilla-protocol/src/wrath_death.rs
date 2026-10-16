//! Build-12340 death, corpse and summon replies and commands.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: QueryHandler.cpp
//! (`HandleCorpseQueryOpcode`, `HandleCorpseMapPositionQuery`), Player.cpp
//! (`SendCorpseReclaimDelay`, `SMSG_DEATH_RELEASE_LOC`, `SMSG_PRE_RESURRECT`), SpellEffects.cpp
//! (`SMSG_SUMMON_REQUEST`), MiscHandler.cpp (`HandleRepopRequestOpcode`, `HandleReclaimCorpseOpcode`,
//! `HandleHearthAndResurrect`), MovementHandler.cpp (`HandleSummonResponseOpcode`), SpellHandler.cpp
//! (`HandleSelfResOpcode`), QueryPackets.cpp. Strict framing.
//!
//! The corpse query, reclaim delay and summon request mean what 1.12's do and feed the shared
//! events (the corpse reply's trailing word is framed and dropped). Release location,
//! pre-resurrect and the corpse map position have no shared event and are capture-only.
use crate::wire::read_packed_guid;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 6] = [0x0216, 0x0269, 0x02ab, 0x0378, 0x0494, 0x04b7];

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    /// `SMSG_DEATH_RELEASE_LOC`: the spirit healer's map and position; map -1 removes it.
    ReleaseLocation { map: i32, position: [f32; 3] },
    /// `SMSG_PRE_RESURRECT`: the packed GUID of the player about to be resurrected.
    PreResurrect { guid: u64 },
    /// `SMSG_CORPSE_MAP_POSITION_QUERY_RESPONSE`: AC answers four zero floats.
    CorpseMapPosition { raw: [f32; 4] },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn f32_(r: &mut Cursor<&[u8]>) -> io::Result<f32> {
    Ok(f32::from_bits(u32_(r)?))
}

fn u64_(r: &mut Cursor<&[u8]>) -> io::Result<u64> {
    let mut b = [0; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let capture = |packet| SessionEvent::WrathDeath { packet };
    let event = match opcode {
        0x0216 => {
            let mut found = [0; 1];
            r.read_exact(&mut found)?;
            match found[0] {
                0 => SessionEvent::CorpseQuery {
                    found: false,
                    display_map: 0,
                    position: [0.0; 3],
                    corpse_map: 0,
                },
                1 => {
                    let display_map = u32_(r)? as i32;
                    let position = [f32_(r)?, f32_(r)?, f32_(r)?];
                    let corpse_map = u32_(r)?;
                    // AC's trailing unknown word, always 0.
                    u32_(r)?;
                    SessionEvent::CorpseQuery {
                        found: true,
                        display_map,
                        position,
                        corpse_map,
                    }
                }
                _ => return Err(invalid("corpse found flag")),
            }
        }
        0x0269 => SessionEvent::CorpseReclaimDelay { delay_ms: u32_(r)? },
        0x02ab => SessionEvent::SummonRequest {
            summoner: u64_(r)?,
            zone: u32_(r)?,
            delay_ms: u32_(r)?,
        },
        0x0378 => capture(Packet::ReleaseLocation {
            map: u32_(r)? as i32,
            position: [f32_(r)?, f32_(r)?, f32_(r)?],
        }),
        0x0494 => capture(Packet::PreResurrect {
            guid: read_packed_guid(r)?,
        }),
        0x04b7 => capture(Packet::CorpseMapPosition {
            raw: [f32_(r)?, f32_(r)?, f32_(r)?, f32_(r)?],
        }),
        _ => return Err(invalid("death opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("death packet extent"));
    }
    Ok(event)
}

/// Death and summon commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_REPOP_REQUEST`: AC skips one byte.
    Repop,
    /// `MSG_CORPSE_QUERY`: empty.
    CorpseQuery,
    /// `CMSG_RECLAIM_CORPSE`: the corpse GUID.
    ReclaimCorpse { corpse: u64 },
    /// `CMSG_SUMMON_RESPONSE`: the summoner and the answer.
    SummonResponse { summoner: u64, agree: bool },
    /// `CMSG_SELF_RES`: empty.
    SelfRes,
    /// `CMSG_HEARTH_AND_RESURRECT`: empty; Wintergrasp's leave-and-resurrect.
    HearthAndResurrect,
    /// `CMSG_CORPSE_MAP_POSITION_QUERY`: a word AC reads and ignores.
    CorpseMapPositionQuery { raw: u32 },
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    match *command {
        Command::Repop => (0x015a, vec![0]),
        Command::CorpseQuery => (0x0216, vec![]),
        Command::ReclaimCorpse { corpse } => (0x01d2, corpse.to_le_bytes().to_vec()),
        Command::SummonResponse { summoner, agree } => {
            let mut body = summoner.to_le_bytes().to_vec();
            body.push(u8::from(agree));
            (0x02ac, body)
        }
        Command::SelfRes => (0x02b3, vec![]),
        Command::HearthAndResurrect => (0x049c, vec![]),
        Command::CorpseMapPositionQuery { raw } => (0x04b6, raw.to_le_bytes().to_vec()),
    }
}

#[cfg(test)]
mod tests;
