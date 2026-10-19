//! Build-12340 layouts of the session messages the HUD needs: names, the spell book and the action
//! bar. Each decodes to the shared [`SessionEvent`]s the Vanilla decoder produces, so the client's
//! consumers are unchanged. Layouts from AzerothCore (`QueryHandler.cpp`, `QueryPackets.cpp`,
//! `Player::SendInitialSpells`, `Player::SendActionButtons`, `Player::learnSpell`).

use std::io::{self, Cursor, Read};

use crate::events::{decode as session_decode, SessionEvent};
use crate::messages::{ActionButton, CreatureQueryInfo, ServerPacket, SpellCooldown};
use crate::wire::read_packed_guid;

pub const SMSG_NAME_QUERY_RESPONSE: u16 = 0x0051;
pub const SMSG_CREATURE_QUERY_RESPONSE: u16 = 0x0061;
pub const SMSG_ACTION_BUTTONS: u16 = 0x0129;
pub const SMSG_INITIAL_SPELLS: u16 = 0x012A;
pub const SMSG_LEARNED_SPELL: u16 = 0x012B;
pub const SMSG_SUPERCEDED_SPELL: u16 = 0x012C;
pub const SMSG_REMOVED_SPELL: u16 = 0x0203;
pub const SMSG_CAST_FAILED: u16 = 0x0130;
pub const SMSG_SPELL_START: u16 = 0x0131;
pub const SMSG_SPELL_GO: u16 = 0x0132;
pub const SMSG_SPELL_COOLDOWN: u16 = 0x0134;
pub const SMSG_SPELL_DELAYED: u16 = 0x01E2;
pub const SMSG_SPELL_FAILED_OTHER: u16 = 0x02A6;
/// `Player::SendAutoRepeatCancel`: the packed guid of the unit whose auto-repeat stopped, to the set.
pub const SMSG_CANCEL_AUTO_REPEAT: u16 = 0x029C;
pub const SMSG_INITIALIZE_FACTIONS: u16 = 0x0122;
pub const SMSG_SET_FACTION_STANDING: u16 = 0x0124;
/// Unchanged since 1.12; the Vanilla parser reads them (XP gain only appends a trailing byte).
pub const SMSG_LOG_XPGAIN: u16 = 0x01D0;
pub const SMSG_PARTYKILLLOG: u16 = 0x01F5;
pub const SMSG_EXPLORATION_EXPERIENCE: u16 = 0x01F8;

pub const CMSG_NAME_QUERY: u32 = 0x0050;
pub const CMSG_GAMEOBJECT_QUERY: u32 = 0x005E;
pub const CMSG_CREATURE_QUERY: u32 = 0x0060;
pub const CMSG_SET_ACTION_BUTTON: u32 = 0x0128;
pub const CMSG_CAST_SPELL: u32 = 0x012E;
pub const CMSG_CANCEL_CAST: u32 = 0x012F;
pub const CMSG_CANCEL_AURA: u32 = 0x0136;

/// Build 12340's action bar: 144 slots (1.12 had 120).
pub const ACTION_BUTTONS: usize = 144;

/// Whether this module decodes `opcode`.
pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_NAME_QUERY_RESPONSE
            | SMSG_CREATURE_QUERY_RESPONSE
            | SMSG_ACTION_BUTTONS
            | SMSG_INITIAL_SPELLS
            | SMSG_LEARNED_SPELL
            | SMSG_SUPERCEDED_SPELL
            | SMSG_REMOVED_SPELL
            | SMSG_CAST_FAILED
            | SMSG_SPELL_START
            | SMSG_SPELL_GO
            | SMSG_SPELL_COOLDOWN
            | SMSG_SPELL_DELAYED
            | SMSG_SPELL_FAILED_OTHER
            | SMSG_CANCEL_AUTO_REPEAT
            | SMSG_INITIALIZE_FACTIONS
            | SMSG_SET_FACTION_STANDING
            | SMSG_LOG_XPGAIN
            | SMSG_PARTYKILLLOG
            | SMSG_EXPLORATION_EXPERIENCE
    )
}

