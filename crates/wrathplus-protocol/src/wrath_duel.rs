//! Lossless build-12340 duel bodies. AzerothCore reference pinned to
//! 7f12e89ee5f467a50e62eba1d525eac7dc953d03: SpellEffects.cpp EffectDuel,
//! Player.cpp DuelComplete/CheckDuelDistance/SendDuelCountdown, DuelHandler.cpp.
//! Unlike Vanilla's presentation codecs, bytes and countdown are not normalized.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Requested {
        arbiter: u64,
        challenger: u64,
    },
    OutOfBounds,
    InBounds,
    Complete {
        started: u8,
    },
    Winner {
        mode: u8,
        winner: Vec<u8>,
        loser: Vec<u8>,
    },
    /// Raw wire word, passed as 3000 by AC. No rounding or time conversion.
    Countdown {
        counter: u32,
    },
}

fn string(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u8>> {
    let start = r.position() as usize;
    let end = r.get_ref()[start..]
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?
        + start;
    let value = r.get_ref()[start..end].to_vec();
    r.set_position((end + 1) as u64);
    Ok(value)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        0x0167 => Packet::Requested {
            arbiter: u64_le(&mut r)?,
            challenger: u64_le(&mut r)?,
        },
        0x0168 => Packet::OutOfBounds,
        0x0169 => Packet::InBounds,
        0x016a => Packet::Complete {
            started: u8_le(&mut r)?,
        },
        0x016b => Packet::Winner {
            mode: u8_le(&mut r)?,
            winner: string(&mut r)?,
            loser: string(&mut r)?,
        },
        0x02b7 => Packet::Countdown {
            counter: u32_le(&mut r)?,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported duel opcode",
            ))
        }
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing duel bytes",
        ));
    }
    Ok(packet)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Accepted { arbiter: u64 },
    Cancelled { arbiter: u64 },
}

impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
            Self::Accepted { .. } => 0x016c,
            Self::Cancelled { .. } => 0x016d,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Accepted { arbiter } | Self::Cancelled { arbiter } => {
                arbiter.to_le_bytes().to_vec()
            }
        }
    }
}

#[cfg(test)]
#[path = "wrath_duel_tests.rs"]
mod tests;
