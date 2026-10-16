//! Build-12340 inspect and talent commands and replies.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: MiscHandler.cpp
//! (`HandleInspectOpcode`), Player.cpp (`BuildPlayerTalentsInfoData`, `BuildEnchantmentsInfoData`,
//! `SendTalentWipeConfirm`), SkillHandler.cpp (`HandleLearnTalentOpcode`,
//! `HandleLearnPreviewTalents`, `HandleTalentWipeConfirmOpcode`). Strict framing.
//!
//! The talent-wipe confirm means what 1.12's does and feeds the shared event. The inspect reply
//! (another player's talent groups, glyphs and equipped enchants) has no shared event and is
//! capture-only.
use crate::wire::read_packed_guid;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 2] = [0x02aa, 0x03f4];

/// `EQUIPMENT_SLOT_END`, `MAX_ENCHANTMENT_SLOT`.
const EQUIPMENT_SLOTS: u32 = 19;
const ENCHANTMENT_SLOTS: u16 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalentGroup {
    /// (Talent.dbc id, rank 0..4).
    pub talents: Vec<(u32, u8)>,
    /// GlyphProperties.dbc ids per glyph slot.
    pub glyphs: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedItem {
    pub slot: u8,
    pub entry: u32,
    /// (enchantment slot, SpellItemEnchantment id) for each set slot.
    pub enchants: Vec<(u8, u16)>,
    pub random_property: i16,
    pub creator: u64,
    pub suffix_factor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_INSPECT_TALENT`.
    Inspect {
        player: u64,
        unspent: u32,
        active_group: u8,
        groups: Vec<TalentGroup>,
        items: Vec<InspectedItem>,
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

fn inspect(r: &mut Cursor<&[u8]>) -> io::Result<Packet> {
    let player = read_packed_guid(r)?;
    let unspent = u32_(r)?;
    let (count, active_group) = (u8_(r)?, u8_(r)?);
    // AC sends zero groups when talent inspection is off, and at most two specs otherwise.
    if count > 2 {
        return Err(invalid("talent group count"));
    }
    let groups = (0..count)
        .map(|_| {
            let talents = (0..u8_(r)?)
                .map(|_| Ok((u32_(r)?, u8_(r)?)))
                .collect::<io::Result<_>>()?;
            let glyphs = (0..u8_(r)?).map(|_| u16_(r)).collect::<io::Result<_>>()?;
            Ok(TalentGroup { talents, glyphs })
        })
        .collect::<io::Result<_>>()?;
    let slot_mask = u32_(r)?;
    if slot_mask >> EQUIPMENT_SLOTS != 0 {
        return Err(invalid("inspect slot mask"));
    }
    let items = (0..EQUIPMENT_SLOTS as u8)
        .filter(|slot| slot_mask & (1 << slot) != 0)
        .map(|slot| {
            let entry = u32_(r)?;
            let enchant_mask = u16_(r)?;
            if enchant_mask >> ENCHANTMENT_SLOTS != 0 {
                return Err(invalid("inspect enchantment mask"));
            }
            let enchants = (0..ENCHANTMENT_SLOTS as u8)
                .filter(|e| enchant_mask & (1 << e) != 0)
                .map(|e| Ok((e, u16_(r)?)))
                .collect::<io::Result<_>>()?;
            Ok(InspectedItem {
                slot,
                entry,
                enchants,
                random_property: u16_(r)? as i16,
                creator: read_packed_guid(r)?,
                suffix_factor: u32_(r)?,
            })
        })
        .collect::<io::Result<_>>()?;
    Ok(Packet::Inspect {
        player,
        unspent,
        active_group,
        groups,
        items,
    })
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let event = match opcode {
        // A zero trainer and cost is AC's "you have no talents" answer.
        0x02aa => SessionEvent::TalentWipeConfirm {
            trainer: u64_(r)?,
            cost: u32_(r)?,
        },
        0x03f4 => SessionEvent::WrathInspect { packet: inspect(r)? },
        _ => return Err(invalid("inspect opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("inspect packet extent"));
    }
    Ok(event)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_INSPECT`.
    Inspect { target: u64 },
    /// `CMSG_LEARN_TALENT`.
    LearnTalent { talent_id: u32, rank: u32 },
    /// `MSG_TALENT_WIPE_CONFIRM`: the trainer.
    TalentWipeConfirm { trainer: u64 },
    /// `CMSG_LEARN_PREVIEW_TALENTS`: (talent, rank) pairs; AC reads at most 150.
    LearnPreviewTalents { talents: Vec<(u32, u32)> },
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let mut body = Vec::new();
    let opcode = match command {
        Command::Inspect { target } => {
            body.extend(target.to_le_bytes());
            0x0114
        }
        Command::LearnTalent { talent_id, rank } => {
            body.extend(talent_id.to_le_bytes());
            body.extend(rank.to_le_bytes());
            0x0251
        }
        Command::TalentWipeConfirm { trainer } => {
            body.extend(trainer.to_le_bytes());
            0x02aa
        }
        Command::LearnPreviewTalents { talents } => {
            body.extend((talents.len() as u32).to_le_bytes());
            for (talent, rank) in talents {
                body.extend(talent.to_le_bytes());
                body.extend(rank.to_le_bytes());
            }
            0x04c1
        }
    };
    (opcode, body)
}

/// A packed GUID, for inspect fixtures.
#[cfg(test)]
fn packed(guid: u64) -> Vec<u8> {
    let mut body = Vec::new();
    crate::wire::write_packed_guid(guid, &mut body).expect("writing to a Vec");
    body
}

#[cfg(test)]
mod tests;
