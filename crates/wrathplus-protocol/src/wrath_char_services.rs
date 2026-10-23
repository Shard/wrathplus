//! Build-12340 character-screen services: rename, customize, faction and race change, declined
//! names and the realm-split query, each a request on the pre-world session and its reply.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03 CharacterHandler.cpp
//! (`HandleCharRenameOpcode`, `HandleCharCustomize`, `HandleCharFactionOrRaceChange`,
//! `HandleSetPlayerDeclinedNames`, `SendCharRename`, `SendCharCustomize`, `SendCharFactionChange`,
//! `SendSetPlayerDeclinedNamesResult`) and MiscHandler.cpp (`HandleRealmSplitOpcode`). Strict
//! framing. Result codes are the raw `ResponseCodes` (0 `RESPONSE_SUCCESS`); no roster change is
//! inferred here.
use crate::wire::read_cstring;
use crate::wrath_world::{WrathWardenRequired, WrathWorldSession};
use anyhow::{bail, Context, Result};
use std::io::{self, Cursor, Read};

/// `MAX_DECLINED_NAME_CASES`.
pub const DECLINED_CASES: usize = 5;
/// The packets a reply may trail by, as for create/delete.
const MAX_REPLY_PACKETS: usize = 64;
const SMSG_WARDEN_DATA: u16 = 0x02e6;

