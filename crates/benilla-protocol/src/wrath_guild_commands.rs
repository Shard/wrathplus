//! Core guild requests verified against AzerothCore GuildHandler/GuildPackets.cpp.
//! Rank-save 0x0231 and bank commands deliberately live elsewhere.
use std::io;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Query { guild_id: u32 },
    Info,
    Roster,
    Invite { name: Vec<u8> },
    Accept,
    Decline,
    Promote { name: Vec<u8> },
    Demote { name: Vec<u8> },
    Remove { name: Vec<u8> },
    Motd { text: Vec<u8> },
}
impl Command {
    pub fn opcode(&self) -> u32 {
        match self {
            Self::Query { .. } => 0x054,
            Self::Invite { .. } => 0x082,
            Self::Accept => 0x084,
            Self::Decline => 0x085,
            Self::Info => 0x087,
            Self::Roster => 0x089,
            Self::Promote { .. } => 0x08b,
            Self::Demote { .. } => 0x08c,
            Self::Remove { .. } => 0x08e,
            Self::Motd { .. } => 0x091,
        }
    }
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        match self {
            Self::Query { guild_id } => Ok(guild_id.to_le_bytes().to_vec()),
            Self::Invite { name }
            | Self::Promote { name }
            | Self::Demote { name }
            | Self::Remove { name }
            | Self::Motd { text: name } => {
                if name.contains(&0) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "guild string contains NUL",
                    ));
                }
                let mut body = name.clone();
                body.push(0);
                Ok(body)
            }
            _ => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_have_exact_opcodes_and_bodies_and_reject_embedded_nuls() {
        for (c, op, body) in [
            (
                Command::Query {
                    guild_id: 0x01020304,
                },
                0x54,
                vec![4, 3, 2, 1],
            ),
            (Command::Info, 0x87, vec![]),
            (Command::Roster, 0x89, vec![]),
            (Command::Invite { name: vec![255] }, 0x82, vec![255, 0]),
            (Command::Accept, 0x84, vec![]),
            (Command::Decline, 0x85, vec![]),
            (Command::Promote { name: vec![] }, 0x8b, vec![0]),
            (
                Command::Demote {
                    name: b"name".to_vec(),
                },
                0x8c,
                b"name\0".to_vec(),
            ),
            (
                Command::Remove {
                    name: b"name".to_vec(),
                },
                0x8e,
                b"name\0".to_vec(),
            ),
            (Command::Motd { text: vec![] }, 0x91, vec![0]),
        ] {
            assert_eq!(c.opcode(), op);
            assert_eq!(c.encode().unwrap(), body);
        }
        for c in [
            Command::Invite { name: vec![0] },
            Command::Promote { name: vec![0] },
            Command::Demote { name: vec![0] },
            Command::Remove { name: vec![0] },
            Command::Motd { text: vec![0] },
        ] {
            assert!(c.encode().is_err());
        }
    }
}
