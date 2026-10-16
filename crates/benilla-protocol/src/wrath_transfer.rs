//! Build-12340 far-transfer framing (AzerothCore Player.cpp:1609, 1629, 11958).
//! Transport poses are relative; decoding them does not grant movement authority.
use std::io;

pub const SMSG_TRANSFER_PENDING: u16 = 0x003f;
pub const SMSG_NEW_WORLD: u16 = 0x003e;
pub const SMSG_TRANSFER_ABORTED: u16 = 0x0040;
pub const MSG_MOVE_WORLDPORT_ACK: u32 = 0x00dc;

/// Local capability, never serialized. Both fields must match the observed transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub epoch: u64,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Pending {
        map_id: u32,
        transport: Option<Transport>,
    },
    NewWorld {
        map_id: u32,
        position: [f32; 3],
        orientation: f32,
    },
    Aborted {
        map_id: u32,
        reason: u8,
        argument: Option<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transport {
    pub entry: u32,
    pub source_map: u32,
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid Wrath transfer framing");
    let word = |offset| u32::from_le_bytes(body[offset..offset + 4].try_into().unwrap());
    match opcode {
        SMSG_TRANSFER_PENDING if matches!(body.len(), 4 | 12) => Ok(Packet::Pending {
            map_id: word(0),
            transport: (body.len() == 12).then(|| Transport {
                entry: word(4),
                source_map: word(8),
            }),
        }),
        SMSG_NEW_WORLD if body.len() == 20 => Ok(Packet::NewWorld {
            map_id: word(0),
            position: [
                f32::from_bits(word(4)),
                f32::from_bits(word(8)),
                f32::from_bits(word(12)),
            ],
            orientation: f32::from_bits(word(16)),
        }),
        SMSG_TRANSFER_ABORTED if body.len() >= 5 => {
            let has_argument = matches!(body[4], 0x07..=0x09);
            if body.len() != 5 + usize::from(has_argument) {
                return Err(invalid());
            }
            Ok(Packet::Aborted {
                map_id: word(0),
                reason: body[4],
                argument: has_argument.then(|| body[5]),
            })
        }
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_preserves_transport_and_source_map() {
        let body = [8u32, 99, 7]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            decode(SMSG_TRANSFER_PENDING, &body).unwrap(),
            Packet::Pending {
                map_id: 8,
                transport: Some(Transport {
                    entry: 99,
                    source_map: 7
                })
            }
        );
        assert_eq!(
            decode(SMSG_TRANSFER_PENDING, &body[..4]).unwrap(),
            Packet::Pending {
                map_id: 8,
                transport: None
            }
        );
        for len in 0..=13 {
            if matches!(len, 4 | 12) {
                continue;
            }
            let mut malformed = body.clone();
            malformed.resize(len, 0);
            assert!(
                decode(SMSG_TRANSFER_PENDING, &malformed).is_err(),
                "length {len}"
            );
        }
    }

    #[test]
    fn abort_all_raw_reasons_and_conditional_arguments() {
        for reason in 0..=255u8 {
            let mut body = 42u32.to_le_bytes().to_vec();
            body.push(reason);
            let argument = matches!(reason, 7..=9).then_some(0xfe);
            if let Some(arg) = argument {
                body.push(arg);
            }
            assert_eq!(
                decode(SMSG_TRANSFER_ABORTED, &body).unwrap(),
                Packet::Aborted {
                    map_id: 42,
                    reason,
                    argument
                }
            );
            for len in 0..body.len() {
                assert!(decode(SMSG_TRANSFER_ABORTED, &body[..len]).is_err());
            }
            body.push(0);
            assert!(decode(SMSG_TRANSFER_ABORTED, &body).is_err());
        }
    }

    #[test]
    fn new_world_exact_framing_raw_pose() {
        let body = [
            571u32,
            1.25f32.to_bits(),
            (-9.5f32).to_bits(),
            22f32.to_bits(),
            0.75f32.to_bits(),
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
        assert_eq!(
            decode(SMSG_NEW_WORLD, &body).unwrap(),
            Packet::NewWorld {
                map_id: 571,
                position: [1.25, -9.5, 22.0],
                orientation: 0.75
            }
        );
        for len in 0..20 {
            assert!(decode(SMSG_NEW_WORLD, &body[..len]).is_err());
        }
        let mut tail = body;
        tail.push(0);
        assert!(decode(SMSG_NEW_WORLD, &tail).is_err());
        assert!(decode(0xffff, &[]).is_err());
    }
}
