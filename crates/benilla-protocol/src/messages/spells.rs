//! The cast pipeline on the wire: the `CMSG_CAST_SPELL` shapes, the server's verdict, the
//! start/launch pair every observer sees, interrupt and pushback notices, the channel timer and the
//! aura messages. Aura state itself is descriptor data (`UNIT_FIELD_AURA`), not a packet.

use crate::wire::Vector3d;

/// `SMSG_CAST_RESULT`'s verdict: status 0 (`OKAY`) ends the packet; status 2 (`FAIL`) adds a `u8`
/// reason and up to two argument words (vmangos `CastResult::AppendBodyTo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastOutcome {
    Ok,
    Failed {
        reason: u8,
        /// The `%s` of the client's message (`0x6e1d8e`): a `SpellFocusObject.dbc` id for
        /// `REQUIRES_SPELL_FOCUS` (0x5e), an `AreaTable.dbc` id for `REQUIRES_AREA` (0x5d), an
        /// `EquippedItemClass` for 0x19-0x1b, a permanent-cooldown flag for `NOT_READY` (0x3c).
        arg: Option<u32>,
    },
}

// --- Start and go --------------------------------------------------------------------------------
//
// vmangos `SpellCastTargetFlags` (`SpellDefines.h:96-113`).
const TARGET_FLAG_SOURCE_LOCATION: u16 = 0x0020;
const TARGET_FLAG_DEST_LOCATION: u16 = 0x0040;
const TARGET_FLAG_CORPSE_ENEMY: u16 = 0x0200;
const TARGET_FLAG_GAMEOBJECT: u16 = 0x0800;
const TARGET_FLAG_CORPSE_ALLY: u16 = 0x8000;

/// A decoded `SpellCastTargets` in the server's write order (`SpellCastTargetsInfo.cpp:180-234`),
/// not its read order: any mix of UNIT, GAMEOBJECT and CORPSE_* bits yields one packed guid.
/// Item, corpse, source and string targets are read and dropped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpellCastTargets {
    pub mask: u16,
    pub unit_target: Option<u64>,
    /// The `TARGET_FLAG_GAMEOBJECT` target, such as the chest an open-lock cast opens.
    pub go_target: Option<u64>,
    pub dest: Option<Vector3d>,
}

/// `SMSG_SPELL_START` (vmangos `Spell.cpp:4468-4503`): a non-triggered cast began, instants
/// included (`cast_time_ms == 0`). `item_or_caster` is the cast item's guid if any, else the
/// caster's; `caster` is 0 for a GameObject caster. `cast_flags` is always 0x2, plus
/// `CAST_FLAG_AMMO` for a ranged spell.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellStart {
    pub item_or_caster: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub cast_flags: u16,
    pub cast_time_ms: u32,
    pub targets: SpellCastTargets,
    pub ammo_display_id: Option<u32>,
}

/// `SMSG_SPELL_GO` (vmangos `Spell.cpp:4505-4538, 4608-4659`): the cast launched. Guids and spell
/// as in [`SpellStart`]; `cast_flags` is always 0x100, plus `CAST_FLAG_AMMO` for a ranged spell.
/// Sent at launch: the server times the impact from `Spell.dbc` Speed, so no missile travel data
/// rides it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellGo {
    pub item_or_caster: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub cast_flags: u16,
    pub hits: Vec<u64>,
    /// `(guid, SpellMissInfo)`; a reflect's trailing outcome byte is dropped.
    pub misses: Vec<(u64, u8)>,
    pub targets: SpellCastTargets,
    pub ammo_display_id: Option<u32>,
}

/// Body of `CMSG_CAST_SPELL` for a self or unit target. `None` sends mask 0 (`TARGET_FLAG_SELF`)
/// and the server applies the spell's implicit targeting; `Some` sends `TARGET_FLAG_UNIT` and the
/// packed guid.
pub fn cast_spell(spell_id: u32, target: Option<u64>) -> Vec<u8> {
    let mut body = Vec::with_capacity(16);
    body.extend_from_slice(&spell_id.to_le_bytes());
    match target {
        None => body.extend_from_slice(&0u16.to_le_bytes()),
        Some(guid) => {
            body.extend_from_slice(&2u16.to_le_bytes());
            crate::wire::write_packed_guid(guid, &mut body).expect("vec write");
        }
    }
    body
}

