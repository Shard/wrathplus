//! Lossless build-12340 mail mutation bodies; no routing or local mailbox mutation.
//! AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: MailHandler.cpp,
//! Player::SendMailResult, WorldSession::SendShowMailBox, SharedDefines.h.
//! Native Wow.exe SHA256 c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c:
//! senders 00570910/005711c0/0056edf0/0056f140/0056f000/0056fe30/0056eef0;
//! 0047afe0 writes u8, 0047b0a0 u32, 0047b100 u64, 0047b300 NUL-terminated bytes.
use crate::wrath_session::{u32_le, u64_le};
use std::io::{self, Cursor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultTail {
    None,
    EquipError { error: u32 },
    ItemTaken { item_low_guid: u32, count: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailResult {
    pub mail_id: u32,
    pub action: u32,
    pub result: u32,
    pub tail: ResultTail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Result(MailResult),
    ShowMailbox { mailbox: u64 },
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        0x0239 => {
            let mail_id = u32_le(&mut r)?;
            let action = u32_le(&mut r)?;
            let result = u32_le(&mut r)?;
            // Equipment failure takes precedence, even for ITEM_TAKEN.
            let tail = if result == 1 {
                ResultTail::EquipError {
                    error: u32_le(&mut r)?,
                }
            } else if action == 2 {
                ResultTail::ItemTaken {
                    item_low_guid: u32_le(&mut r)?,
                    count: u32_le(&mut r)?,
                }
            } else {
                ResultTail::None
            };
            Packet::Result(MailResult {
                mail_id,
                action,
                result,
                tail,
            })
        }
        0x0297 => Packet::ShowMailbox {
            mailbox: u64_le(&mut r)?,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "mail mutation opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "mail mutation trailing bytes",
        ));
    }
    Ok(packet)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub slot: u8,
    pub item: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendMail {
    pub mailbox: u64,
    pub receiver: Vec<u8>,
    pub subject: Vec<u8>,
    pub body: Vec<u8>,
    pub stationery: u32,
    pub unknown_word: u32,
    pub attachments: Vec<Attachment>,
    pub money: u32,
    pub cod: u32,
    pub unknown_guid: u64,
    pub unknown_byte: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Send(SendMail),
    TakeMoney {
        mailbox: u64,
        mail_id: u32,
    },
    TakeItem {
        mailbox: u64,
        mail_id: u32,
        item_low_guid: u32,
    },
    MarkRead {
        mailbox: u64,
        mail_id: u32,
    },
    Return {
        mailbox: u64,
        mail_id: u32,
        sender: u64,
    },
    Delete {
        mailbox: u64,
        mail_id: u32,
        template: u32,
    },
    CreateTextItem {
        mailbox: u64,
        mail_id: u32,
    },
}

impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
            Self::Send(_) => 0x0238,
            Self::TakeMoney { .. } => 0x0245,
            Self::TakeItem { .. } => 0x0246,
            Self::MarkRead { .. } => 0x0247,
            Self::Return { .. } => 0x0248,
            Self::Delete { .. } => 0x0249,
            Self::CreateTextItem { .. } => 0x024a,
        }
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        if let Self::Send(mail) = self {
            // Native client supports twelve attachment slots; never silently truncate.
            if mail.attachments.len() > 12 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "mail attachment count",
                ));
            }
            out.extend_from_slice(&mail.mailbox.to_le_bytes());
            for bytes in [&mail.receiver, &mail.subject, &mail.body] {
                if bytes.contains(&0) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "mail embedded NUL",
                    ));
                }
                out.extend_from_slice(bytes);
                out.push(0);
            }
            out.extend_from_slice(&mail.stationery.to_le_bytes());
            out.extend_from_slice(&mail.unknown_word.to_le_bytes());
            out.push(mail.attachments.len() as u8);
            for item in &mail.attachments {
                out.push(item.slot);
                out.extend_from_slice(&item.item.to_le_bytes());
            }
            out.extend_from_slice(&mail.money.to_le_bytes());
            out.extend_from_slice(&mail.cod.to_le_bytes());
            out.extend_from_slice(&mail.unknown_guid.to_le_bytes());
            out.push(mail.unknown_byte);
            return Ok(out);
        }
        let (mailbox, mail_id) = match *self {
            Self::TakeMoney { mailbox, mail_id }
            | Self::TakeItem {
                mailbox, mail_id, ..
            }
            | Self::MarkRead { mailbox, mail_id }
            | Self::Return {
                mailbox, mail_id, ..
            }
            | Self::Delete {
                mailbox, mail_id, ..
            }
            | Self::CreateTextItem { mailbox, mail_id } => (mailbox, mail_id),
            Self::Send(_) => unreachable!(),
        };
        out.extend_from_slice(&mailbox.to_le_bytes());
        out.extend_from_slice(&mail_id.to_le_bytes());
        match *self {
            Self::TakeItem { item_low_guid, .. } => {
                out.extend_from_slice(&item_low_guid.to_le_bytes())
            }
            Self::Return { sender, .. } => out.extend_from_slice(&sender.to_le_bytes()),
            Self::Delete { template, .. } => out.extend_from_slice(&template.to_le_bytes()),
            _ => {}
        }
        Ok(out)
    }
}

#[cfg(test)]
#[path = "wrath_mail_mutations_tests.rs"]
mod tests;
