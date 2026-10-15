//! Native in-dungeon notifications, not inferred queue/roster transitions.
//! AzerothCore 7f12e89e LFGHandler.cpp::SendLfg{OfferContinue,TeleportError,Disabled}.
//! Native handler 0x0055bdc0 confirms one full word for continue/denied and
//! no body for disabled. Its continue-entry low-24-bit mask is presentation
//! policy only; retain the packed wire entry here.
use super::invalid;
use crate::wrath_session::u32_le;
use std::io::{self, Cursor};

pub const SMSG_LFG_OFFER_CONTINUE: u16 = 0x0293;
pub const SMSG_LFG_TELEPORT_DENIED: u16 = 0x0200;
pub const SMSG_LFG_DISABLED: u16 = 0x0398;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// Full packed entry; an offer is not acceptance or confirmed membership.
    OfferContinue {
        entry: u32,
    },
    /// Full wire word, including unknown reason values.
    TeleportDenied {
        reason: u32,
    },
    Disabled,
}
pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_LFG_OFFER_CONTINUE | SMSG_LFG_TELEPORT_DENIED | SMSG_LFG_DISABLED
    )
}
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        SMSG_LFG_OFFER_CONTINUE => Packet::OfferContinue {
            entry: u32_le(&mut r)?,
        },
        SMSG_LFG_TELEPORT_DENIED => Packet::TeleportDenied {
            reason: u32_le(&mut r)?,
        },
        SMSG_LFG_DISABLED => Packet::Disabled,
        _ => return Err(invalid("unsupported LFG dungeon opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("trailing LFG dungeon bytes"));
    }
    Ok(packet)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_continue_and_denied_words_preserve_full_values() {
        for (opcode, body, expected) in [
            (
                0x0293,
                [0x56, 0x34, 0x12, 0xab],
                Packet::OfferContinue { entry: 0xab123456 },
            ),
            (
                0x0200,
                [0xef, 0xcd, 0xab, 0x80],
                Packet::TeleportDenied { reason: 0x80abcdef },
            ),
            (0x0293, [0; 4], Packet::OfferContinue { entry: 0 }),
            (0x0200, [0; 4], Packet::TeleportDenied { reason: 0 }),
        ] {
            assert!(handles(opcode));
            assert!(super::super::handles(opcode));
            assert_eq!(
                super::super::decode(opcode, &body).unwrap(),
                super::super::Packet::Dungeon(expected)
            );
            for cut in 0..4 {
                assert!(decode(opcode, &body[..cut]).is_err());
            }
            let mut trailing = body.to_vec();
            trailing.push(0);
            assert!(decode(opcode, &trailing).is_err());
        }
    }
    #[test]
    fn disabled_is_explicit_empty_and_unknown_opcodes_fail() {
        assert_eq!(
            super::super::decode(0x0398, &[]).unwrap(),
            super::super::Packet::Dungeon(Packet::Disabled)
        );
        assert!(decode(0x0398, &[0]).is_err());
        assert!(!handles(0xffff));
        assert!(decode(0xffff, &[]).is_err());
    }
}