/// Body of `CMSG_CAST_SPELL` at a GameObject: the open-lock cast on a locked chest, vein or herb
/// (instead of `CMSG_GAMEOBJ_USE`), or a targeting-cursor commit. The mask is `GAMEOBJECT` alone:
/// the 1.12 client never puts `TARGET_FLAG_LOCKED` (0x4000) on the wire; `BindTarget` (`0x6e5b40`)
/// consumes it from the targeting word, and no write to the wire mask (`0xceac5c`) sets it.
pub fn cast_spell_gameobject(spell_id: u32, go_guid: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(16);
    body.extend_from_slice(&spell_id.to_le_bytes());
    body.extend_from_slice(&TARGET_FLAG_GAMEOBJECT.to_le_bytes());
    crate::wire::write_packed_guid(go_guid, &mut body).expect("vec write");
    body
}

/// The corpse bit a corpse cast carries: `BindTarget 0x6e5b40`'s corpse arm writes `0x8000` for a
/// corpse its caster's reaction toward is friendly (`0x6e5fc7`), `0x200` for one it is not
/// (`0x6e600c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseTarget {
    /// `TARGET_FLAG_CORPSE_ALLY`, a resurrection's.
    Ally,
    /// `TARGET_FLAG_CORPSE_ENEMY`.
    Enemy,
}

impl CorpseTarget {
    /// The wire mask bit, which is also the flag-word bit the bind clears.
    pub const fn target_flag(self) -> u16 {
        match self {
            Self::Ally => TARGET_FLAG_CORPSE_ALLY,
            Self::Enemy => TARGET_FLAG_CORPSE_ENEMY,
        }
    }
}

/// Body of `CMSG_CAST_SPELL` at a corpse: the corpse bit and the corpse's packed guid. The client's
/// targets writer `0x7e4e10` puts the guid for any mask bit in `0x8a02`; vmangos reads it for
/// either corpse bit (`SpellCastTargetsInfo.cpp:156-157`).
pub fn cast_spell_corpse(spell_id: u32, target: CorpseTarget, corpse_guid: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(16);
    body.extend_from_slice(&spell_id.to_le_bytes());
    body.extend_from_slice(&target.target_flag().to_le_bytes());
    crate::wire::write_packed_guid(corpse_guid, &mut body).expect("vec write");
    body
}

/// Body of `CMSG_CAST_SPELL` at a ground point, for a `Targets & 0x40` spell such as Blizzard:
/// `DEST_LOCATION` and three `f32` WoW world coords (`SpellCastTargetsInfo.cpp:169-174`; client
/// `BindLocation` `0x6e60f0`).
pub fn cast_spell_at_dest(spell_id: u32, dest: [f32; 3]) -> Vec<u8> {
    let mut body = Vec::with_capacity(18);
    body.extend_from_slice(&spell_id.to_le_bytes());
    body.extend_from_slice(&TARGET_FLAG_DEST_LOCATION.to_le_bytes());
    for c in dest {
        body.extend_from_slice(&c.to_le_bytes());
    }
    body
}

/// Body of `CMSG_CAST_SPELL` at a source point, for a `Targets & 0x20` spell: `SOURCE_LOCATION`
/// and three `f32` WoW world coords (client `0x6e60f0`, `SpellCastTargetsInfo.cpp:161-167`). The
/// server centres the AoE there (`TARGET_ENUM_UNITS_ENEMY_AOE_AT_SRC_LOC`, `Spell.cpp:2265`).
pub fn cast_spell_at_source(spell_id: u32, src: [f32; 3]) -> Vec<u8> {
    let mut body = Vec::with_capacity(18);
    body.extend_from_slice(&spell_id.to_le_bytes());
    body.extend_from_slice(&TARGET_FLAG_SOURCE_LOCATION.to_le_bytes());
    for c in src {
        body.extend_from_slice(&c.to_le_bytes());
    }
    body
}

