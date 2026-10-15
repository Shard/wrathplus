//! Lossless build-12340 stable transport. AzerothCore Handlers/NPCHandler.cpp:
//! SendStablePet/SendStableResult and Handle*Stable*; Opcodes.h 0x26f..0x275.
//! Wrath has no loyalty word; the trailing row byte is flags, NOT a slot index.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const MSG_LIST_STABLED_PETS: u16 = 0x026f;
pub const SMSG_STABLE_RESULT: u16 = 0x0273;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pet {
    pub pet_number: u32,
    pub creature_entry: u32,
    pub level: u32,
    /// Raw NUL-terminated name bytes without the terminator.
    pub name: Vec<u8>,
    pub flags: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    List {
        npc: u64,
        purchased_slots: u8,
        pets: Vec<Pet>,
    },
    Result {
        result: u8,
    },
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        MSG_LIST_STABLED_PETS => {
            let npc = u64_le(&mut r)?;
            let count = u8_le(&mut r)?;
            let purchased_slots = u8_le(&mut r)?;
            let mut pets = Vec::new();
            for _ in 0..count {
                let pet_number = u32_le(&mut r)?;
                let creature_entry = u32_le(&mut r)?;
                let level = u32_le(&mut r)?;
                let mut name = Vec::new();
                loop {
                    let byte = u8_le(&mut r)?;
                    if byte == 0 {
                        break;
                    }
                    name.push(byte);
                }
                pets.push(Pet {
                    pet_number,
                    creature_entry,
                    level,
                    name,
                    flags: u8_le(&mut r)?,
                });
            }
            Packet::List {
                npc,
                purchased_slots,
                pets,
            }
        }
        SMSG_STABLE_RESULT => Packet::Result {
            result: u8_le(&mut r)?,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported stable opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stable trailing bytes",
        ));
    }
    Ok(packet)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    List { npc: u64 },
    Stable { npc: u64 },
    Unstable { npc: u64, pet_number: u32 },
    BuySlot { npc: u64 },
    Swap { npc: u64, pet_number: u32 },
}
impl Command {
    pub fn opcode(&self) -> u32 {
        match self {
            Self::List { .. } => 0x026f,
            Self::Stable { .. } => 0x0270,
            Self::Unstable { .. } => 0x0271,
            Self::BuySlot { .. } => 0x0272,
            Self::Swap { .. } => 0x0275,
        }
    }
    pub fn encode(&self) -> Vec<u8> {
        let (npc, pet) = match *self {
            Self::List { npc } | Self::Stable { npc } | Self::BuySlot { npc } => (npc, None),
            Self::Unstable { npc, pet_number } | Self::Swap { npc, pet_number } => {
                (npc, Some(pet_number))
            }
        };
        let mut body = npc.to_le_bytes().to_vec();
        if let Some(pet) = pet {
            body.extend(pet.to_le_bytes());
        }
        body
    }
}

#[cfg(test)]
#[path = "wrath_stable_tests.rs"]
mod tests;