/// Decode one of the [`handles`] opcodes. `self_guid` is our player, once its create arrived:
/// build 12340 confirms our own successful cast only by its spell-go.
pub fn decode(opcode: u16, body: &[u8], self_guid: Option<u64>) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    Ok(match opcode {
        // 1.12's layout with 128 slots: the count, then `(u8 flags, i32 standing)` each.
        SMSG_INITIALIZE_FACTIONS => {
            let count = u32_le(&mut r)?;
            let standings = (0..count)
                .map(|_| Ok((u8_le(&mut r)?, u32_le(&mut r)? as i32)))
                .collect::<io::Result<Vec<_>>>()?;
            vec![SessionEvent::Reputations { standings }]
        }
        // A refer-a-friend bonus float and a visual byte precede 1.12's `(list id, standing)`s.
        SMSG_SET_FACTION_STANDING => {
            let _bonus = u32_le(&mut r)?;
            let _visual = u8_le(&mut r)?;
            let count = u32_le(&mut r)?;
            let standings = (0..count)
                .map(|_| Ok((u32_le(&mut r)?, u32_le(&mut r)? as i32)))
                .collect::<io::Result<Vec<_>>>()?;
            vec![SessionEvent::ReputationDelta { standings }]
        }
        // AC `Player::SendLogXPGain`: victim, total, type (0 kill), for a kill the award before
        // the rested bonus and the group-bonus float, then the refer-a-friend byte 1.12 lacked.
        SMSG_LOG_XPGAIN => {
            let victim = u64_le(&mut r)?;
            let total = u32_le(&mut r)?;
            let kill = u8_le(&mut r)? == 0;
            let base = if kill {
                let base = u32_le(&mut r)?;
                let _group_bonus = f32_le(&mut r)?;
                base
            } else {
                total
            };
            let _recruit_a_friend = u8_le(&mut r)?;
            strict(&r, body, "LOG_XPGAIN")?;
            vec![SessionEvent::XpGain(crate::messages::XpGain {
                victim,
                total,
                base,
                kill,
            })]
        }
        // AC `Unit::Kill`: the killing blow's player and the victim.
        SMSG_PARTYKILLLOG => {
            let log = crate::messages::PartyKillLog {
                killer: u64_le(&mut r)?,
                victim: u64_le(&mut r)?,
            };
            strict(&r, body, "PARTYKILLLOG")?;
            vec![SessionEvent::PartyKillLog(log)]
        }
        // AC `Player::CheckAreaExploreAndOutdoor`: the area and the XP.
        SMSG_EXPLORATION_EXPERIENCE => {
            let xp = crate::messages::ExplorationXp {
                area_id: u32_le(&mut r)?,
                xp: u32_le(&mut r)?,
            };
            strict(&r, body, "EXPLORATION_EXPERIENCE")?;
            vec![SessionEvent::ExplorationXp(xp)]
        }
        SMSG_CAST_FAILED => cast_failed(&mut r)?,
        SMSG_SPELL_START => spell_start(&mut r)?,
        SMSG_SPELL_GO => spell_go(&mut r, self_guid)?,
        SMSG_SPELL_COOLDOWN => spell_cooldown(&mut r)?,
        SMSG_SPELL_DELAYED => vec![SessionEvent::SpellDelayed {
            caster: read_packed_guid(&mut r)?,
            delay_ms: u32_le(&mut r)?,
        }],
        SMSG_SPELL_FAILED_OTHER => {
            let caster = read_packed_guid(&mut r)?;
            // AzerothCore `Spell::SendInterrupted`: packed caster, cast count, spell, result. Strict.
            let cast_count = Some(u8_le(&mut r)?);
            let spell_id = u32_le(&mut r)?;
            let reason = Some(u8_le(&mut r)?);
            if r.position() as usize != body.len() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "SPELL_FAILED_OTHER trailing bytes"));
            }
            vec![SessionEvent::SpellFailedOther { caster, spell_id, cast_count, reason }]
        }
        // Only our own drives the shared (self-only) cancel; another unit's is consumed silently.
        SMSG_CANCEL_AUTO_REPEAT => {
            let unit = read_packed_guid(&mut r)?;
            if r.position() as usize != body.len() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "CANCEL_AUTO_REPEAT trailing bytes"));
            }
            if self_guid == Some(unit) {
                vec![SessionEvent::CancelAutoRepeat]
            } else {
                Vec::new()
            }
        }
        SMSG_NAME_QUERY_RESPONSE => name_query(&mut r)?,
        SMSG_CREATURE_QUERY_RESPONSE => session_decode(creature_query(&mut r)?),
        SMSG_ACTION_BUTTONS => action_buttons(&mut r)?,
        SMSG_INITIAL_SPELLS => initial_spells(&mut r)?,
        SMSG_LEARNED_SPELL => vec![SessionEvent::SpellLearned { spell_id: u32_le(&mut r)? }],
        SMSG_REMOVED_SPELL => vec![SessionEvent::SpellRemoved { spell_id: u32_le(&mut r)? }],
        SMSG_SUPERCEDED_SPELL => vec![SessionEvent::SpellSuperceded {
            old_spell_id: u32_le(&mut r)?,
            new_spell_id: u32_le(&mut r)?,
        }],
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "not a Wrath session opcode")),
    })
}

