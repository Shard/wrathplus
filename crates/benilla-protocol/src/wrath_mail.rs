//! Build 12340 mail notifications. AzerothCore Player::SendNewMail and
//! MailHandler.cpp::HandleQueryNextMailTime; inbox/attachments use a different native layout.
use crate::events::SessionEvent;
use crate::wrath_session::{f32_le, u32_le, u64_le};
use std::io::{self, Cursor};

pub const SMSG_MAIL_LIST_RESULT: u16 = 0x23b;
pub const CMSG_GET_MAIL_LIST: u32 = 0x23a;

pub const MSG_QUERY_NEXT_MAIL_TIME: u16 = 0x284;
pub const SMSG_RECEIVED_MAIL: u16 = 0x285;

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        MSG_QUERY_NEXT_MAIL_TIME | SMSG_RECEIVED_MAIL | SMSG_MAIL_LIST_RESULT
    )
}

/// The request on this bidirectional opcode has no payload.
pub fn encode_query_next_mail_time() -> Vec<u8> {
    Vec::new()
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    if opcode == SMSG_MAIL_LIST_RESULT {
        return Ok(vec![SessionEvent::WrathMailList {
            inbox: decode_inbox(body)?,
        }]);
    }
    let mut r = Cursor::new(body);
    let seconds = f32_le(&mut r)?;
    if !seconds.is_finite() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "nonfinite mail delay",
        ));
    }
    let event = match opcode {
        SMSG_RECEIVED_MAIL => SessionEvent::ReceivedMail { seconds },
        MSG_QUERY_NEXT_MAIL_TIME => {
            let count = u32_le(&mut r)?;
            // AC sends at most two unique senders, each a 24-byte preview. Validate the
            // complete native tail even though HasNewMail only consumes the delay.
            if count > 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "mail preview count",
                ));
            }
            for _ in 0..count {
                let _player = u64_le(&mut r)?;
                let _entry = u32_le(&mut r)?;
                let _kind = u32_le(&mut r)?;
                let _stationery = u32_le(&mut r)?;
                if !f32_le(&mut r)?.is_finite() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "mail preview delay",
                    ));
                }
            }
            SessionEvent::NextMailTime { seconds }
        }
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "mail opcode")),
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "mail trailing bytes",
        ));
    }
    Ok(vec![event])
}

#[cfg(test)]
#[path = "wrath_mail/tests.rs"]
mod tests;

mod inbox;
pub use inbox::{decode_inbox, Attachment, Inbox, Mail};

#[cfg(test)]
#[path = "wrath_mail/inbox_tests.rs"]
mod inbox_tests;