/// The appearance a customize or faction/race change asks for, in the request's field order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appearance {
    pub gender: u8,
    pub skin: u8,
    pub hair_color: u8,
    pub hair_style: u8,
    pub facial_hair: u8,
    pub face: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_CHAR_RENAME`.
    Rename { guid: u64, name: String },
    /// `CMSG_CHAR_CUSTOMIZE`.
    Customize {
        guid: u64,
        name: String,
        appearance: Appearance,
    },
    /// `CMSG_CHAR_FACTION_CHANGE`, or `CMSG_CHAR_RACE_CHANGE` when `faction` is false.
    ChangeRace {
        faction: bool,
        guid: u64,
        name: String,
        appearance: Appearance,
        race: u8,
    },
    /// `CMSG_SET_PLAYER_DECLINED_NAMES`: the base name, then the five cases.
    DeclinedNames {
        guid: u64,
        name: String,
        declined: [String; DECLINED_CASES],
    },
    /// `CMSG_REALM_SPLIT`: a word echoed back.
    RealmSplit { raw: u32 },
}

/// The appearance a successful customize or faction/race change reports, in the reply's order
/// (which is not the request's: face before hair, facial hair last).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedAppearance {
    pub gender: u8,
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// `SMSG_CHAR_RENAME`: the GUID and new name on success.
    Rename {
        result: u8,
        renamed: Option<(u64, String)>,
    },
    /// `SMSG_CHAR_CUSTOMIZE`.
    Customize {
        result: u8,
        applied: Option<(u64, String, AppliedAppearance)>,
    },
    /// `SMSG_CHAR_FACTION_CHANGE`, the reply to both faction and race changes.
    ChangeRace {
        result: u8,
        applied: Option<(u64, String, AppliedAppearance, u8)>,
    },
    /// `SMSG_SET_PLAYER_DECLINED_NAMES_RESULT`: 0 success.
    DeclinedNames { result: u32, guid: u64 },
    /// `SMSG_REALM_SPLIT`: the echoed word, the split state, the split date.
    RealmSplit {
        raw: u32,
        state: u32,
        date: String,
    },
}

impl Reply {
    /// Whether the server applied the change, so a refreshed roster is owed before the result.
    pub fn changed_roster(&self) -> bool {
        match self {
            Reply::Rename { result, .. }
            | Reply::Customize { result, .. }
            | Reply::ChangeRace { result, .. } => *result == 0,
            Reply::DeclinedNames { .. } | Reply::RealmSplit { .. } => false,
        }
    }
}

fn cstring(text: &str, body: &mut Vec<u8>) {
    body.extend(text.split('\0').next().unwrap_or("").as_bytes());
    body.push(0);
}

/// The request's opcode, body and the reply opcode it awaits.
pub fn encode(command: &Command) -> (u32, Vec<u8>, u16) {
    let mut b = Vec::new();
    let appearance = |a: &Appearance, b: &mut Vec<u8>| {
        b.extend([a.gender, a.skin, a.hair_color, a.hair_style, a.facial_hair, a.face]);
    };
    match command {
        Command::Rename { guid, name } => {
            b.extend(guid.to_le_bytes());
            cstring(name, &mut b);
            (0x02c7, b, 0x02c8)
        }
        Command::Customize {
            guid,
            name,
            appearance: a,
        } => {
            b.extend(guid.to_le_bytes());
            cstring(name, &mut b);
            appearance(a, &mut b);
            (0x0473, b, 0x0474)
        }
        Command::ChangeRace {
            faction,
            guid,
            name,
            appearance: a,
            race,
        } => {
            b.extend(guid.to_le_bytes());
            cstring(name, &mut b);
            appearance(a, &mut b);
            b.push(*race);
            (if *faction { 0x04d9 } else { 0x04f8 }, b, 0x04da)
        }
        Command::DeclinedNames {
            guid,
            name,
            declined,
        } => {
            b.extend(guid.to_le_bytes());
            cstring(name, &mut b);
            for case in declined {
                cstring(case, &mut b);
            }
            (0x0419, b, 0x041a)
        }
        Command::RealmSplit { raw } => {
            b.extend(raw.to_le_bytes());
            (0x038c, b, 0x038b)
        }
    }
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u8_(r: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
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

fn applied(r: &mut Cursor<&[u8]>) -> io::Result<AppliedAppearance> {
    Ok(AppliedAppearance {
        gender: u8_(r)?,
        skin: u8_(r)?,
        face: u8_(r)?,
        hair_style: u8_(r)?,
        hair_color: u8_(r)?,
        facial_hair: u8_(r)?,
    })
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Reply> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let reply = match opcode {
        0x02c8 => {
            let result = u8_(r)?;
            let renamed = if result == 0 {
                Some((u64_(r)?, read_cstring(r)?))
            } else {
                None
            };
            Reply::Rename { result, renamed }
        }
        0x0474 => {
            let result = u8_(r)?;
            let applied = if result == 0 {
                Some((u64_(r)?, read_cstring(r)?, applied(r)?))
            } else {
                None
            };
            Reply::Customize { result, applied }
        }
        0x04da => {
            let result = u8_(r)?;
            let applied = if result == 0 {
                Some((u64_(r)?, read_cstring(r)?, applied(r)?, u8_(r)?))
            } else {
                None
            };
            Reply::ChangeRace { result, applied }
        }
        0x041a => Reply::DeclinedNames {
            result: u32_(r)?,
            guid: u64_(r)?,
        },
        0x038b => Reply::RealmSplit {
            raw: u32_(r)?,
            state: u32_(r)?,
            date: read_cstring(r)?,
        },
        _ => return Err(invalid("character service opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("character service extent"));
    }
    Ok(reply)
}

impl WrathWorldSession {
    /// Send one character-screen service request and wait for its reply, as create/delete do.
    pub fn char_service_result(&mut self, command: &Command) -> Result<Reply> {
        let (opcode, body, reply) = encode(command);
        self.send_raw(opcode, &body)
            .with_context(|| format!("sending character service {opcode:#06x}"))?;
        for _ in 0..MAX_REPLY_PACKETS {
            let packet = self.recv_raw()?;
            match packet.opcode {
                op if op == reply => {
                    return decode(op, &packet.body).context("character service reply")
                }
                SMSG_WARDEN_DATA => return Err(WrathWardenRequired.into()),
                _ => {}
            }
        }
        bail!("no character service reply {reply:#x} within {MAX_REPLY_PACKETS} packets")
    }
}

#[cfg(test)]
mod tests;