/// Packed guid, a "name unknown" byte, then name, realm name, race, sex and class bytes.
fn name_query(r: &mut Cursor<&[u8]>) -> io::Result<Vec<SessionEvent>> {
    let guid = read_packed_guid(r)?;
    if u8_le(r)? != 0 {
        return Ok(Vec::new());
    }
    let name = cstring(r)?;
    let _realm = cstring(r)?;
    let (race, gender, class) = (u8_le(r)?, u8_le(r)?, u8_le(r)?);
    Ok(vec![SessionEvent::PlayerName {
        guid,
        name,
        race: u32::from(race),
        gender: u32::from(gender),
        class: u32::from(class),
    }])
}

/// Entry (high bit: no such template), four names, title, icon, then the template words.
fn creature_query(r: &mut Cursor<&[u8]>) -> io::Result<ServerPacket> {
    let raw = u32_le(r)?;
    let entry = raw & 0x7fff_ffff;
    if raw & 0x8000_0000 != 0 {
        return Ok(ServerPacket::CreatureQueryResponse { entry, info: None });
    }
    let name = cstring(r)?;
    for _ in 0..3 {
        cstring(r)?;
    }
    let subname = cstring(r)?;
    let _icon = cstring(r)?;
    let type_flags = u32_le(r)?;
    let creature_type = u32_le(r)?;
    let pet_family = u32_le(r)?;
    let rank = u32_le(r)?;
    let _kill_credit = (u32_le(r)?, u32_le(r)?);
    let display_id = u32_le(r)?;
    for _ in 0..3 {
        u32_le(r)?;
    }
    let modifiers = [u32_le(r)?, u32_le(r)?];
    let racial_leader = u8_le(r)? != 0;
    Ok(ServerPacket::CreatureQueryResponse {
        entry,
        info: Some(CreatureQueryInfo {
            name,
            subname,
            creature_type,
            pet_family,
            rank,
            type_flags,
            // 3.3.5 carries no civilian word; the flag lives server-side.
            civilian: false,
            racial_leader,
            display_id,
            modifiers: Some(modifiers),
        }),
    })
}

/// A state byte (2: clear the bars, nothing follows), then 144 packed `action | type << 24`.
fn action_buttons(r: &mut Cursor<&[u8]>) -> io::Result<Vec<SessionEvent>> {
    if u8_le(r)? == 2 {
        return Ok(vec![SessionEvent::ActionButtons { buttons: Vec::new() }]);
    }
    let mut buttons = Vec::new();
    for slot in 0..ACTION_BUTTONS {
        let packed = u32_le(r)?;
        if packed != 0 {
            buttons.push(ActionButton {
                slot: slot as u8,
                action: packed & 0x00ff_ffff,
                kind: (packed >> 24) as u8,
            });
        }
    }
    Ok(vec![SessionEvent::ActionButtons { buttons }])
}

/// `u8 0`, a `u16` count of `{u32 spell, u16 0}`, then a `u16` count of cooldowns (dropped).
fn initial_spells(r: &mut Cursor<&[u8]>) -> io::Result<Vec<SessionEvent>> {
    u8_le(r)?;
    let count = u16_le(r)?;
    let mut spell_ids = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        spell_ids.push(u32_le(r)?);
        u16_le(r)?;
    }
    // Running cooldowns: `u32 spell, u16 item, u16 category, u32 cooldown, u32 category cooldown`.
    // AzerothCore counts entries it then skips (`needSendToClient`), so the body's end decides.
    let listed = u16_le(r).unwrap_or(0);
    let mut cooldowns = Vec::new();
    for _ in 0..listed {
        let entry = (|| -> io::Result<SpellCooldown> {
            Ok(SpellCooldown {
                spell_id: u32_le(r)?,
                item_id: u16_le(r)?,
                category: u16_le(r)?,
                spell_cd_ms: u32_le(r)?,
                category_cd_ms: u32_le(r)?,
            })
        })();
        match entry {
            Ok(cd) => cooldowns.push(cd),
            Err(_) => break,
        }
    }
    Ok(vec![SessionEvent::SpellBook {
        spell_ids,
        cooldowns,
    }])
}

