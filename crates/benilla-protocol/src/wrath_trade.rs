//! Lossless build-12340 trade bodies. Reference: AzerothCore
//! 7f12e89ee5f467a50e62eba1d525eac7dc953d03 TradeHandler.cpp.
//! Native Wow.exe SHA256 c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c:
//! senders 00703cf0/00703e00/00703e70/00703ee0/00703f50/00703fd0/00704040;
//! 0047b0a0 writes four bytes, 0047b100 writes eight. Status reader 007044a0.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusTail {
    None,
    Trader {
        guid: u64,
    },
    Open {
        trade_id: u32,
    },
    Close {
        result: u32,
        target_result: u8,
        item: u32,
    },
    Slot {
        slot: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub code: u32,
    pub tail: StatusTail,
}

/// One slot byte followed by an unconditional 72-byte item block, including empty items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub slot: u8,
    pub entry: u32,
    pub display_id: u32,
    pub count: u32,
    pub wrapped: u32,
    pub gift_creator: u64,
    pub permanent_enchant: u32,
    pub socket_enchants: [u32; 3],
    pub creator: u64,
    pub charges: u32,
    pub suffix_factor: u32,
    pub random_property: i32,
    pub lock_id: u32,
    pub max_durability: u32,
    pub durability: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extended {
    pub trader: u8,
    pub trade_id: u32,
    pub count_word_1: u32,
    pub count_word_2: u32,
    pub gold: u32,
    pub spell: u32,
    /// Ordered wire rows; neither count word is normalized or used to invent rows.
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Status(Status),
    Extended(Extended),
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        0x0120 => {
            let code = u32_le(&mut r)?;
            let tail = match code {
                1 => StatusTail::Trader {
                    guid: u64_le(&mut r)?,
                },
                2 => StatusTail::Open {
                    trade_id: u32_le(&mut r)?,
                },
                12 => StatusTail::Close {
                    result: u32_le(&mut r)?,
                    target_result: u8_le(&mut r)?,
                    item: u32_le(&mut r)?,
                },
                22 | 23 => StatusTail::Slot {
                    slot: u8_le(&mut r)?,
                },
                _ => StatusTail::None,
            };
            Packet::Status(Status { code, tail })
        }
        0x0121 => {
            let trader = u8_le(&mut r)?;
            let trade_id = u32_le(&mut r)?;
            let count_word_1 = u32_le(&mut r)?;
            let count_word_2 = u32_le(&mut r)?;
            let gold = u32_le(&mut r)?;
            let spell = u32_le(&mut r)?;
            let remaining = body.len() - r.position() as usize;
            if remaining % 73 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "partial trade item row",
                ));
            }
            let mut items = Vec::with_capacity(remaining / 73);
            while r.position() < body.len() as u64 {
                items.push(Item {
                    slot: u8_le(&mut r)?,
                    entry: u32_le(&mut r)?,
                    display_id: u32_le(&mut r)?,
                    count: u32_le(&mut r)?,
                    wrapped: u32_le(&mut r)?,
                    gift_creator: u64_le(&mut r)?,
                    permanent_enchant: u32_le(&mut r)?,
                    socket_enchants: [u32_le(&mut r)?, u32_le(&mut r)?, u32_le(&mut r)?],
                    creator: u64_le(&mut r)?,
                    charges: u32_le(&mut r)?,
                    suffix_factor: u32_le(&mut r)?,
                    random_property: u32_le(&mut r)? as i32,
                    lock_id: u32_le(&mut r)?,
                    max_durability: u32_le(&mut r)?,
                    durability: u32_le(&mut r)?,
                });
            }
            Packet::Extended(Extended {
                trader,
                trade_id,
                count_word_1,
                count_word_2,
                gold,
                spell,
                items,
            })
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported trade opcode",
            ))
        }
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing trade bytes",
        ));
    }
    Ok(packet)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Initiate {
        target: u64,
    },
    Begin,
    Busy,
    Ignore,
    /// Native 00703f50 reads DAT00ca0b70, initialized to 1 by 007044a0 case 2.
    /// This is NOT the trade ID (DAT00ca0b68); the codec synthesizes no client state.
    Accept {
        raw: u32,
    },
    Unaccept,
    Cancel,
    SetItem {
        trade_slot: u8,
        bag: u8,
        slot: u8,
    },
    ClearItem {
        trade_slot: u8,
    },
    SetGold {
        gold: u32,
    },
}

impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
            Self::Initiate { .. } => 0x0116,
            Self::Begin => 0x0117,
            Self::Busy => 0x0118,
            Self::Ignore => 0x0119,
            Self::Accept { .. } => 0x011a,
            Self::Unaccept => 0x011b,
            Self::Cancel => 0x011c,
            Self::SetItem { .. } => 0x011d,
            Self::ClearItem { .. } => 0x011e,
            Self::SetGold { .. } => 0x011f,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match *self {
            Self::Initiate { target } => target.to_le_bytes().to_vec(),
            Self::Accept { raw } => raw.to_le_bytes().to_vec(),
            Self::SetGold { gold } => gold.to_le_bytes().to_vec(),
            Self::SetItem {
                trade_slot,
                bag,
                slot,
            } => vec![trade_slot, bag, slot],
            Self::ClearItem { trade_slot } => vec![trade_slot],
            Self::Begin | Self::Busy | Self::Ignore | Self::Unaccept | Self::Cancel => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "wrath_trade_tests.rs"]
mod tests;
