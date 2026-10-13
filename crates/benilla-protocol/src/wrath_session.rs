//! Build-12340 layouts of the session messages the HUD needs: names, the spell book and the action
//! bar. Each decodes to the shared [`SessionEvent`]s the Vanilla decoder produces, so the client's
//! consumers are unchanged. Layouts from AzerothCore (`QueryHandler.cpp`, `QueryPackets.cpp`,
//! `Player::SendInitialSpells`, `Player::SendActionButtons`, `Player::learnSpell`).

use std::io::{self, Cursor, Read};

use crate::events::{decode as session_decode, SessionEvent};
use crate::messages::{ActionButton, CreatureQueryInfo, ServerPacket};
use crate::wire::read_packed_guid;

pub const SMSG_NAME_QUERY_RESPONSE: u16 = 0x0051;
pub const SMSG_CREATURE_QUERY_RESPONSE: u16 = 0x0061;
pub const SMSG_ACTION_BUTTONS: u16 = 0x0129;
pub const SMSG_INITIAL_SPELLS: u16 = 0x012A;
pub const SMSG_LEARNED_SPELL: u16 = 0x012B;
pub const SMSG_SUPERCEDED_SPELL: u16 = 0x012C;
pub const SMSG_REMOVED_SPELL: u16 = 0x0203;

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
    )
}

/// Decode one of the [`handles`] opcodes.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    Ok(match opcode {
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
    let _modifiers = (u32_le(r)?, u32_le(r)?);
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
    Ok(vec![SessionEvent::SpellBook {
        spell_ids,
        cooldowns: Vec::new(),
    }])
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

fn u8_le(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u16_le(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn u32_le(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn cstring(r: &mut impl Read) -> io::Result<String> {
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

    #[test]
    fn a_known_player_name_decodes() {
        // Packed guid 0x05 (mask 0x01, byte 0x05), known, "Bob", realm "", human male warrior.
        let body = [0x01, 0x05, 0, b'B', b'o', b'b', 0, 0, 1, 0, 1, 0];
        let events = decode(SMSG_NAME_QUERY_RESPONSE, &body).unwrap();
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
        let events = decode(SMSG_ACTION_BUTTONS, &body).unwrap();
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
        let events = decode(SMSG_INITIAL_SPELLS, &body).unwrap();
        let [SessionEvent::SpellBook { spell_ids, .. }] = &events[..] else { panic!() };
        assert_eq!(spell_ids, &[6603, 70_000]);
    }
}
