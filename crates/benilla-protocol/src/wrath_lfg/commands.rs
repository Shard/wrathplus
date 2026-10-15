//! Build-12340 LFG queue command bodies, without eligibility or reducer policy.
//! Reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03,
//! LFGHandler.cpp::{HandleLfgLeaveOpcode,HandleLfgGetStatusOpcode,
//! HandleLfgSetRolesOpcode,HandleLfgSetCommentOpcode}.
use std::io;

pub const CMSG_LFG_PROPOSAL_RESULT: u32 = 0x0362;
pub const CMSG_LFG_JOIN: u32 = 0x035c;
pub const CMSG_LFG_LEAVE: u32 = 0x035d;
pub const CMSG_LFG_GET_STATUS: u32 = 0x0296;
pub const CMSG_SET_LFG_COMMENT: u32 = 0x0366;
pub const CMSG_LFG_SET_ROLES: u32 = 0x036a;

/// Server-compatibility bound: AC LFGPackets.h uses Array<uint32, 50>.
/// This is not a native-client selection policy; no native 50-slot check is proven.
pub const MAX_JOIN_SLOTS: usize = 50;

/// Native sender 0x005572d0 writes full packed entries and literal needs count 3.
/// Selection setter 0x00559f50 packs (type << 24) | (id & 0x00ffffff).
/// AC LFGPackets.cpp::LFGJoin::Read confirms the same wire layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Join {
    pub roles: u32,
    pub no_partial_clear: bool,
    pub achievements: bool,
    /// Full packed entries in caller order; never masked, deduplicated or sorted.
    pub slots: Vec<u32>,
    pub needs: [u8; 3],
    pub comment: String,
}

fn comment_bytes(comment: &str) -> io::Result<Vec<u8>> {
    if comment.as_bytes().contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "embedded LFG comment NUL",
        ));
    }
    let mut bytes = comment.as_bytes().to_vec();
    bytes.push(0);
    Ok(bytes)
}

