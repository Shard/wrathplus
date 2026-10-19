//! Build-12340 completed-quest query/reply; no quest-state interpretation.
//! AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03 QuestHandler.cpp:
//! HandleQueryQuestsCompleted writes count u32 then ordered quest IDs u32.
//! Wow.exe SHA256 c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c:
//! sender 005b1bb0 writes only opcode 0500 then sends via 006b0b50;
//! registration 005b6df0 binds 0501 to 005b5190, which reads count then IDs
//! through the same u32 reader 0047b3c0. The request body is empty.
use crate::wrath_session::u32_le;
use std::io::{self, Cursor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub quest_ids: Vec<u32>,
}

pub fn encode_query() -> (u32, Vec<u8>) {
    (0x0500, Vec::new())
}

pub fn decode(body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let count = u32_le(&mut r)? as usize;
    // Validate before allocating, including overflow-sized counts and trailing bytes.
    if count != (body.len() - 4) / 4 || !(body.len() - 4).is_multiple_of(4) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "completed quest count/framing",
        ));
    }
    let mut quest_ids = Vec::with_capacity(count);
    for _ in 0..count {
        quest_ids.push(u32_le(&mut r)?);
    }
    Ok(Packet { quest_ids })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_reply_keeps_order_duplicates_and_raw_ids() {
        let body = [
            4, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 0x78, 0x56, 0x34, 0x12, 255, 255, 255, 255,
        ];
        assert_eq!(
            decode(&body).unwrap().quest_ids,
            vec![u32::MAX, 0, 0x12345678, u32::MAX]
        );
        assert_eq!(decode(&[0; 4]).unwrap().quest_ids, vec![]);
        assert_eq!(encode_query(), (0x500, vec![]));
    }

    #[test]
    fn rejects_truncation_trailing_bytes_and_untrusted_counts() {
        let body = [2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0];
        for end in 0..body.len() {
            assert!(decode(&body[..end]).is_err());
        }
        for extra in [vec![0], vec![0; 4]] {
            let mut trailing = body.to_vec();
            trailing.extend(extra);
            assert!(decode(&trailing).is_err());
        }
        assert!(decode(&[255; 4]).is_err());
        assert!(decode(&[0, 0, 0, 0, 1, 0, 0, 0]).is_err());
    }
}
