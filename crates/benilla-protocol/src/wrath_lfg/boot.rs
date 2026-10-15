//! Boot snapshot, AzerothCore 7f12e89e LFGHandler.cpp::SendLfgBootProposalUpdate.
//! Native handler 0x0055bdc0 reads all fields even when in_progress is zero.
//! Its 256-byte string destination is not an established protocol length cap.
use super::invalid;
use crate::wrath_session::{cstring, u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_LFG_BOOT_PROPOSAL_UPDATE: u16 = 0x036d;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boot {
    pub in_progress: u8,
    pub did_vote: u8,
    pub agree: u8,
    pub victim: u64,
    pub votes: u32,
    pub agrees: u32,
    pub seconds_left: u32,
    pub votes_needed: u32,
    pub reason: String,
}
pub fn decode(body: &[u8]) -> io::Result<Boot> {
    let mut r = Cursor::new(body);
    let value = Boot {
        in_progress: u8_le(&mut r)?,
        did_vote: u8_le(&mut r)?,
        agree: u8_le(&mut r)?,
        victim: u64_le(&mut r)?,
        votes: u32_le(&mut r)?,
        agrees: u32_le(&mut r)?,
        seconds_left: u32_le(&mut r)?,
        votes_needed: u32_le(&mut r)?,
        reason: cstring(&mut r)?,
    };
    if r.position() as usize != body.len() {
        return Err(invalid("trailing LFG boot bytes"));
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut body = vec![
            0x80, 0x81, 0x82, 8, 7, 6, 5, 4, 3, 2, 0xf1, 0xff, 0xff, 0xff, 0xff, 4, 3, 2, 1, 0, 0,
            0, 0x80, 0x44, 0x33, 0x22, 0x11,
        ];
        body.extend("raison é\0".as_bytes());
        body
    }
    #[test]
    fn independent_boot_fixture_preserves_raw_flags_guid_words_and_reason() {
        let expected = Boot {
            in_progress: 0x80,
            did_vote: 0x81,
            agree: 0x82,
            victim: 0xf102030405060708,
            votes: u32::MAX,
            agrees: 0x01020304,
            seconds_left: 0x80000000,
            votes_needed: 0x11223344,
            reason: "raison é".into(),
        };
        assert_eq!(decode(&fixture()).unwrap(), expected);
        assert!(super::super::handles(0x036d));
        assert_eq!(
            super::super::decode(0x036d, &fixture()).unwrap(),
            super::super::Packet::Boot(expected)
        );
    }
    #[test]
    fn inactive_boot_is_unconditional_and_reason_has_no_native_buffer_cap() {
        for reason in [String::new(), "x".repeat(256), "x".repeat(1024)] {
            let mut body = vec![0; 27];
            body.extend(reason.as_bytes());
            body.push(0);
            let value = decode(&body).unwrap();
            assert_eq!(value.in_progress, 0);
            assert_eq!(value.reason, reason);
            assert_eq!(value.victim, 0);
            assert_eq!(value.votes_needed, 0);
        }
        assert!(decode(&[0]).is_err());
    }
    #[test]
    fn boot_rejects_all_truncations_missing_terminator_and_trailing_data() {
        let body = fixture();
        for cut in 0..body.len() {
            assert!(
                super::super::decode(0x036d, &body[..cut]).is_err(),
                "cut {cut}"
            );
        }
        let mut trailing = body;
        trailing.push(0);
        assert!(decode(&trailing).is_err());
    }
}
