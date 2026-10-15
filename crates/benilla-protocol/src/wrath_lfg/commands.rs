//! Build-12340 LFG queue command bodies, without eligibility or reducer policy.
//! Reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03,
//! LFGHandler.cpp::{HandleLfgLeaveOpcode,HandleLfgGetStatusOpcode,
//! HandleLfgSetRolesOpcode,HandleLfgSetCommentOpcode}.
use std::io;

pub const CMSG_LFG_LEAVE: u32 = 0x035d;
pub const CMSG_LFG_GET_STATUS: u32 = 0x0296;
pub const CMSG_SET_LFG_COMMENT: u32 = 0x0366;
pub const CMSG_LFG_SET_ROLES: u32 = 0x036a;

/// Raw role bits are preserved; the reducer decides which choices to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Leave,
    GetStatus,
    SetRoles(u8),
    SetComment(String),
}
impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
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
            Self::Leave | Self::GetStatus => Ok(Vec::new()),
            Self::SetRoles(roles) => Ok(vec![*roles]),
            Self::SetComment(comment) => {
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
