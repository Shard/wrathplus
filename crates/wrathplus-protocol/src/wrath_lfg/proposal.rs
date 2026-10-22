//! Native L3 proposal body. AzerothCore 7f12e89e LFGHandler.cpp
//! SendLfgUpdateProposal. Rows are anonymous, ordered, and raw-byte preserving.
//! Native handler 0x0055bdc0, branch 0x0055c76c, confirms the 15-byte header
//! and nine-byte rows, but stops storing/reading after five rows. This codec
//! retains every structurally valid counted row (u8 count); App/UI presentation
//! projects only the first five in wire order, without discarding protocol data.
use super::{check_rows, invalid};
use crate::wrath_session::{u32_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_LFG_PROPOSAL_UPDATE: u16 = 0x0361;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub roles: u32,
    pub is_self: u8,
    pub in_dungeon: u8,
    pub same_group: u8,
    pub answered: u8,
    pub accepted: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// Full packed dungeon entry, not a proposal identity.
    pub entry: u32,
    pub state: u8,
    pub id: u32,
    pub encounter_mask: u32,
    pub silent: u8,
    /// No GUID or name occurs on the wire; do not infer roster identities.
    pub members: Vec<Member>,
}
pub fn decode(body: &[u8]) -> io::Result<Proposal> {
    let mut r = Cursor::new(body);
    let entry = u32_le(&mut r)?;
    let state = u8_le(&mut r)?;
    let id = u32_le(&mut r)?;
    let encounter_mask = u32_le(&mut r)?;
    let silent = u8_le(&mut r)?;
    let count = u8_le(&mut r)? as usize;
    check_rows(&r, count, 9)?;
    let mut members = Vec::with_capacity(count);
    for _ in 0..count {
        members.push(Member {
            roles: u32_le(&mut r)?,
            is_self: u8_le(&mut r)?,
            in_dungeon: u8_le(&mut r)?,
            same_group: u8_le(&mut r)?,
            answered: u8_le(&mut r)?,
            accepted: u8_le(&mut r)?,
        });
    }
    if r.position() as usize != body.len() {
        return Err(invalid("trailing LFG proposal bytes"));
    }
    Ok(Proposal {
        entry,
        state,
        id,
        encounter_mask,
        silent,
        members,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        vec![
            0x56, 0x34, 0x12, 0xab, 0xfe, 4, 3, 2, 1, 0, 0, 0, 0x80, 0x81, 2, 0xff, 0xff, 0xff,
            0xff, 0x80, 0x81, 0x82, 0x83, 0x84, 8, 0, 0, 0, 0, 1, 0, 1, 0,
        ]
    }
    #[test]
    fn independent_proposal_fixture_preserves_raw_header_and_anonymous_rows() {
        let expected = Proposal {
            entry: 0xab123456,
            state: 0xfe,
            id: 0x01020304,
            encounter_mask: 0x80000000,
            silent: 0x81,
            members: vec![
                Member {
                    roles: u32::MAX,
                    is_self: 0x80,
                    in_dungeon: 0x81,
                    same_group: 0x82,
                    answered: 0x83,
                    accepted: 0x84,
                },
                Member {
                    roles: 8,
                    is_self: 0,
                    in_dungeon: 1,
                    same_group: 0,
                    answered: 1,
                    accepted: 0,
                },
            ],
        };
        assert_eq!(decode(&fixture()).unwrap(), expected);
        assert_eq!(
            super::super::decode(0x0361, &fixture()).unwrap(),
            super::super::Packet::Proposal(expected)
        );
        assert!(super::super::handles(0x0361));
    }
    #[test]
    fn proposal_preserves_rows_beyond_native_five_through_u8_count_boundary() {
        for count in [5u8, 6, 255] {
            let mut body = vec![0; 15];
            body[14] = count;
            for row in 0..count {
                // Independent nine-byte rows make truncation or reordering visible.
                body.extend([row, 0x12, 0x34, 0x80, row, 0x81, 0x82, 0x83, 0x84]);
            }
            let proposal = decode(&body).unwrap();
            assert_eq!(proposal.members.len(), count as usize);
            for (index, member) in proposal.members.iter().enumerate() {
                assert_eq!(member.roles, 0x80341200 | index as u32);
                assert_eq!(member.is_self, index as u8);
                assert_eq!(
                    (
                        member.in_dungeon,
                        member.same_group,
                        member.answered,
                        member.accepted
                    ),
                    (0x81, 0x82, 0x83, 0x84)
                );
            }
            body.pop();
            assert!(decode(&body).is_err());
        }
    }

    #[test]
    fn proposal_empty_is_explicit_and_strict() {
        let body = [0; 15];
        let value = decode(&body).unwrap();
        assert!(value.members.is_empty());
        assert_eq!(value.id, 0);
        for cut in 0..body.len() {
            assert!(decode(&body[..cut]).is_err());
        }
        let mut trailing = body.to_vec();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
    }
    #[test]
    fn proposal_truncation_trailing_and_forged_counts_fail_transactionally() {
        let body = fixture();
        for cut in 0..body.len() {
            assert!(
                super::super::decode(0x0361, &body[..cut]).is_err(),
                "cut {cut}"
            );
        }
        let mut trailing = body.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
        for count in [0, 1, 3, 255] {
            let mut bad = body.clone();
            bad[14] = count;
            assert!(decode(&bad).is_err(), "count {count}");
        }
    }
}
