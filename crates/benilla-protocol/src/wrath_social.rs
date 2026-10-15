//! Lossless build-12340 contact family. Reference: AzerothCore
//! 7f12e89ee5f467a50e62eba1d525eac7dc953d03 SocialMgr.cpp,
//! Handlers/Socialhandler.cpp and Opcodes.h. Separate from Vanilla social schemas.
//! No name normalization, contact caps or presence inference belongs here.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_CONTACT_LIST: u16 = 0x0067;
pub const SMSG_FRIEND_STATUS: u16 = 0x0068;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presence {
    pub area: u32,
    pub level: u32,
    pub class: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friend {
    pub status: u8,
    pub presence: Option<Presence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub guid: u64,
    pub flags: u32,
    pub note: Vec<u8>,
    /// Present exactly when contact flags contain bit 1, even if offline.
    pub friend: Option<Friend>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    ContactList {
        flags: u32,
        contacts: Vec<Contact>,
    },
    FriendStatus {
        result: u8,
        guid: u64,
        /// Sent only for added-online (6) and added-offline (7).
        note: Option<Vec<u8>>,
        /// Sent only for online (2) and added-online (6), regardless of status.
        online: Option<Friend>,
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

fn presence(r: &mut Cursor<&[u8]>) -> io::Result<Presence> {
    Ok(Presence {
        area: u32_le(r)?,
        level: u32_le(r)?,
        class: u32_le(r)?,
    })
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        SMSG_CONTACT_LIST => {
            let flags = u32_le(&mut r)?;
            let count = u32_le(&mut r)? as usize;
            // Minimum row: GUID, flags, empty terminated note. Validate before allocation.
            if count > (body.len() - 8) / 13 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            let mut contacts = Vec::with_capacity(count);
            for _ in 0..count {
                let guid = u64_le(&mut r)?;
                let flags = u32_le(&mut r)?;
                let note = string(&mut r)?;
                let friend = if flags & 1 != 0 {
                    let status = u8_le(&mut r)?;
                    Some(Friend {
                        status,
                        presence: if status != 0 {
                            Some(presence(&mut r)?)
                        } else {
                            None
                        },
                    })
                } else {
                    None
                };
                contacts.push(Contact {
                    guid,
                    flags,
                    note,
                    friend,
                });
            }
            Packet::ContactList { flags, contacts }
        }
        SMSG_FRIEND_STATUS => {
            let result = u8_le(&mut r)?;
            let guid = u64_le(&mut r)?;
            let note = if matches!(result, 6 | 7) {
                Some(string(&mut r)?)
            } else {
                None
            };
            let online = if matches!(result, 2 | 6) {
                Some(Friend {
                    status: u8_le(&mut r)?,
                    presence: Some(presence(&mut r)?),
                })
            } else {
                None
            };
            Packet::FriendStatus {
                result,
                guid,
                note,
                online,
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported social opcode",
            ))
        }
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing social bytes",
        ));
    }
    Ok(packet)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    ContactList { flags: u32 },
    AddFriend { name: Vec<u8>, note: Vec<u8> },
    DeleteFriend { guid: u64 },
    SetContactNotes { guid: u64, note: Vec<u8> },
    AddIgnore { name: Vec<u8> },
    DeleteIgnore { guid: u64 },
}

impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
            Self::ContactList { .. } => 0x0066,
            Self::AddFriend { .. } => 0x0069,
            Self::DeleteFriend { .. } => 0x006a,
            Self::SetContactNotes { .. } => 0x006b,
            Self::AddIgnore { .. } => 0x006c,
            Self::DeleteIgnore { .. } => 0x006d,
        }
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let mut body = Vec::new();
        match self {
            Self::ContactList { flags } => body.extend(flags.to_le_bytes()),
            Self::DeleteFriend { guid } | Self::DeleteIgnore { guid } => {
                body.extend(guid.to_le_bytes())
            }
            Self::AddFriend { name, note } => {
                put_string(&mut body, name)?;
                put_string(&mut body, note)?;
            }
            Self::SetContactNotes { guid, note } => {
                body.extend(guid.to_le_bytes());
                put_string(&mut body, note)?;
            }
            Self::AddIgnore { name } => put_string(&mut body, name)?,
        }
        Ok(body)
    }
}

fn put_string(body: &mut Vec<u8>, value: &[u8]) -> io::Result<()> {
    if value.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "embedded social NUL",
        ));
    }
    body.extend(value);
    body.push(0);
    Ok(())
}

#[cfg(test)]
#[path = "wrath_social_tests.rs"]
mod tests;
