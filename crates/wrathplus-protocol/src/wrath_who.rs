//! Native build-12340 WHO/WHOIS codecs, not Vanilla social projections.
//! Reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03,
//! MiscHandler.cpp HandleWhoOpcode/HandleWhoisOpcode and Opcodes.h.
//! WHOIS text can contain private account data. Do not log it or use real replies
//! as fixtures. These codecs neither authorize WHOIS nor normalize search text.
use crate::wrath_session::{u32_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_WHO: u16 = 0x0063;
pub const SMSG_WHOIS: u16 = 0x0065;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: Vec<u8>,
    pub guild: Vec<u8>,
    pub level: u32,
    pub class: u32,
    pub race: u32,
    pub gender: u8,
    pub zone: u32,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Packet {
    Who { total: u32, entries: Vec<Entry> },
    Whois { message: Vec<u8> },
}

fn string(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u8>> {
    let start = r.position() as usize;
    let end = r.get_ref()[start..]
        .iter()
        .position(|&v| v == 0)
        .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?
        + start;
    let value = r.get_ref()[start..end].to_vec();
    r.set_position((end + 1) as u64);
    Ok(value)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        SMSG_WHO => {
            let displayed = u32_le(&mut r)? as usize;
            let total = u32_le(&mut r)?;
            // Two empty strings plus four u32 values and gender. No client row cap.
            if displayed > (body.len() - 8) / 19 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            let mut entries = Vec::with_capacity(displayed);
            for _ in 0..displayed {
                entries.push(Entry {
                    name: string(&mut r)?,
                    guild: string(&mut r)?,
                    level: u32_le(&mut r)?,
                    class: u32_le(&mut r)?,
                    race: u32_le(&mut r)?,
                    gender: u8_le(&mut r)?,
                    zone: u32_le(&mut r)?,
                });
            }
            Packet::Who { total, entries }
        }
        SMSG_WHOIS => Packet::Whois {
            message: string(&mut r)?,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported WHO opcode",
            ))
        }
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing WHO bytes",
        ));
    }
    Ok(packet)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub level_min: u32,
    pub level_max: u32,
    pub player_name: Vec<u8>,
    pub guild_name: Vec<u8>,
    pub race_mask: u32,
    pub class_mask: u32,
    pub zones: Vec<u32>,
    pub search_terms: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Command {
    Who(Query),
    Whois { name: Vec<u8> },
}

impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
            Self::Who(_) => 0x0062,
            Self::Whois { .. } => 0x0064,
        }
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let mut body = Vec::new();
        match self {
            Self::Who(q) => {
                // AC rejects these arrays above the native client limits. Never
                // silently narrow a query by truncating its filters.
                if q.zones.len() > 10 || q.search_terms.len() > 4 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "WHO filter count exceeds native limit",
                    ));
                }
                body.extend(q.level_min.to_le_bytes());
                body.extend(q.level_max.to_le_bytes());
                put_string(&mut body, &q.player_name)?;
                put_string(&mut body, &q.guild_name)?;
                body.extend(q.race_mask.to_le_bytes());
                body.extend(q.class_mask.to_le_bytes());
                body.extend((q.zones.len() as u32).to_le_bytes());
                for zone in &q.zones {
                    body.extend(zone.to_le_bytes());
                }
                body.extend((q.search_terms.len() as u32).to_le_bytes());
                for term in &q.search_terms {
                    put_string(&mut body, term)?;
                }
            }
            Self::Whois { name } => put_string(&mut body, name)?,
        }
        Ok(body)
    }
}

fn put_string(body: &mut Vec<u8>, value: &[u8]) -> io::Result<()> {
    if value.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "embedded WHO NUL",
        ));
    }
    body.extend(value);
    body.push(0);
    Ok(())
}

#[cfg(test)]
#[path = "wrath_who_tests.rs"]
mod tests;

// WHOIS can contain account identifiers; nested SessionEvent/ClientCommand
// Debug must remain safe even in generic connection/drop diagnostics.
impl std::fmt::Debug for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Who { total, entries } => f
                .debug_struct("Who")
                .field("total", total)
                .field("entries", entries)
                .finish(),
            Self::Whois { .. } => f.write_str("Whois { <redacted> }"),
        }
    }
}
impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Who(query) => f.debug_tuple("Who").field(query).finish(),
            Self::Whois { .. } => f.write_str("Whois { <redacted> }"),
        }
    }
}
