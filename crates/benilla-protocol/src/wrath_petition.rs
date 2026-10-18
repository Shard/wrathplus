//! Build-12340 guild and arena-team charters, onto the shared petition events and from the shared
//! petition commands. Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03
//! `Handlers/PetitionsHandler.cpp`. Against 1.12: each `SMSG_PETITION_SHOWLIST` row adds a required
//! signature count, `SMSG_PETITION_QUERY_RESPONSE` has one word fewer before the gender and ends with
//! the charter type (0 guild, 1 arena team), `CMSG_PETITION_BUY` carries ten empty strings and the
//! charter index, and `CMSG_OFFER_PETITION` leads with a word the server skips. Strict framing.
use crate::messages::{
    PetitionQueryResponse, PetitionRename, PetitionShowList, PetitionShowListEntry,
    PetitionShowSignatures, PetitionSignResults, PetitionSignature,
};
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 7] = [0x01bc, 0x01bf, 0x01c1, 0x01c2, 0x01c5, 0x01c7, 0x02c1];

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
fn cstr(r: &mut Cursor<&[u8]>) -> io::Result<String> {
    let mut out = Vec::new();
    loop {
        match u8_(r)? {
            0 => return Ok(String::from_utf8_lossy(&out).into_owned()),
            b => out.push(b),
        }
    }
}

/// One reply as the shared petition event. The query response's charter type and each list row's
/// required-signature count have no shared field and are dropped.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let event = match opcode {
        0x01bc => {
            let npc = u64_(r)?;
            let count = u8_(r)?;
            let mut entries = Vec::new();
            for _ in 0..count {
                let entry = PetitionShowListEntry {
                    index: u32_(r)?,
                    charter_entry: u32_(r)?,
                    charter_display_id: u32_(r)?,
                    charter_cost: u32_(r)? as i32,
                    entry_flags: u32_(r)? as i32,
                };
                let _required_signatures = u32_(r)?;
                entries.push(entry);
            }
            SessionEvent::PetitionShowList(PetitionShowList { npc, entries })
        }
        0x01bf => {
            let item = u64_(r)?;
            let owner = u64_(r)?;
            let petition_id = u32_(r)?;
            let count = u8_(r)?;
            let mut signatures = Vec::new();
            for _ in 0..count {
                signatures.push(PetitionSignature {
                    signer: u64_(r)?,
                    unknown: u32_(r)?,
                });
            }
            SessionEvent::PetitionShowSignatures(PetitionShowSignatures {
                item,
                owner,
                petition_id,
                signatures,
            })
        }
        0x01c1 => SessionEvent::PetitionSignResults(PetitionSignResults {
            item: u64_(r)?,
            player: u64_(r)?,
            result: u32_(r)?,
        }),
        0x01c2 => SessionEvent::PetitionDeclined { player: u64_(r)? },
        0x01c5 => SessionEvent::TurnInPetitionResults { result: u32_(r)? },
        0x01c7 => {
            let petition_id = u32_(r)?;
            let owner = u64_(r)?;
            let name = cstr(r)?;
            let body_text = cstr(r)?;
            let min_signatures = u32_(r)?;
            let max_signatures = u32_(r)?;
            let deadline = u32_(r)?;
            let creation = u32_(r)?;
            let allowed_guild_id = u32_(r)?;
            let allowed_classes = u32_(r)?;
            let allowed_races = u32_(r)?;
            let allowed_gender = u16_(r)?;
            let allowed_min_level = u32_(r)?;
            let allowed_max_level = u32_(r)?;
            let count = u32_(r)?;
            if u64::from(count) > (body.len() - r.position() as usize) as u64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "petition choices",
                ));
            }
            let mut choices = Vec::new();
            for _ in 0..count {
                choices.push(cstr(r)?);
            }
            let default_choice = u32_(r)?;
            let _charter_type = u32_(r)?;
            SessionEvent::PetitionQueryResponse(PetitionQueryResponse {
                petition_id,
                owner,
                name,
                body_text,
                flags: 0,
                min_signatures,
                max_signatures,
                deadline,
                creation,
                allowed_guild_id,
                allowed_classes,
                allowed_races,
                allowed_gender,
                allowed_min_level,
                allowed_max_level,
                choices,
                default_choice,
            })
        }
        0x02c1 => SessionEvent::PetitionRenamed(PetitionRename {
            item: u64_(r)?,
            name: cstr(r)?,
        }),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "petition opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "petition packet extent",
        ));
    }
    Ok(event)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    ShowList {
        npc: u64,
    },
    /// `index` is the charter row, 1-based; AC reads it only for arena-team charters.
    Buy {
        npc: u64,
        name: String,
        index: u32,
    },
    ShowSignatures {
        item: u64,
    },
    Sign {
        item: u64,
        byte: i8,
    },
    Offer {
        item: u64,
        player: u64,
    },
    TurnIn {
        item: u64,
    },
    Query {
        petition_id: u32,
        item: u64,
    },
    Rename {
        item: u64,
        name: String,
    },
    Decline {
        item: u64,
    },
}

fn push_cstr(body: &mut Vec<u8>, text: &str) {
    body.extend(text.split('\0').next().unwrap_or("").as_bytes());
    body.push(0);
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let mut body = Vec::new();
    let opcode = match command {
        Command::ShowList { npc } => {
            body.extend(npc.to_le_bytes());
            0x01bb
        }
        // `HandlePetitionBuyOpcode`: npc, u32, u64, name, a string, seven u32, a u16, three u32,
        // ten strings, the index, a u32.
        Command::Buy { npc, name, index } => {
            body.extend(npc.to_le_bytes());
            body.extend(0u32.to_le_bytes());
            body.extend(0u64.to_le_bytes());
            push_cstr(&mut body, name);
            body.push(0);
            body.extend([0u8; 28]);
            body.extend(0u16.to_le_bytes());
            body.extend([0u8; 12]);
            body.extend([0u8; 10]);
            body.extend(index.to_le_bytes());
            body.extend(0u32.to_le_bytes());
            0x01bd
        }
        Command::ShowSignatures { item } => {
            body.extend(item.to_le_bytes());
            0x01be
        }
        Command::Sign { item, byte } => {
            body.extend(item.to_le_bytes());
            body.push(*byte as u8);
            0x01c0
        }
        Command::Offer { item, player } => {
            body.extend(0u32.to_le_bytes());
            body.extend(item.to_le_bytes());
            body.extend(player.to_le_bytes());
            0x01c3
        }
        Command::TurnIn { item } => {
            body.extend(item.to_le_bytes());
            0x01c4
        }
        Command::Query { petition_id, item } => {
            body.extend(petition_id.to_le_bytes());
            body.extend(item.to_le_bytes());
            0x01c6
        }
        Command::Rename { item, name } => {
            body.extend(item.to_le_bytes());
            push_cstr(&mut body, name);
            0x02c1
        }
        Command::Decline { item } => {
            body.extend(item.to_le_bytes());
            0x01c2
        }
    };
    (opcode, body)
}

#[cfg(test)]
#[path = "wrath_petition_tests.rs"]
mod tests;
