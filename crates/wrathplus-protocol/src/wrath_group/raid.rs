//! Build-12340 raid responses, AC GroupHandler.cpp::SendPartyResult and
//! HandleRaidReadyCheckOpcode; Group.cpp::{SetTargetIcon,SendTargetIconList}.
use super::{cstring, invalid, u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    CommandResult {
        operation: u32,
        member: String,
        result: u32,
        value: u32,
    },
    TargetSet {
        setter: u64,
        icon: u8,
        target: u64,
    },
    TargetList {
        entries: Vec<(u8, u64)>,
    },
    ReadyStart {
        initiator: u64,
    },
    ReadyConfirm {
        guid: u64,
        state: u8,
    },
    ReadyFinished,
}
pub const fn handles(opcode: u16) -> bool {
    matches!(opcode, 0x07f | 0x321 | 0x322 | 0x3ae | 0x3c6)
}
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        0x07f => Packet::CommandResult {
            operation: u32_le(&mut r)?,
            member: cstring(&mut r)?,
            result: u32_le(&mut r)?,
            value: u32_le(&mut r)?,
        },
        0x321 => match u8_le(&mut r)? {
            0 => Packet::TargetSet {
                setter: u64_le(&mut r)?,
                icon: u8_le(&mut r)?,
                target: u64_le(&mut r)?,
            },
            1 => {
                let mut entries = Vec::new();
                while (r.position() as usize) < body.len() {
                    entries.push((u8_le(&mut r)?, u64_le(&mut r)?));
                }
                Packet::TargetList { entries }
            }
            _ => return Err(invalid("raid target selector")),
        },
        0x322 => Packet::ReadyStart {
            initiator: u64_le(&mut r)?,
        },
        0x3ae => Packet::ReadyConfirm {
            guid: u64_le(&mut r)?,
            state: u8_le(&mut r)?,
        },
        0x3c6 => Packet::ReadyFinished,
        _ => return Err(invalid("raid opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("raid trailing bytes"));
    }
    Ok(packet)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_native_raid_bytes_and_strict_packet_boundaries() {
        let fixtures = [
            (
                0x07f,
                vec![2, 0, 0, 0, b'A', 0, 3, 0, 0, 0, 255, 255, 255, 255],
                Packet::CommandResult {
                    operation: 2,
                    member: "A".into(),
                    result: 3,
                    value: u32::MAX,
                },
            ),
            (
                0x321,
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 7, 8, 7, 6, 5, 4, 3, 2, 1],
                Packet::TargetSet {
                    setter: 0x0807060504030201,
                    icon: 7,
                    target: 0x0102030405060708,
                },
            ),
            (
                0x322,
                vec![1, 2, 3, 4, 5, 6, 7, 8],
                Packet::ReadyStart {
                    initiator: 0x0807060504030201,
                },
            ),
            (
                0x3ae,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 255],
                Packet::ReadyConfirm {
                    guid: 0x0807060504030201,
                    state: 255,
                },
            ),
            (0x3c6, vec![], Packet::ReadyFinished),
        ];
        for (opcode, bytes, expected) in fixtures {
            assert_eq!(decode(opcode, &bytes).unwrap(), expected);
            for end in 0..bytes.len() {
                assert!(decode(opcode, &bytes[..end]).is_err());
            }
            let mut extra = bytes.clone();
            extra.push(0);
            assert!(decode(opcode, &extra).is_err());
            assert_eq!(
                super::super::decode(opcode, &bytes).unwrap(),
                super::super::Packet::Raid(expected)
            );
        }
        assert_eq!(
            decode(0x321, &[1]).unwrap(),
            Packet::TargetList { entries: vec![] }
        );
        assert_eq!(
            decode(0x321, &[1, 7, 1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
            Packet::TargetList {
                entries: vec![(7, 0x0807060504030201)]
            }
        );
        for end in 2..10 {
            assert!(decode(0x321, &[1, 7, 1, 2, 3, 4, 5, 6, 7, 8][..end]).is_err());
        }
        assert!(decode(0x321, &[2]).is_err());
    }
}