/// Raw role bits are preserved; the reducer decides which choices to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Reply to the active proposal ID, never the dungeon entry.
    /// Native sender 0x00554450 writes u32 ID + canonical accept byte;
    /// bindings 0x005545e0/0x00554600 supply true/false respectively.
    /// Active-ID selection, guards and lifecycle remain App/UI policy.
    ProposalResult {
        proposal_id: u32,
        accept: bool,
    },
    Join(Join),
    Leave,
    GetStatus,
    SetRoles(u8),
    SetComment(String),
}
impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
            Self::ProposalResult { .. } => CMSG_LFG_PROPOSAL_RESULT,
            Self::Join(_) => CMSG_LFG_JOIN,
            Self::Leave => CMSG_LFG_LEAVE,
            Self::GetStatus => CMSG_LFG_GET_STATUS,
            Self::SetRoles(_) => CMSG_LFG_SET_ROLES,
            Self::SetComment(_) => CMSG_SET_LFG_COMMENT,
        }
    }
    /// Encode the entire body or fail before handing any bytes to the transport.
    /// Comments are UTF-8 C strings; an embedded NUL cannot be represented.
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        match self {
            Self::ProposalResult {
                proposal_id,
                accept,
            } => {
                let mut bytes = proposal_id.to_le_bytes().to_vec();
                bytes.push(u8::from(*accept));
                Ok(bytes)
            }
            Self::Leave | Self::GetStatus => Ok(Vec::new()),
            Self::SetRoles(roles) => Ok(vec![*roles]),
            Self::SetComment(comment) => comment_bytes(comment),
            Self::Join(join) => {
                if join.slots.len() > MAX_JOIN_SLOTS {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "LFG JOIN exceeds 50-slot server-compatibility limit",
                    ));
                }
                let comment = comment_bytes(&join.comment)?;
                let mut bytes = join.roles.to_le_bytes().to_vec();
                bytes.extend([
                    u8::from(join.no_partial_clear),
                    u8::from(join.achievements),
                    join.slots.len() as u8,
                ]);
                for slot in &join.slots {
                    bytes.extend(slot.to_le_bytes());
                }
                bytes.push(3);
                bytes.extend(join.needs);
                bytes.extend(comment);
                Ok(bytes)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_queue_command_fixtures_preserve_opcode_and_width() {
        assert_eq!(
            (Command::Leave.opcode(), Command::Leave.encode().unwrap()),
            (0x035d, vec![])
        );
        assert_eq!(
            (
                Command::GetStatus.opcode(),
                Command::GetStatus.encode().unwrap()
            ),
            (0x0296, vec![])
        );
        for roles in 0..=255u8 {
            let command = Command::SetRoles(roles);
            assert_eq!(command.opcode(), 0x036a);
            assert_eq!(command.encode().unwrap(), vec![roles]);
        }
    }

    #[test]
    fn comments_keep_empty_utf8_and_terminator_without_truncating_nul() {
        for (comment, body) in [
            ("", &b"\0"[..]),
            ("tank", &b"tank\0"[..]),
            ("é", &[0xc3, 0xa9, 0][..]),
        ] {
            let command = Command::SetComment(comment.into());
            assert_eq!(command.opcode(), 0x0366);
            assert_eq!(command.encode().unwrap(), body);
        }
        for comment in ["\0", "before\0after", "trailing\0"] {
            assert_eq!(
                Command::SetComment(comment.into())
                    .encode()
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}

#[cfg(test)]
mod join_tests {
    use super::*;
    fn join(slots: Vec<u32>) -> Join {
        Join {
            roles: 0x80ff010f,
            no_partial_clear: true,
            achievements: false,
            slots,
            needs: [0, 0x80, 0xff],
            comment: "é".into(),
        }
    }
    #[test]
    fn join_independent_native_fixture_preserves_packed_entries_and_duplicates() {
        let command = Command::Join(join(vec![0xab123456, 0x01020304, 0xab123456]));
        assert_eq!(command.opcode(), 0x035c);
        assert_eq!(
            command.encode().unwrap(),
            vec![
                0x0f, 0x01, 0xff, 0x80, 1, 0, 3, 0x56, 0x34, 0x12, 0xab, 4, 3, 2, 1, 0x56, 0x34,
                0x12, 0xab, 3, 0, 0x80, 0xff, 0xc3, 0xa9, 0
            ]
        );
    }
    #[test]
    fn join_empty_selection_flags_and_comment_are_not_eligibility_policy() {
        for partial in [false, true] {
            for achievements in [false, true] {
                let mut value = join(vec![]);
                value.roles = 0;
                value.no_partial_clear = partial;
                value.achievements = achievements;
                value.needs = [0; 3];
                value.comment.clear();
                assert_eq!(
                    Command::Join(value).encode().unwrap(),
                    vec![
                        0,
                        0,
                        0,
                        0,
                        u8::from(partial),
                        u8::from(achievements),
                        0,
                        3,
                        0,
                        0,
                        0,
                        0
                    ]
                );
            }
        }
    }
    #[test]
    fn join_server_bound_accepts_50_rejects_51_without_truncation() {
        let mut value = join(vec![0xffffffff; 50]);
        let bytes = Command::Join(value.clone()).encode().unwrap();
        assert_eq!(bytes[6], 50);
        assert_eq!(&bytes[7..207], &[0xff; 200]);
        assert_eq!(&bytes[207..], &[3, 0, 0x80, 0xff, 0xc3, 0xa9, 0]);
        for count in [51, 255, 256] {
            value.slots.resize(count, 0xffffffff);
            let err = Command::Join(value.clone()).encode().unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
            assert!(err.to_string().contains("50-slot server-compatibility"));
            assert_eq!(value.slots.len(), count);
        }
    }
    #[test]
    fn join_rejects_embedded_comment_nul() {
        let mut value = join(vec![1]);
        value.comment = "before\0after".into();
        assert_eq!(
            Command::Join(value).encode().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[cfg(test)]
mod proposal_result_tests {
    use super::*;
    #[test]
    fn independent_accept_reject_fixtures_use_proposal_identity_and_bool_width() {
        for (id, accept, body) in [
            (0x80abcdef, true, vec![0xef, 0xcd, 0xab, 0x80, 1]),
            (0x01020304, false, vec![4, 3, 2, 1, 0]),
            (0, false, vec![0; 5]),
            (u32::MAX, true, vec![0xff, 0xff, 0xff, 0xff, 1]),
        ] {
            let command = Command::ProposalResult {
                proposal_id: id,
                accept,
            };
            assert_eq!(command.opcode(), 0x0362);
            assert_eq!(command.encode().unwrap(), body);
        }
    }
}
