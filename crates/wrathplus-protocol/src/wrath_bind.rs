//! Build 12340 innkeeper binding: AzerothCore Player::SetBindPoint,
//! WorldSession::HandleBinderActivateOpcode and Spell::EffectBind.

use crate::events::SessionEvent;
use crate::wrath_session::{f32_le, u32_le, u64_le};
use std::io::{self, Cursor};

pub const SMSG_BINDPOINTUPDATE: u16 = 0x155;
pub const SMSG_PLAYERBOUND: u16 = 0x158;
pub const SMSG_BINDER_CONFIRM: u16 = 0x2eb;
pub const CMSG_BINDER_ACTIVATE: u32 = 0x1b5;

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_BINDPOINTUPDATE | SMSG_PLAYERBOUND | SMSG_BINDER_CONFIRM
    )
}

pub fn encode_activate(binder: u64) -> Vec<u8> {
    binder.to_le_bytes().to_vec()
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    let event = match opcode {
        SMSG_BINDER_CONFIRM => SessionEvent::BinderConfirm {
            binder: u64_le(&mut r)?,
        },
        SMSG_PLAYERBOUND => SessionEvent::PlayerBound {
            binder: u64_le(&mut r)?,
            area: u32_le(&mut r)?,
        },
        SMSG_BINDPOINTUPDATE => {
            // Shared home-bind storage needs the AreaTable id, not a movement teleport.
            for _ in 0..3 {
                let _position = f32_le(&mut r)?;
            }
            let _map = u32_le(&mut r)?;
            SessionEvent::BindPoint {
                area: u32_le(&mut r)?,
            }
        }
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "bind opcode")),
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bind trailing bytes",
        ));
    }
    Ok(vec![event])
}

#[cfg(test)]
#[path = "wrath_bind/tests.rs"]
mod tests;
