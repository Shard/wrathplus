//! Build-12340 logout lifecycle. AzerothCore CharacterPackets.{h,cpp} defines the
//! response as u32 result + u8 instant; complete and cancel ACK have empty bodies.
//! Cancel is an empty ClientPacket. These packets never grant movement authority.

use std::io;

pub const CMSG_LOGOUT_REQUEST: u32 = 0x004b;
pub const CMSG_LOGOUT_CANCEL: u32 = 0x004e;
pub const SMSG_LOGOUT_RESPONSE: u16 = 0x004c;
pub const SMSG_LOGOUT_COMPLETE: u16 = 0x004d;
pub const SMSG_LOGOUT_CANCEL_ACK: u16 = 0x004f;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Packet {
    Response { reason: u32, instant: u8 },
    Complete,
    CancelAck,
}

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_LOGOUT_RESPONSE | SMSG_LOGOUT_COMPLETE | SMSG_LOGOUT_CANCEL_ACK
    )
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    match (opcode, body) {
        (SMSG_LOGOUT_RESPONSE, [a, b, c, d, instant]) => Ok(Packet::Response {
            reason: u32::from_le_bytes([*a, *b, *c, *d]),
            instant: *instant,
        }),
        (SMSG_LOGOUT_COMPLETE, []) => Ok(Packet::Complete),
        (SMSG_LOGOUT_CANCEL_ACK, []) => Ok(Packet::CancelAck),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Wrath logout opcode/body",
        )),
    }
}

pub fn encode_request() -> (u32, Vec<u8>) {
    (CMSG_LOGOUT_REQUEST, Vec::new())
}
pub fn encode_cancel() -> (u32, Vec<u8>) {
    (CMSG_LOGOUT_CANCEL, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_preserves_unknown_reason_and_every_instant_byte() {
        for instant in 0..=255 {
            assert_eq!(
                decode(0x4c, &[0x12, 0x34, 0x56, 0x78, instant]).unwrap(),
                Packet::Response {
                    reason: 0x78563412,
                    instant
                }
            );
        }
    }

    #[test]
    fn all_lifecycle_bodies_are_exact_and_opcodes_native() {
        for len in 0..=10 {
            assert_eq!(decode(0x4c, &vec![0; len]).is_ok(), len == 5);
            for op in [0x4d, 0x4f] {
                assert_eq!(decode(op, &vec![0; len]).is_ok(), len == 0);
            }
        }
        assert_eq!(decode(0x4d, &[]).unwrap(), Packet::Complete);
        assert_eq!(decode(0x4f, &[]).unwrap(), Packet::CancelAck);
        assert!(!handles(0x4e));
        assert!(decode(0x4e, &[]).is_err());
        assert_eq!(encode_request(), (0x4b, vec![]));
        assert_eq!(encode_cancel(), (0x4e, vec![]));
    }
}