/// Cast count, spell, a build-12340 `SpellCastResult` (renumbered from 1.12; the client maps
/// it by name), then the first extra word some results carry.
fn cast_failed(r: &mut Cursor<&[u8]>) -> io::Result<Vec<SessionEvent>> {
    let _count = u8_le(r)?;
    let spell_id = u32_le(r)?;
    let reason = u8_le(r)?;
    let arg = u32_le(r).ok();
    Ok(vec![SessionEvent::CastResult {
        spell_id,
        success: false,
        reason: Some(reason),
        arg,
    }])
}

/// What a `SpellCastTargets` block names.
#[derive(Default)]
struct Targets {
    unit: Option<u64>,
    game_object: Option<u64>,
    dest: Option<[f32; 3]>,
}

fn targets(r: &mut Cursor<&[u8]>) -> io::Result<Targets> {
    let mask = u32_le(r)?;
    let mut out = Targets::default();
    if mask & (TARGET_FLAG_UNIT | TARGET_FLAG_GAMEOBJECT | 0x0200 | 0x8000 | 0x1_0000) != 0 {
        let guid = read_packed_guid(r)?;
        if mask & TARGET_FLAG_GAMEOBJECT != 0 {
            out.game_object = Some(guid);
        } else {
            out.unit = Some(guid).filter(|g| *g != 0);
        }
    }
    if mask & (TARGET_FLAG_ITEM | 0x1000) != 0 {
        read_packed_guid(r)?;
    }
    for flag in [TARGET_FLAG_SOURCE_LOCATION, TARGET_FLAG_DEST_LOCATION] {
        if mask & flag != 0 {
            read_packed_guid(r)?;
            let at = [f32_le(r)?, f32_le(r)?, f32_le(r)?];
            if flag == TARGET_FLAG_DEST_LOCATION {
                out.dest = Some(at);
            }
        }
    }
    Ok(out)
}

/// Cast item or caster, caster, cast count, spell, flags, cast time, targets.
fn spell_start(r: &mut Cursor<&[u8]>) -> io::Result<Vec<SessionEvent>> {
    let item_or_caster = read_packed_guid(r)?;
    let caster = read_packed_guid(r)?;
    let count = u8_le(r)?;
    let spell_id = u32_le(r)?;
    let cast_flags = u32_le(r)?;
    let cast_time_ms = u32_le(r)?;
    let t = targets(r)?;
    Ok(vec![SessionEvent::SpellStart {
        caster: if caster == 0 { item_or_caster } else { caster },
        spell_id,
        cast_flags: cast_flags as u16,
        cast_time_ms,
        target: t.unit,
        ammo_display_id: None,
        cast_count: Some(count),
    }])
}

