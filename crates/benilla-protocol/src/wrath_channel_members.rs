//! Build-12340 channel membership notices for a watched channel and the member count. Wire
//! reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03 `Chat/Channels/Channel.cpp`
//! (`SMSG_USERLIST_ADD`/`UPDATE`: GUID, member flags, channel flags, member count, name;
//! `SMSG_USERLIST_REMOVE`: GUID, channel flags, count, name) and `Handlers/ChannelHandler.cpp`
//! (`SMSG_CHANNEL_MEMBER_COUNT`: name, channel flags, count). Strict framing.
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 4] = [0x03d5, 0x03f0, 0x03f1, 0x03f2];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    Add { guid: u64, flags: u8 },
    Update { guid: u64, flags: u8 },
    Remove { guid: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// A watched channel's roster changed.
    Roster {
        channel: String,
        delta: Delta,
        channel_flags: u8,
        count: u32,
    },
    MemberCount {
        channel: String,
        channel_flags: u8,
        count: u32,
    },
}

fn bytes<const N: usize>(r: &mut Cursor<&[u8]>) -> io::Result<[u8; N]> {
    let mut b = [0; N];
    r.read_exact(&mut b)?;
    Ok(b)
}
fn cstr(r: &mut Cursor<&[u8]>) -> io::Result<String> {
    let mut out = Vec::new();
    loop {
        match bytes::<1>(r)?[0] {
            0 => return Ok(String::from_utf8_lossy(&out).into_owned()),
            b => out.push(b),
        }
    }
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let packet = match opcode {
        0x03d5 => Packet::MemberCount {
            channel: cstr(r)?,
            channel_flags: bytes::<1>(r)?[0],
            count: u32::from_le_bytes(bytes(r)?),
        },
        0x03f0 | 0x03f2 => {
            let guid = u64::from_le_bytes(bytes(r)?);
            let flags = bytes::<1>(r)?[0];
            let channel_flags = bytes::<1>(r)?[0];
            let count = u32::from_le_bytes(bytes(r)?);
            let delta = if opcode == 0x03f0 {
                Delta::Add { guid, flags }
            } else {
                Delta::Update { guid, flags }
            };
            Packet::Roster {
                channel: cstr(r)?,
                delta,
                channel_flags,
                count,
            }
        }
        0x03f1 => {
            let guid = u64::from_le_bytes(bytes(r)?);
            let channel_flags = bytes::<1>(r)?[0];
            let count = u32::from_le_bytes(bytes(r)?);
            Packet::Roster {
                channel: cstr(r)?,
                delta: Delta::Remove { guid },
                channel_flags,
                count,
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "channel members opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "channel members extent",
        ));
    }
    Ok(packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deltas_and_counts_decode_strictly() {
        let mut add = 9u64.to_le_bytes().to_vec();
        add.extend([2, 0x18]);
        add.extend(5u32.to_le_bytes());
        add.extend(b"World\0");
        assert_eq!(
            decode(0x3f0, &add).unwrap(),
            Packet::Roster {
                channel: "World".into(),
                delta: Delta::Add { guid: 9, flags: 2 },
                channel_flags: 0x18,
                count: 5
            }
        );
        assert!(matches!(
            decode(0x3f2, &add).unwrap(),
            Packet::Roster {
                delta: Delta::Update { guid: 9, flags: 2 },
                ..
            }
        ));
        let mut remove = 9u64.to_le_bytes().to_vec();
        remove.push(0x18);
        remove.extend(4u32.to_le_bytes());
        remove.extend(b"World\0");
        assert!(matches!(
            decode(0x3f1, &remove).unwrap(),
            Packet::Roster {
                delta: Delta::Remove { guid: 9 },
                count: 4,
                ..
            }
        ));
        let mut count = b"Trade\0".to_vec();
        count.push(0x1c);
        count.extend(77u32.to_le_bytes());
        assert_eq!(
            decode(0x3d5, &count).unwrap(),
            Packet::MemberCount {
                channel: "Trade".into(),
                channel_flags: 0x1c,
                count: 77
            }
        );
        for (opcode, body) in [(0x3f0, &add), (0x3f1, &remove), (0x3d5, &count)] {
            for end in 0..body.len() {
                assert!(decode(opcode, &body[..end]).is_err(), "{opcode:#x} {end}");
            }
        }
        assert!(decode(0x3f0, &[add.clone(), vec![0]].concat()).is_err());
    }
}
