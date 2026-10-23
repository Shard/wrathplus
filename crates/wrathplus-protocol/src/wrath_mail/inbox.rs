//! AzerothCore MailHandler.cpp::SendMailList; Item.h's seven inspected enchantment slots.
use super::*;
use crate::wrath_session::{cstring, u16_le, u8_le};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub slot: u8,
    pub guid_low: u32,
    pub entry: u32,
    /// (id, duration, charges), including temporary and socket enchantments.
    pub enchantments: [[u32; 3]; 7],
    pub random_property: i32,
    pub suffix_factor: u32,
    pub count: u32,
    pub charges: u32,
    pub max_durability: u32,
    pub durability: u32,
    pub unknown: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mail {
    pub id: u32,
    pub kind: u8,
    pub sender: u64,
    pub cod: u32,
    pub unknown: u32,
    pub stationery: u32,
    pub money: u32,
    pub checked: u32,
    pub expire_days: f32,
    pub template_id: u32,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inbox {
    /// Includes messages beyond the returned page; do not replace with mails.len().
    pub total_count: u32,
    pub mails: Vec<Mail>,
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Wrath inbox layout")
}

pub fn decode_inbox(body: &[u8]) -> io::Result<Inbox> {
    let mut r = Cursor::new(body);
    let total_count = u32_le(&mut r)?;
    let count = u8_le(&mut r)?;
    if count > 50 || total_count < u32::from(count) {
        return Err(invalid());
    }
    let mut mails = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let start = r.position() as usize;
        let size = usize::from(u16_le(&mut r)?);
        let end = start
            .checked_add(size)
            .filter(|e| *e <= body.len())
            .ok_or_else(invalid)?;
        if size < 2 {
            return Err(invalid());
        }
        // A bounded cursor prevents malformed strings/counts from reading the next row.
        let mut row = Cursor::new(&body[start + 2..end]);
        let id = u32_le(&mut row)?;
        let kind = u8_le(&mut row)?;
        let sender = match kind {
            0 => u64_le(&mut row)?,
            2..=5 => u64::from(u32_le(&mut row)?),
            _ => return Err(invalid()),
        };
        let cod = u32_le(&mut row)?;
        let unknown = u32_le(&mut row)?;
        let stationery = u32_le(&mut row)?;
        let money = u32_le(&mut row)?;
        let checked = u32_le(&mut row)?;
        let expire_days = f32_le(&mut row)?;
        if !expire_days.is_finite() {
            return Err(invalid());
        }
        let template_id = u32_le(&mut row)?;
        let subject = cstring(&mut row)?;
        let text = cstring(&mut row)?;
        let n = u8_le(&mut row)?;
        if n > 12 {
            return Err(invalid());
        }
        let mut attachments = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let slot = u8_le(&mut row)?;
            if slot >= 12 || attachments.iter().any(|a: &Attachment| a.slot == slot) {
                return Err(invalid());
            }
            let guid_low = u32_le(&mut row)?;
            let entry = u32_le(&mut row)?;
            let mut enchantments = [[0; 3]; 7];
            for enchantment in &mut enchantments {
                for word in enchantment {
                    *word = u32_le(&mut row)?;
                }
            }
            attachments.push(Attachment {
                slot,
                guid_low,
                entry,
                enchantments,
                random_property: u32_le(&mut row)? as i32,
                suffix_factor: u32_le(&mut row)?,
                count: u32_le(&mut row)?,
                charges: u32_le(&mut row)?,
                max_durability: u32_le(&mut row)?,
                durability: u32_le(&mut row)?,
                unknown: u8_le(&mut row)?,
            });
        }
        if row.position() as usize != size - 2 {
            return Err(invalid());
        }
        mails.push(Mail {
            id,
            kind,
            sender,
            cod,
            unknown,
            stationery,
            money,
            checked,
            expire_days,
            template_id,
            subject,
            body: text,
            attachments,
        });
        r.set_position(end as u64);
    }
    if r.position() as usize != body.len() {
        return Err(invalid());
    }
    Ok(Inbox { total_count, mails })
}
