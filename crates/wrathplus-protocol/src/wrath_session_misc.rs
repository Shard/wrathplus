//! Build-12340 session odds and ends: the server-time query, mirror timers, the play-time warning,
//! inebriation and the feature-system status, plus the logout, emote, time-query and unlearn-skill
//! commands.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: QueryHandler.cpp
//! (`SendTimeQueryResponse`), QueryPackets.cpp (`TimeQueryResponse`), MiscPackets.cpp (mirror
//! timers, `PlayTimeWarning`, `CrossedInebriationThreshold`), CharacterHandler.cpp (feature-system
//! status at login), ChatHandler.cpp (`HandleEmoteOpcode`), SkillHandler.cpp
//! (`HandleUnlearnSkillOpcode`), MiscHandler.cpp (`HandlePlayerLogoutOpcode`). Strict framing.
//!
//! The time response and the mirror timers mean what 1.12's do and feed the shared events; the
//! time response's seconds until the daily quest reset (new in this build) is also captured. The
//! play-time warning, inebriation and feature-system status have no 1.12 counterpart.
use crate::types::MirrorTimerStart;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 7] = [0x01cf, 0x01d9, 0x01da, 0x01db, 0x02f5, 0x03c1, 0x03c9];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_QUERY_TIME_RESPONSE`: the server's unix time and the seconds until the daily quest
    /// reset (`GetQuestResetTime`).
    QueryTime {
        server_time: u32,
        daily_reset_in: u32,
    },
    /// `SMSG_PLAY_TIME_WARNING`: a `PlayTimeFlag` and the signed seconds left.
    PlayTimeWarning { flags: u32, remaining: i32 },
    /// `SMSG_CROSSED_INEBRIATION_THRESHOLD`: `player` crossed a drunk threshold, from `item` (0
    /// when not from an item).
    Inebriation {
        player: u64,
        threshold: u32,
        item: u32,
    },
    /// `SMSG_FEATURE_SYSTEM_STATUS`: the complaint system mode (AC 2, enabled with auto-ignore)
    /// and the voice-chat switch (AC 0).
    FeatureSystemStatus { complaints: u8, voice_chat: u8 },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u8_(r: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn u64_(r: &mut Cursor<&[u8]>) -> io::Result<u64> {
    let mut b = [0; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn flag(r: &mut Cursor<&[u8]>) -> io::Result<bool> {
    match u8_(r)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("boolean byte")),
    }
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let capture = |packet| SessionEvent::WrathSessionMisc { packet };
    let events = match opcode {
        0x01cf => {
            let (server_time, daily_reset_in) = (u32_(r)?, u32_(r)?);
            vec![
                SessionEvent::ServerUnixTime {
                    unix_time: server_time,
                },
                capture(Packet::QueryTime {
                    server_time,
                    daily_reset_in,
                }),
            ]
        }
        0x01d9 => vec![SessionEvent::MirrorTimerStart(MirrorTimerStart {
            kind: u32_(r)?,
            remaining_ms: u32_(r)?,
            duration_ms: u32_(r)?,
            scale: u32_(r)? as i32,
            paused: flag(r)?,
            spell_id: u32_(r)?,
        })],
        0x01da => vec![SessionEvent::MirrorTimerPause {
            kind: u32_(r)?,
            paused: flag(r)?,
        }],
        0x01db => vec![SessionEvent::MirrorTimerStop { kind: u32_(r)? }],
        0x02f5 => vec![capture(Packet::PlayTimeWarning {
            flags: u32_(r)?,
            remaining: u32_(r)? as i32,
        })],
        0x03c1 => vec![capture(Packet::Inebriation {
            player: u64_(r)?,
            threshold: u32_(r)?,
            item: u32_(r)?,
        })],
        0x03c9 => vec![capture(Packet::FeatureSystemStatus {
            complaints: u8_(r)?,
            voice_chat: u8_(r)?,
        })],
        _ => return Err(invalid("session misc opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("session misc extent"));
    }
    Ok(events)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_PLAYER_LOGOUT`: empty; AC's handler does nothing (logging out is
    /// `CMSG_LOGOUT_REQUEST`).
    PlayerLogout,
    /// `CMSG_EMOTE`: AC acts only on 0 (`EMOTE_ONESHOT_NONE`) and 3 (`EMOTE_ONESHOT_WAVE`).
    Emote { emote: u32 },
    /// `CMSG_QUERY_TIME`: empty; answered by `SMSG_QUERY_TIME_RESPONSE`.
    QueryTime,
    /// `CMSG_UNLEARN_SKILL`: AC unlearns primary professions only.
    UnlearnSkill { skill: u32 },
    /// `CMSG_TOGGLE_PVP` with its one-byte body (`SetPVP`, `0x5168b0`): AC sets the PvP flag to
    /// the byte, where an empty body toggles it (`HandleTogglePvP`).
    SetPvp { on: u8 },
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    match command {
        Command::PlayerLogout => (0x004a, vec![]),
        Command::Emote { emote } => (0x0102, emote.to_le_bytes().to_vec()),
        Command::QueryTime => (0x01ce, vec![]),
        Command::UnlearnSkill { skill } => (0x0202, skill.to_le_bytes().to_vec()),
        Command::SetPvp { on } => (0x0253, vec![*on]),
    }
}

#[cfg(test)]
mod tests;