/// As spell start, then a timestamp, the hit and miss lists, and targets. Our own player-cast
/// spell-go (a nonzero cast count) is also its success result.
fn spell_go(r: &mut Cursor<&[u8]>, self_guid: Option<u64>) -> io::Result<Vec<SessionEvent>> {
    let item_or_caster = read_packed_guid(r)?;
    let caster_slot = read_packed_guid(r)?;
    let caster = if caster_slot == 0 { item_or_caster } else { caster_slot };
    let count = u8_le(r)?;
    let spell_id = u32_le(r)?;
    let cast_flags = u32_le(r)?;
    let _timestamp = u32_le(r)?;
    let hits = (0..u8_le(r)?).map(|_| u64_le(r)).collect::<io::Result<Vec<_>>>()?;
    let mut misses = Vec::new();
    for _ in 0..u8_le(r)? {
        let guid = u64_le(r)?;
        let condition = u8_le(r)?;
        if condition == 11 {
            u8_le(r)?; // the reflect result
        }
        misses.push((guid, condition));
    }
    let t = targets(r)?;
    // The tail AzerothCore writes after the targets: the caster's power (`CAST_FLAG_POWER_LEFT_SELF`)
    // then the rune list. A short tail drops the runes, never the spell-go itself.
    let runes = (|| -> io::Result<Option<crate::wrath_spell_control::RunePacket>> {
        if cast_flags & CAST_FLAG_POWER_LEFT_SELF != 0 {
            u32_le(r)?;
        }
        if cast_flags & CAST_FLAG_RUNE_LIST == 0 {
            return Ok(None);
        }
        let (before, after) = (u8_le(r)?, u8_le(r)?);
        let passed = (0..6)
            .filter(|i| before >> i & 1 == 1 && after >> i & 1 == 0)
            .map(|_| u8_le(r))
            .collect::<io::Result<_>>()?;
        Ok(Some(crate::wrath_spell_control::RunePacket::Spend {
            before,
            after,
            passed,
        }))
    })()
    .ok()
    .flatten()
    .filter(|_| Some(caster) == self_guid);
    let mut events = Vec::with_capacity(3);
    if count != 0 && Some(caster) == self_guid {
        events.push(SessionEvent::CastResult {
            spell_id,
            success: true,
            reason: None,
            arg: None,
        });
    }
    events.push(SessionEvent::SpellGo {
        caster,
        spell_id,
        cast_flags: cast_flags as u16,
        hits,
        misses,
        target: t.unit,
        go_target: t.game_object,
        dest: t.dest,
        ammo_display_id: None,
        item_caster: (item_or_caster != caster).then_some(item_or_caster),
        cast_count: Some(count),
    });
    if let Some(packet) = runes {
        events.push(SessionEvent::WrathRunes { packet });
    }
    Ok(events)
}

/// AzerothCore `SpellCastFlags`: the power word and the rune list that follow a spell-go's targets.
const CAST_FLAG_POWER_LEFT_SELF: u32 = 0x0000_0800;
const CAST_FLAG_RUNE_LIST: u32 = 0x0020_0000;

/// Caster guid, a flags byte, then `(spell, cooldown ms)` pairs.
fn spell_cooldown(r: &mut Cursor<&[u8]>) -> io::Result<Vec<SessionEvent>> {
    let caster = u64_le(r)?;
    let _flags = u8_le(r)?;
    let mut cooldowns = Vec::new();
    while let Ok(spell) = u32_le(r) {
        cooldowns.push((spell, u32_le(r)?));
    }
    Ok(vec![SessionEvent::SpellCooldowns { caster, cooldowns }])
}

/// `CMSG_NAME_QUERY`: the full guid.
pub fn encode_name_query(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `CMSG_CREATURE_QUERY` and `CMSG_GAMEOBJECT_QUERY`: entry, then the full guid.
pub fn encode_template_query(entry: u32, guid: u64) -> Vec<u8> {
    let mut body = entry.to_le_bytes().to_vec();
    body.extend(guid.to_le_bytes());
    body
}

/// `CMSG_SET_ACTION_BUTTON`: slot, then `action | type << 24` (0 clears).
pub fn encode_set_action_button(button: u8, packed: u32) -> Vec<u8> {
    let mut body = vec![button];
    body.extend(packed.to_le_bytes());
    body
}

/// `CMSG_CANCEL_CAST`: a cast count byte the server skips, then the spell.
pub fn encode_cancel_cast(spell_id: u32) -> Vec<u8> {
    let mut body = vec![0];
    body.extend(spell_id.to_le_bytes());
    body
}

/// `CMSG_CANCEL_AURA`: the spell.
pub fn encode_cancel_aura(spell_id: u32) -> Vec<u8> {
    spell_id.to_le_bytes().to_vec()
}

/// A cast's target block (`SpellCastTargets::Read`): the 32-bit mask, then what it names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CastTarget {
    /// No explicit target: self or the spell's implicit targets.
    None,
    Unit(u64),
    GameObject(u64),
    Item(u64),
    /// A world position, not on a transport.
    Dest([f32; 3]),
    Source([f32; 3]),
}

const TARGET_FLAG_UNIT: u32 = 0x0002;
const TARGET_FLAG_ITEM: u32 = 0x0010;
const TARGET_FLAG_SOURCE_LOCATION: u32 = 0x0020;
const TARGET_FLAG_DEST_LOCATION: u32 = 0x0040;
const TARGET_FLAG_GAMEOBJECT: u32 = 0x0800;

/// `CMSG_PET_CAST_SPELL`: a cast whose caster is not the active player, our pet, charm or
/// possessed unit (`Wow.exe` `0x80ac90` opens it instead of `CMSG_CAST_SPELL` then). AzerothCore
/// `HandlePetCastSpellOpcode` reads the caster's full guid, then the cast's own body.
pub const CMSG_PET_CAST_SPELL: u32 = 0x01F0;

