//! Build-12340 faction remainders. Wire reference: AzerothCore
//! 7f12e89ee5f467a50e62eba1d525eac7dc953d03 ReputationMgr.cpp (`SendForceReactions`,
//! `SendVisible`) and CharacterHandler.cpp (`HandleSetFactionAtWar`, `HandleSetFactionInactiveOpcode`,
//! `HandleSetWatchedFactionOpcode`). Strict framing.
//!
//! A faction becoming visible means what 1.12's does and feeds the shared event; forced reactions
//! have no shared event and are capture-only. The initial list and standing updates keep their
//! existing decoders.
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 2] = [0x0123, 0x02a5];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_SET_FORCED_REACTIONS`: (Faction.dbc id, reputation rank) the server forces.
    ForcedReactions { reactions: Vec<(u32, u32)> },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let event = match opcode {
        0x0123 => SessionEvent::ReputationVisible { list_id: u32_(r)? },
        0x02a5 => {
            let count = u32_(r)? as usize;
            if count > body.len() / 8 {
                return Err(invalid("forced reaction count"));
            }
            SessionEvent::WrathFaction {
                packet: Packet::ForcedReactions {
                    reactions: (0..count)
                        .map(|_| Ok((u32_(r)?, u32_(r)?)))
                        .collect::<io::Result<_>>()?,
                },
            }
        }
        _ => return Err(invalid("faction opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("faction packet extent"));
    }
    Ok(event)
}

/// `CMSG_SET_FACTION_ATWAR` (list id, flag), `CMSG_SET_FACTION_INACTIVE` (list id, flag),
/// `CMSG_SET_WATCHED_FACTION` (list id, -1 none).
pub fn encode_at_war(list_id: u32, at_war: bool) -> (u32, Vec<u8>) {
    let mut body = list_id.to_le_bytes().to_vec();
    body.push(u8::from(at_war));
    (0x0125, body)
}

pub fn encode_inactive(list_id: u32, inactive: bool) -> (u32, Vec<u8>) {
    let mut body = list_id.to_le_bytes().to_vec();
    body.push(u8::from(inactive));
    (0x0317, body)
}

pub fn encode_watched(list_id: i32) -> (u32, Vec<u8>) {
    (0x0318, list_id.to_le_bytes().to_vec())
}

#[cfg(test)]
mod tests;
