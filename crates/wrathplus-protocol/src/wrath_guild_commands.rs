//! Core guild requests verified against AzerothCore GuildHandler/GuildPackets.cpp.
//! Rank-save 0x0231 (`wrath_guild_rank`) and bank commands live elsewhere.
use std::io;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Query {
        guild_id: u32,
    },
    Info,
    Roster,
    Invite {
        name: Vec<u8>,
    },
    Accept,
    Decline,
    Promote {
        name: Vec<u8>,
    },
    Demote {
        name: Vec<u8>,
    },
    Remove {
        name: Vec<u8>,
    },
    Motd {
        text: Vec<u8>,
    },
    /// `CMSG_GUILD_CREATE` (`GuildCreate::Read`).
    Create {
        name: Vec<u8>,
    },
    /// `CMSG_GUILD_LEAVE`, empty.
    Leave,
    /// `CMSG_GUILD_DISBAND`, empty.
    Disband,
    /// `CMSG_GUILD_LEADER` (`GuildSetGuildMaster::Read`).
    Leader {
        name: Vec<u8>,
    },
    /// `CMSG_GUILD_ADD_RANK`.
    AddRank {
        name: Vec<u8>,
    },
    /// `CMSG_GUILD_DEL_RANK`, empty: the server deletes the lowest rank.
    DelRank,
    /// `CMSG_GUILD_SET_PUBLIC_NOTE` / `CMSG_GUILD_SET_OFFICER_NOTE` (`GuildSetMemberNote::Read`).
    SetNote {
        officer: bool,
        name: Vec<u8>,
        note: Vec<u8>,
    },
    /// `CMSG_GUILD_INFO_TEXT` (`GuildUpdateInfoText::Read`).
    InfoText {
        text: Vec<u8>,
    },
    /// `MSG_SAVE_GUILD_EMBLEM` (`SaveGuildEmblem::Read`): the vendor, then emblem style, emblem
    /// colour, border style, border colour and background, each an `int32`.
    SaveEmblem {
        vendor: u64,
        design: [u32; 5],
    },
    /// `MSG_TABARDVENDOR_ACTIVATE`: the vendor (`HandleTabardVendorActivateOpcode`).
    TabardVendorActivate {
        vendor: u64,
    },
    /// `MSG_GUILD_EVENT_LOG_QUERY`, empty.
    EventLogQuery,
}
impl Command {
    pub fn opcode(&self) -> u32 {
        match self {
            Self::Query { .. } => 0x054,
            Self::Create { .. } => 0x081,
            Self::Invite { .. } => 0x082,
            Self::Accept => 0x084,
            Self::Decline => 0x085,
            Self::Info => 0x087,
            Self::Roster => 0x089,
            Self::Promote { .. } => 0x08b,
            Self::Demote { .. } => 0x08c,
            Self::Leave => 0x08d,
            Self::Remove { .. } => 0x08e,
            Self::Disband => 0x08f,
            Self::Leader { .. } => 0x090,
            Self::Motd { .. } => 0x091,
            Self::SaveEmblem { .. } => 0x1f1,
            Self::TabardVendorActivate { .. } => 0x1f2,
            Self::AddRank { .. } => 0x232,
            Self::DelRank => 0x233,
            Self::SetNote { officer: false, .. } => 0x234,
            Self::SetNote { officer: true, .. } => 0x235,
            Self::InfoText { .. } => 0x2fc,
            Self::EventLogQuery => 0x3ff,
        }
    }
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        fn cstr(body: &mut Vec<u8>, text: &[u8]) -> io::Result<()> {
            if text.contains(&0) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "guild string contains NUL",
                ));
            }
            body.extend_from_slice(text);
            body.push(0);
            Ok(())
        }
        let mut body = Vec::new();
        match self {
            Self::Query { guild_id } => body.extend(guild_id.to_le_bytes()),
            Self::Invite { name }
            | Self::Promote { name }
            | Self::Demote { name }
            | Self::Remove { name }
            | Self::Motd { text: name }
            | Self::Create { name }
            | Self::Leader { name }
            | Self::AddRank { name }
            | Self::InfoText { text: name } => cstr(&mut body, name)?,
            Self::SetNote { name, note, .. } => {
                cstr(&mut body, name)?;
                cstr(&mut body, note)?;
            }
            Self::SaveEmblem { vendor, design } => {
                body.extend(vendor.to_le_bytes());
                for word in design {
                    body.extend(word.to_le_bytes());
                }
            }
            Self::TabardVendorActivate { vendor } => body.extend(vendor.to_le_bytes()),
            Self::Info
            | Self::Roster
            | Self::Accept
            | Self::Decline
            | Self::Leave
            | Self::Disband
            | Self::DelRank
            | Self::EventLogQuery => {}
        }
        Ok(body)
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
            (
                Command::Create {
                    name: b"G".to_vec(),
                },
                0x81,
                b"G\0".to_vec(),
            ),
            (Command::Leave, 0x8d, vec![]),
            (Command::Disband, 0x8f, vec![]),
            (
                Command::Leader {
                    name: b"Bob".to_vec(),
                },
                0x90,
                b"Bob\0".to_vec(),
            ),
            (
                Command::AddRank {
                    name: b"Alt".to_vec(),
                },
                0x232,
                b"Alt\0".to_vec(),
            ),
            (Command::DelRank, 0x233, vec![]),
            (
                Command::SetNote {
                    officer: false,
                    name: b"Bob".to_vec(),
                    note: b"tank".to_vec(),
                },
                0x234,
                b"Bob\0tank\0".to_vec(),
            ),
            (
                Command::SetNote {
                    officer: true,
                    name: b"Bob".to_vec(),
                    note: vec![],
                },
                0x235,
                b"Bob\0\0".to_vec(),
            ),
            (
                Command::InfoText {
                    text: b"hi".to_vec(),
                },
                0x2fc,
                b"hi\0".to_vec(),
            ),
            (
                Command::SaveEmblem {
                    vendor: 7,
                    design: [1, 2, 3, 4, 5],
                },
                0x1f1,
                [
                    7u64.to_le_bytes().to_vec(),
                    [1u32, 2, 3, 4, 5]
                        .iter()
                        .flat_map(|w| w.to_le_bytes())
                        .collect(),
                ]
                .concat(),
            ),
            (
                Command::TabardVendorActivate { vendor: 9 },
                0x1f2,
                9u64.to_le_bytes().to_vec(),
            ),
            (Command::EventLogQuery, 0x3ff, vec![]),
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
            Command::SetNote {
                officer: true,
                name: b"a".to_vec(),
                note: vec![0],
            },
        ] {
            assert!(c.encode().is_err());
        }
    }
}