/// `CMSG_PET_CAST_SPELL`: `u64 caster`, then [`encode_cast_spell`]'s body.
pub fn encode_pet_cast_spell(
    caster: u64,
    cast_count: u8,
    spell_id: u32,
    target: &CastTarget,
) -> Vec<u8> {
    let mut body = caster.to_le_bytes().to_vec();
    body.extend(encode_cast_spell(cast_count, spell_id, target));
    body
}

/// `CMSG_CAST_SPELL`: cast count, spell, cast flags, then the target block.
pub fn encode_cast_spell(cast_count: u8, spell_id: u32, target: &CastTarget) -> Vec<u8> {
    let mut body = vec![cast_count];
    body.extend(spell_id.to_le_bytes());
    body.push(0);
    let packed = |guid: u64, body: &mut Vec<u8>| {
        crate::wire::write_packed_guid(guid, body).expect("writing to a Vec");
    };
    let position = |at: &[f32; 3], body: &mut Vec<u8>| {
        body.push(0); // no transport: a packed zero guid
        for v in at {
            body.extend(v.to_le_bytes());
        }
    };
    match target {
        CastTarget::None => body.extend(0u32.to_le_bytes()),
        CastTarget::Unit(guid) => {
            body.extend(TARGET_FLAG_UNIT.to_le_bytes());
            packed(*guid, &mut body);
        }
        CastTarget::GameObject(guid) => {
            body.extend(TARGET_FLAG_GAMEOBJECT.to_le_bytes());
            packed(*guid, &mut body);
        }
        CastTarget::Item(guid) => {
            body.extend(TARGET_FLAG_ITEM.to_le_bytes());
            packed(*guid, &mut body);
        }
        CastTarget::Dest(at) => {
            body.extend(TARGET_FLAG_DEST_LOCATION.to_le_bytes());
            position(at, &mut body);
        }
        CastTarget::Source(at) => {
            body.extend(TARGET_FLAG_SOURCE_LOCATION.to_le_bytes());
            position(at, &mut body);
        }
    }
    body
}

