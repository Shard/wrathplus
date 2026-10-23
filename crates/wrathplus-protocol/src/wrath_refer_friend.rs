//! Build-12340 refer-a-friend level grants.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: ReferAFriendHandler.cpp
//! (`HandleGrantLevel`, `HandleAcceptGrantLevel`), Player.h (`ReferAFriendError`). Strict framing.
//!
//! Both receipts are capture-only: 1.12 has no level grant.
use crate::wire::{read_packed_guid, write_packed_guid};
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 2] = [0x041f, 0x0421];

/// `ERR_REFER_A_FRIEND_NOT_IN_GROUP`: the one failure that names the target.
pub const ERR_NOT_IN_GROUP: u32 = 0x09;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_PROPOSE_LEVEL_GRANT`: `granter` offers us a level; accept with
    /// [`Command::AcceptLevelGrant`].
    ProposeLevelGrant { granter: u64 },
    /// `SMSG_REFER_A_FRIEND_FAILURE`: a `ReferAFriendError`, with the target's name only for
    /// [`ERR_NOT_IN_GROUP`].
    Failure { error: u32, name: Option<String> },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

/// A NUL-terminated string; the terminator is required.
fn cstring(r: &mut Cursor<&[u8]>) -> io::Result<String> {
    let rest = &r.get_ref()[r.position() as usize..];
    let end = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| invalid("unterminated string"))?;
    let text = String::from_utf8_lossy(&rest[..end]).into_owned();
    r.set_position(r.position() + end as u64 + 1);
    Ok(text)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let packet = match opcode {
        0x041f => Packet::ProposeLevelGrant {
            granter: read_packed_guid(r)?,
        },
        0x0421 => {
            let error = u32_(r)?;
            let name = (error == ERR_NOT_IN_GROUP)
                .then(|| cstring(r))
                .transpose()?;
            Packet::Failure { error, name }
        }
        _ => return Err(invalid("refer-a-friend opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("refer-a-friend extent"));
    }
    Ok(SessionEvent::WrathReferFriend { packet })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_GRANT_LEVEL`: offer a level to a recruited friend (packed guid).
    GrantLevel { target: u64 },
    /// `CMSG_ACCEPT_LEVEL_GRANT`: accept the level `granter` proposed (packed guid).
    AcceptLevelGrant { granter: u64 },
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let (opcode, guid) = match command {
        Command::GrantLevel { target } => (0x040d, *target),
        Command::AcceptLevelGrant { granter } => (0x0420, *granter),
    };
    let mut body = Vec::with_capacity(9);
    write_packed_guid(guid, &mut body).expect("Vec writes are infallible");
    (opcode, body)
}

#[cfg(test)]
mod tests;