/// Body of `CMSG_CANCEL_AURA`: by spell id, not slot. `HandleCancelAuraOpcode`
/// (`SpellHandler.cpp:333-405`) refuses passives, debuffs and `SPELL_ATTR_NO_AURA_CANCEL` spells;
/// the client gates on the aura's `AURA_FLAG_CANCELABLE` bit.
pub fn cancel_aura(spell_id: u32) -> Vec<u8> {
    spell_id.to_le_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_spell_gameobject_body_golden() {
        // spell 1, mask 0x0800 (no LOCKED 0x4000), packed guid 0x1234 (mask 0x03, bytes 34 12).
        assert_eq!(
            cast_spell_gameobject(1, 0x1234),
            [0x01, 0x00, 0x00, 0x00, 0x00, 0x08, 0x03, 0x34, 0x12],
            "CMSG_CAST_SPELL (GameObject/OPEN_LOCK) body"
        );
        // The self-cast shape stays distinct: mask 0, no guid.
        assert_eq!(cast_spell(1, None), [0x01, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    /// Resurrection (2006) at a released player's corpse, `HIGHGUID_CORPSE` 0xF101
    /// (vmangos `ObjectGuid.h:76`), counter 0x2A.
    #[test]
    fn cast_spell_corpse_body_golden() {
        const CORPSE: u64 = 0xF101_0000_0000_002A;
        assert_eq!(
            cast_spell_corpse(2006, CorpseTarget::Ally, CORPSE),
            [
                0xD6, 0x07, 0x00, 0x00, // spell id 2006
                0x00, 0x80, // TARGET_FLAG_CORPSE_ALLY
                0xC1, 0x2A, 0x01, 0xF1, // packed guid: bytes 0, 6 and 7
            ],
            "CMSG_CAST_SPELL (ally corpse) body"
        );
        // The enemy arm differs only in the mask.
        assert_eq!(
            cast_spell_corpse(2006, CorpseTarget::Enemy, CORPSE),
            [0xD6, 0x07, 0x00, 0x00, 0x00, 0x02, 0xC1, 0x2A, 0x01, 0xF1],
            "CMSG_CAST_SPELL (enemy corpse) body"
        );
    }

    #[test]
    fn cast_spell_at_dest_body_golden() {
        // Spell 10 is Blizzard.
        assert_eq!(
            cast_spell_at_dest(10, [1.0, -2.5, 3.0]),
            [
                0x0A, 0x00, 0x00, 0x00, // spell id
                0x40, 0x00, // TARGET_FLAG_DEST_LOCATION
                0x00, 0x00, 0x80, 0x3F, // x = 1.0
                0x00, 0x00, 0x20, 0xC0, // y = -2.5
                0x00, 0x00, 0x40, 0x40, // z = 3.0
            ],
            "CMSG_CAST_SPELL (ground dest) body"
        );
    }

    #[test]
    fn cast_spell_at_source_body_golden() {
        // Spell 265 is Area Death (TEST), Martin Fury's on-use.
        assert_eq!(
            cast_spell_at_source(265, [1.0, -2.5, 3.0]),
            [
                0x09, 0x01, 0x00, 0x00, // spell id 265
                0x20, 0x00, // TARGET_FLAG_SOURCE_LOCATION
                0x00, 0x00, 0x80, 0x3F, // x = 1.0
                0x00, 0x00, 0x20, 0xC0, // y = -2.5
                0x00, 0x00, 0x40, 0x40, // z = 3.0
            ],
            "CMSG_CAST_SPELL (ground source) body"
        );
        let (src, dest) = (
            cast_spell_at_source(265, [1.0, -2.5, 3.0]),
            cast_spell_at_dest(265, [1.0, -2.5, 3.0]),
        );
        assert_eq!(src.len(), dest.len());
        assert_eq!(
            src.iter().zip(&dest).filter(|(a, b)| a != b).count(),
            1,
            "source and dest bodies differ only in the mask"
        );
    }
}

/// `SMSG_SPELL_UPDATE_CHAIN_TARGETS`: a beam's hop list (vmangos `Spell.cpp:4970-4997`, client
/// `0x6e9820`). vmangos sends it only for channeled spells; the 1.12 client also fills the same
/// hop array from `SMSG_SPELL_GO`'s hits (`0x6e800d`), which is how Chain Lightning draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellChainTargets {
    pub caster: u64,
    pub spell_id: u32,
    /// In wire order. The client (`0x605780`) skips any entry equal to the caster; that filter is
    /// the consumer's.
    pub targets: Vec<u64>,
}