pub(crate) fn u8_le(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

pub(crate) fn u16_le(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

pub(crate) fn u32_le(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

pub(crate) fn u64_le(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

/// A body read to its end: trailing bytes are a layout this decoder does not know.
fn strict(r: &Cursor<&[u8]>, body: &[u8], what: &'static str) -> io::Result<()> {
    if r.position() as usize != body.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, what));
    }
    Ok(())
}

pub(crate) fn f32_le(r: &mut impl Read) -> io::Result<f32> {
    Ok(f32::from_bits(u32_le(r)?))
}

pub(crate) fn cstring(r: &mut impl Read) -> io::Result<String> {
    let mut bytes = Vec::new();
    loop {
        let b = u8_le(r)?;
        if b == 0 {
            break;
        }
        bytes.push(b);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_targeted_cast_writes_count_spell_flags_mask_and_packed_guid() {
        let body = encode_cast_spell(1, 78, &CastTarget::Unit(0x05));
        assert_eq!(body, [1, 78, 0, 0, 0, 0, 2, 0, 0, 0, 0x01, 0x05]);
        let body = encode_cast_spell(2, 6603, &CastTarget::None);
        assert_eq!(body, [2, 0xCB, 0x19, 0, 0, 0, 0, 0, 0, 0]);
    }

    /// A death knight's Plague Strike: power word, then runes 0b111111 before and 0b111010
    /// after, so runes 0 and 2 went down with 0 and 51 of 255 run; an observer gets no runes.
    #[test]
    fn our_spell_go_rune_list_spends_runes() {
        use crate::wrath_spell_control::RunePacket;
        let mut body = vec![0x01, 0x10, 0x01, 0x10, 1];
        body.extend(45462u32.to_le_bytes());
        body.extend((0x100 | CAST_FLAG_POWER_LEFT_SELF | CAST_FLAG_RUNE_LIST).to_le_bytes());
        body.extend(0u32.to_le_bytes());
        body.extend([0, 0]); // no hits, no misses
        body.extend(0u32.to_le_bytes()); // self-targeted: an empty target mask
        body.extend(100u32.to_le_bytes()); // runic power left
        body.extend([0b11_1111, 0b11_1010, 0, 51]);
        let runes = |self_guid| -> Vec<RunePacket> {
            decode(SMSG_SPELL_GO, &body, Some(self_guid))
                .unwrap()
                .into_iter()
                .filter_map(|e| match e {
                    SessionEvent::WrathRunes { packet } => Some(packet),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            runes(0x10),
            [RunePacket::Spend {
                before: 0b11_1111,
                after: 0b11_1010,
                passed: vec![0, 51]
            }]
        );
        assert!(runes(0x99).is_empty());
        // A truncated tail keeps the spell-go and drops the runes.
        body.truncate(body.len() - 1);
        let events = decode(SMSG_SPELL_GO, &body, Some(0x10)).unwrap();
        assert!(events.iter().any(|e| matches!(e, SessionEvent::SpellGo { .. })));
        assert!(!events.iter().any(|e| matches!(e, SessionEvent::WrathRunes { .. })));
    }

    #[test]
    fn our_spell_go_is_also_the_success_result() {
        // Caster 0x10 twice, count 1, Heroic Strike, flags, timestamp, one hit, no misses,
        // unit target 0x05.
        let mut body = vec![0x01, 0x10, 0x01, 0x10, 1];
        body.extend(78u32.to_le_bytes());
        body.extend(0x100u32.to_le_bytes());
        body.extend(0u32.to_le_bytes());
        body.push(1);
        body.extend(5u64.to_le_bytes());
        body.push(0);
        body.extend(TARGET_FLAG_UNIT.to_le_bytes());
        body.extend([0x01, 0x05]);
        let events = decode(SMSG_SPELL_GO, &body, Some(0x10)).unwrap();
        assert!(matches!(
            &events[..],
            [
                SessionEvent::CastResult { spell_id: 78, success: true, .. },
                SessionEvent::SpellGo { caster: 0x10, target: Some(5), .. }
            ]
        ));
        let others = decode(SMSG_SPELL_GO, &body, Some(0x99)).unwrap();
        assert_eq!(others.len(), 1);
    }

    #[test]
    fn a_known_player_name_decodes() {
        // Packed guid 0x05 (mask 0x01, byte 0x05), known, "Bob", realm "", human male warrior.
        let body = [0x01, 0x05, 0, b'B', b'o', b'b', 0, 0, 1, 0, 1, 0];
        let events = decode(SMSG_NAME_QUERY_RESPONSE, &body, None).unwrap();
        assert!(matches!(
            &events[..],
            [SessionEvent::PlayerName { guid: 5, name, race: 1, gender: 0, class: 1 }] if name == "Bob"
        ));
    }

    #[test]
    fn action_buttons_unpack_all_144_slots() {
        let mut body = vec![1];
        for slot in 0..ACTION_BUTTONS as u32 {
            body.extend(if slot == 143 { 78u32 } else { 0 }.to_le_bytes());
        }
        let events = decode(SMSG_ACTION_BUTTONS, &body, None).unwrap();
        let [SessionEvent::ActionButtons { buttons }] = &events[..] else { panic!() };
        assert_eq!((buttons[0].slot, buttons[0].action, buttons[0].kind), (143, 78, 0));
    }

    #[test]
    fn initial_spells_keep_ids_past_u16() {
        let mut body = vec![0, 2, 0];
        for id in [6603u32, 70_000] {
            body.extend(id.to_le_bytes());
            body.extend(0u16.to_le_bytes());
        }
        body.extend(0u16.to_le_bytes());
        let events = decode(SMSG_INITIAL_SPELLS, &body, None).unwrap();
        let [SessionEvent::SpellBook { spell_ids, .. }] = &events[..] else { panic!() };
        assert_eq!(spell_ids, &[6603, 70_000]);
    }

    /// A running cooldown survives the relog; the server's count may include entries it skipped.
    #[test]
    fn initial_spells_carry_running_cooldowns_and_tolerate_an_overcount() {
        let mut body = vec![0, 1, 0];
        body.extend(59752u32.to_le_bytes());
        body.extend(0u16.to_le_bytes());
        body.extend(2u16.to_le_bytes()); // two listed, one written
        body.extend(59752u32.to_le_bytes());
        body.extend(0u16.to_le_bytes());
        body.extend(0u16.to_le_bytes());
        body.extend(95_000u32.to_le_bytes());
        body.extend(0u32.to_le_bytes());
        let events = decode(SMSG_INITIAL_SPELLS, &body, None).unwrap();
        let [SessionEvent::SpellBook { cooldowns, .. }] = &events[..] else { panic!() };
        assert_eq!(cooldowns.len(), 1);
        assert_eq!((cooldowns[0].spell_id, cooldowns[0].spell_cd_ms), (59752, 95_000));
    }

    #[test]
    fn a_pet_cast_is_the_casters_full_guid_then_the_cast_body() {
        let at = [1.0f32, 2.0, 3.0];
        let body = encode_pet_cast_spell(0xF140_0000_0000_0050, 7, 31707, &CastTarget::Dest(at));
        assert_eq!(&body[..8], &0xF140_0000_0000_0050u64.to_le_bytes());
        assert_eq!(&body[8..], encode_cast_spell(7, 31707, &CastTarget::Dest(at)).as_slice());
        // count, spell, flags 0, DEST mask, a packed zero transport guid, xyz.
        assert_eq!(body[8], 7);
        assert_eq!(&body[13..18], &[0, 0x40, 0, 0, 0]);
        assert_eq!(body[18], 0);
        assert_eq!(body.len(), 8 + 1 + 4 + 1 + 4 + 1 + 12);
    }

    #[test]
    fn spell_failed_other_keeps_cast_count_and_reason_with_strict_framing() {
        // Packed caster 0x42, cast count 3, spell 133, result 0x28.
        let body = [0x01, 0x42, 3, 0x85, 0, 0, 0, 0x28];
        let events = decode(SMSG_SPELL_FAILED_OTHER, &body, None).unwrap();
        let [SessionEvent::SpellFailedOther {
            caster,
            spell_id,
            cast_count,
            reason,
        }] = &events[..]
        else {
            panic!()
        };
        assert_eq!(
            (*caster, *spell_id, *cast_count, *reason),
            (0x42, 133, Some(3), Some(0x28))
        );
        for end in 0..body.len() {
            assert!(decode(SMSG_SPELL_FAILED_OTHER, &body[..end], None).is_err(), "prefix {end}");
        }
        assert!(decode(SMSG_SPELL_FAILED_OTHER, &[body.as_slice(), &[0]].concat(), None).is_err());
    }

    #[test]
    fn xp_gain_reads_the_recruit_a_friend_byte_and_party_kill_and_exploration_are_strict() {
        let mut kill = 0x42u64.to_le_bytes().to_vec();
        kill.extend(120u32.to_le_bytes());
        kill.push(0);
        kill.extend(100u32.to_le_bytes());
        kill.extend(1.0f32.to_le_bytes());
        kill.push(0); // refer-a-friend
        let [SessionEvent::XpGain(x)] = &decode(SMSG_LOG_XPGAIN, &kill, None).unwrap()[..] else {
            panic!("one gain")
        };
        assert_eq!((x.victim, x.total, x.base, x.kill), (0x42, 120, 100, true));
        let mut other = 0u64.to_le_bytes().to_vec();
        other.extend(50u32.to_le_bytes());
        other.extend([1, 0]);
        let [SessionEvent::XpGain(x)] = &decode(SMSG_LOG_XPGAIN, &other, None).unwrap()[..] else {
            panic!("one gain")
        };
        assert_eq!((x.total, x.base, x.kill), (50, 50, false));
        // The 1.12 shape (no refer-a-friend byte) and an extra byte are both refused.
        assert!(decode(SMSG_LOG_XPGAIN, &other[..other.len() - 1], None).is_err());
        assert!(decode(SMSG_LOG_XPGAIN, &[&other[..], &[0]].concat(), None).is_err());

        let pk = [1u64.to_le_bytes(), 2u64.to_le_bytes()].concat();
        assert!(matches!(
            &decode(SMSG_PARTYKILLLOG, &pk, None).unwrap()[..],
            [SessionEvent::PartyKillLog(l)] if (l.killer, l.victim) == (1, 2)
        ));
        let ex = [12u32.to_le_bytes(), 300u32.to_le_bytes()].concat();
        assert!(matches!(
            &decode(SMSG_EXPLORATION_EXPERIENCE, &ex, None).unwrap()[..],
            [SessionEvent::ExplorationXp(e)] if (e.area_id, e.xp) == (12, 300)
        ));
        assert!(decode(SMSG_EXPLORATION_EXPERIENCE, &ex[..7], None).is_err());
    }
}
