//! Build-12340 WORLD_STATE_UI_TIMER_UPDATE transport, without clock interpretation.
//! AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03 MiscHandler.cpp
//! HandleWorldStateUITimerUpdate: empty request, response GameTime as u32.
//! Wow.exe SHA256 c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c:
//! 00548760 sends 04F6 without fields; 0052b550 registers 04F7 to 00526530.
//! Its 04F7 arm calls 0050f420 (one u32 via 0047b3c0), then 005486e0
//! computes a local clock offset. Preserve the raw word here, not that derived offset.
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub time: u32,
}

pub fn encode_query() -> (u32, Vec<u8>) {
    (0x04f6, Vec::new())
}

pub fn decode(body: &[u8]) -> io::Result<Packet> {
    let bytes: [u8; 4] = body.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "UI timer requires exactly one u32",
        )
    })?;
    Ok(Packet {
        time: u32::from_le_bytes(bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_reply_preserves_raw_word_and_empty_request() {
        assert_eq!(
            decode(&[0x78, 0x56, 0x34, 0x12]).unwrap(),
            Packet { time: 0x12345678 }
        );
        assert_eq!(decode(&[0; 4]).unwrap().time, 0);
        assert_eq!(decode(&[255; 4]).unwrap().time, u32::MAX);
        assert_eq!(encode_query(), (0x4f6, vec![]));
    }

    #[test]
    fn rejects_truncation_and_trailing_bytes() {
        for length in 0..9 {
            if length != 4 {
                assert!(decode(&vec![0; length]).is_err());
            }
        }
    }
}
