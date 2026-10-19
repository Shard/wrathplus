//! Build-12340 game objects, fishing and world presentation: object animations, page text, the
//! spirit healer prompt, music and object sounds, light overrides, phase shifts, movies, dance
//! moves and destructible-building damage, plus the use and report-use commands.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: GameObject.cpp
//! (`SendCustomAnim`, fishing `Use`, goober page text, `ModifyHealth`), Object.cpp
//! (`SendObjectDeSpawnAnim`, `SendPlayMusic`, `PlayDirectSound`/`PlayDistanceSound`),
//! MiscPackets.cpp (`PlayObjectSound`), spell_generic.cpp (spirit healer confirm), Map.cpp
//! (`SendZoneDynamicInfo`, `SetZoneOverrideLight`), CharacterHandler.cpp (dance moves at login),
//! Player.cpp (`SendMovieStart`), MiscHandler.cpp (`SendSetPhaseShift`), SpellHandler.cpp
//! (`HandleGameObjectUseOpcode`, `HandleGameobjectReportUse`). Strict framing.
//!
//! Animations, fishing, the spirit healer prompt, music and object sounds mean what 1.12's do and
//! feed the shared events. Page text, light overrides, phase shifts, movies, dance moves and
//! building damage have no 1.12 counterpart and are capture-only.
use crate::wire::read_packed_guid;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 13] = [
    0x0032, 0x00b3, 0x01c8, 0x01c9, 0x01df, 0x0215, 0x0222, 0x0277, 0x0278, 0x0412, 0x0455, 0x0464,
    0x047c,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_DESTRUCTIBLE_BUILDING_DAMAGE`: a siege building took `change` (negative heals) from
    /// `attacker`, credited to `player`'s combat log.
    BuildingDamage {
        building: u64,
        attacker: u64,
        player: u64,
        change: i32,
        spell: u32,
    },
    /// `SMSG_GAMEOBJECT_PAGETEXT`: a goober with a page was used; its template names the page.
    PageText { object: u64 },
    /// `SMSG_OVERRIDE_LIGHT`: the zone's default light and its override (`Light.dbc` ids), with the
    /// fade-in time in ms.
    OverrideLight {
        default_light: u32,
        override_light: u32,
        fade_in_ms: u32,
    },
    /// `SMSG_LEARNED_DANCE_MOVES`: two raw words; AC always sends zeros at login.
    LearnedDanceMoves { raw: [u32; 2] },
    /// `SMSG_TRIGGER_MOVIE`: a `Movie.dbc` id.
    TriggerMovie { movie: u32 },
    /// `SMSG_SET_PHASE_SHIFT`: the phase mask now visible.
    PhaseShift { mask: u32 },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
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

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let capture = |packet| SessionEvent::WrathWorldObjects { packet };
    let event = match opcode {
        0x00b3 => SessionEvent::GameObjectCustomAnim {
            guid: u64_(r)?,
            anim_id: u32_(r)?,
        },
        0x0215 => SessionEvent::GameObjectDespawnAnim { guid: u64_(r)? },
        0x01c8 => SessionEvent::FishNotHooked,
        0x01c9 => SessionEvent::FishEscaped,
        0x0222 => SessionEvent::SpiritHealerConfirm { npc: u64_(r)? },
        0x0277 => SessionEvent::PlayMusic { music_id: u32_(r)? },
        // `PlayObjectSound::Write`: the sound kit, then the source object.
        0x0278 => {
            let sound_id = u32_(r)?;
            SessionEvent::PlayObjectSound {
                sound_id,
                guid: u64_(r)?,
            }
        }
        0x0032 => capture(Packet::BuildingDamage {
            building: read_packed_guid(r)?,
            attacker: read_packed_guid(r)?,
            player: read_packed_guid(r)?,
            change: u32_(r)? as i32,
            spell: u32_(r)?,
        }),
        0x01df => capture(Packet::PageText { object: u64_(r)? }),
        0x0412 => capture(Packet::OverrideLight {
            default_light: u32_(r)?,
            override_light: u32_(r)?,
            fade_in_ms: u32_(r)?,
        }),
        0x0455 => capture(Packet::LearnedDanceMoves {
            raw: [u32_(r)?, u32_(r)?],
        }),
        0x0464 => capture(Packet::TriggerMovie { movie: u32_(r)? }),
        0x047c => capture(Packet::PhaseShift { mask: u32_(r)? }),
        _ => return Err(invalid("world object opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("world object extent"));
    }
    Ok(event)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_GAMEOBJ_USE`: open, loot, gather or activate the object (full guid).
    Use { object: u64 },
    /// `CMSG_GAMEOBJ_REPORT_USE`: AC credits the use-game-object achievement criterion.
    ReportUse { object: u64 },
    /// `CMSG_COMPLETE_MOVIE`, empty: the client's `0x518870` ends a server-triggered movie with it.
    /// AC binds it to `Handle_NULL`.
    CompleteMovie,
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    match command {
        Command::Use { object } => (0x00b1, object.to_le_bytes().to_vec()),
        Command::ReportUse { object } => (0x0481, object.to_le_bytes().to_vec()),
        Command::CompleteMovie => (0x0465, Vec::new()),
    }
}

#[cfg(test)]
mod tests;
