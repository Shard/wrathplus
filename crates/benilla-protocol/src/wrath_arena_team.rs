//! Native arena-team transport, AzerothCore Battlegrounds/ArenaTeam.cpp::Roster,
//! Query, SendStats and Handlers/ArenaTeamHandler.cpp. No inferred team/UI state.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub guid: u64,
    pub online: u8,
    pub name: Vec<u8>,
    pub captain_flag: u32,
    pub level: u8,
    pub class: u8,
    pub week_games: u32,
    pub week_wins: u32,
    pub season_games: u32,
    pub season_wins: u32,
    pub personal_rating: u32,
    /// Two uninterpreted IEEE-754 words when the roster discriminator is nonzero.
    pub extra_bits: Option<[u32; 2]>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packet {
    Roster {
        team_id: u32,
        discriminator: u8,
        team_type: u32,
        members: Vec<Member>,
    },
    Query {
        team_id: u32,
        name: Vec<u8>,
        team_type: u32,
        emblem: [u32; 5],
    },
    Stats {
        team_id: u32,
        rating: u32,
        week_games: u32,
        week_wins: u32,
        season_games: u32,
        season_wins: u32,
        rank: u32,
    },
    Invite {
        inviter: Vec<u8>,
        team: Vec<u8>,
    },
}
fn string(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let b = u8_le(r)?;
        if b == 0 {
            return Ok(bytes);
        }
        bytes.push(b);
    }
}
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        0x034c => {
            let team_id = u32_le(&mut r)?;
            let name = string(&mut r)?;
            let team_type = u32_le(&mut r)?;
            let mut emblem = [0; 5];
            for word in &mut emblem {
                *word = u32_le(&mut r)?;
            }
            Packet::Query {
                team_id,
                name,
                team_type,
                emblem,
            }
        }
        0x034e => {
            let team_id = u32_le(&mut r)?;
            let discriminator = u8_le(&mut r)?;
            let count = u32_le(&mut r)?;
            let team_type = u32_le(&mut r)?;
            // Empty string + all fixed fields: 36 bytes; nonzero discriminator adds 8.
            let min_row = if discriminator == 0 { 36 } else { 44 };
            if u64::from(count) > (body.len() - r.position() as usize) as u64 / min_row {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "arena member count exceeds body",
                ));
            }
            let mut members = Vec::new();
            for _ in 0..count {
                let guid = u64_le(&mut r)?;
                let online = u8_le(&mut r)?;
                let name = string(&mut r)?;
                let captain_flag = u32_le(&mut r)?;
                let level = u8_le(&mut r)?;
                let class = u8_le(&mut r)?;
                let week_games = u32_le(&mut r)?;
                let week_wins = u32_le(&mut r)?;
                let season_games = u32_le(&mut r)?;
                let season_wins = u32_le(&mut r)?;
                let personal_rating = u32_le(&mut r)?;
                let extra_bits = if discriminator == 0 {
                    None
                } else {
                    Some([u32_le(&mut r)?, u32_le(&mut r)?])
                };
                members.push(Member {
                    guid,
                    online,
                    name,
                    captain_flag,
                    level,
                    class,
                    week_games,
                    week_wins,
                    season_games,
                    season_wins,
                    personal_rating,
                    extra_bits,
                });
            }
            Packet::Roster {
                team_id,
                discriminator,
                team_type,
                members,
            }
        }
        0x0350 => Packet::Invite {
            inviter: string(&mut r)?,
            team: string(&mut r)?,
        },
        0x035b => Packet::Stats {
            team_id: u32_le(&mut r)?,
            rating: u32_le(&mut r)?,
            week_games: u32_le(&mut r)?,
            week_wins: u32_le(&mut r)?,
            season_games: u32_le(&mut r)?,
            season_wins: u32_le(&mut r)?,
            rank: u32_le(&mut r)?,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported arena-team opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "arena-team trailing bytes",
        ));
    }
    Ok(packet)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Query { team_id: u32 },
    Roster { team_id: u32 },
    Invite { team_id: u32, name: Vec<u8> },
    Accept,
    Decline,
    /// `CMSG_ARENA_TEAM_LEAVE`.
    Leave { team_id: u32 },
    /// `CMSG_ARENA_TEAM_REMOVE`: a member by name.
    Remove { team_id: u32, name: Vec<u8> },
    /// `CMSG_ARENA_TEAM_DISBAND`.
    Disband { team_id: u32 },
    /// `CMSG_ARENA_TEAM_LEADER`: the new captain by name.
    Leader { team_id: u32, name: Vec<u8> },
}
impl Command {
    pub fn opcode(&self) -> u32 {
        match self {
            Self::Query { .. } => 0x034b,
            Self::Roster { .. } => 0x034d,
            Self::Invite { .. } => 0x034f,
            Self::Accept => 0x0351,
            Self::Decline => 0x0352,
            Self::Leave { .. } => 0x0353,
            Self::Remove { .. } => 0x0354,
            Self::Disband { .. } => 0x0355,
            Self::Leader { .. } => 0x0356,
        }
    }
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        match self {
            Self::Query { team_id }
            | Self::Roster { team_id }
            | Self::Leave { team_id }
            | Self::Disband { team_id } => {
                Ok(team_id.to_le_bytes().to_vec())
            }
            Self::Invite { team_id, name }
            | Self::Remove { team_id, name }
            | Self::Leader { team_id, name } => {
                if name.contains(&0) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "arena invite contains NUL",
                    ));
                }
                let mut body = team_id.to_le_bytes().to_vec();
                body.extend(name);
                body.push(0);
                Ok(body)
            }
            _ => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
#[path = "wrath_arena_team_tests.rs"]
mod tests;
