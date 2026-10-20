//! Pet messages: the pet action bar's state packet and the client verbs that drive it. The bar's
//! contents are the server's; its state (lit command, lit reaction, autocast) is the client's,
//! applied on the press, because the server never answers these verbs.

/// Pet bar slots: vmangos `MAX_UNIT_ACTION_BAR_INDEX`, FrameXML's `NUM_PET_ACTION_SLOTS`.
pub const PET_ACTION_SLOTS: usize = 10;

/// vmangos `ActiveStates` (`UnitDefines.h:724-732`): the server's top byte of a packed word; the
/// client decodes the masked [`PetActionEntry::kind`] instead. Passive: shown, never clickable.
pub const PET_ACT_PASSIVE: u8 = 0x01;
/// A castable spell with autocast off: `0x01 | ` [`PET_AUTOCAST_ALLOWED`]`>>24`.
pub const PET_ACT_DISABLED: u8 = 0x81;
/// A castable spell with autocast on: `0x81 | ` [`PET_AUTOCAST_ON`]`>>24`.
pub const PET_ACT_ENABLED: u8 = 0xC1;
/// A command token: the action is a [`PET_COMMAND_STAY`]-family value, not a spell id.
pub const PET_ACT_COMMAND: u8 = 0x07;
/// A reaction token: the action is a [`PET_REACT_PASSIVE`]-family value, not a spell id.
pub const PET_ACT_REACTION: u8 = 0x06;

/// First spell-branch slot type. The client's type is the top byte `& 0x3F` (`0x4bdccd`), which
/// folds all three spell `ActiveStates` onto 1; types 6 and 7 are reaction and command.
pub const PET_TYPE_SPELL_FIRST: u8 = 1;
/// The last type the client's jump table routes to the spell branch.
pub const PET_TYPE_SPELL_LAST: u8 = 5;

/// Bit 31 of a packed word: the slot may autocast (`0x4bdd65`), given a `Spell.dbc` record.
pub const PET_AUTOCAST_ALLOWED: u32 = 0x8000_0000;
/// Bit 30: autocast is running (`0x4bdda4`); `TogglePetAutocast` flips it before the send.
pub const PET_AUTOCAST_ON: u32 = 0x4000_0000;

/// vmangos `CommandStates` (`UnitDefines.h:755-761`): a [`PET_ACT_COMMAND`] slot's action.
pub const PET_COMMAND_STAY: u32 = 0;
pub const PET_COMMAND_FOLLOW: u32 = 1;
pub const PET_COMMAND_ATTACK: u32 = 2;
pub const PET_COMMAND_DISMISS: u32 = 3;

/// vmangos `ReactStates` (`UnitDefines.h:734-739`): a [`PET_ACT_REACTION`] slot's action.
pub const PET_REACT_PASSIVE: u32 = 0;
pub const PET_REACT_DEFENSIVE: u32 = 1;
pub const PET_REACT_AGGRESSIVE: u32 = 2;

/// Bit 27 of [`PetSpells::state`], vmangos's fourth byte `0x8` (`Player.cpp:17536`): the bar is
/// disabled, its lit reaction reads Passive and it is unusable, but it still draws (`0x4bd08d`).
pub const PET_STATE_BAR_DISABLED: u32 = 0x0800_0000;

/// The pet's `UNIT_FIELD_FLAGS` bits that make its bar unusable: stunned, confused, fleeing
/// (`0x4bd075`). `UNIT_FLAG_POSSESSED` is not among them: a possessed unit's bar must work.
pub const PET_UNUSABLE_UNIT_FLAGS: u32 = 0x0004_0000 | 0x0040_0000 | 0x0080_0000;

/// Permanent-cooldown marker OR-ed into a pet cooldown's category duration (vmangos
/// `Unit.cpp:11270-11271`); strip it before using the number as a duration.
pub const PET_COOLDOWN_PERMANENT: u32 = 0x0800_0000;

/// One packed pet action word, as the client decodes it (`0x4bdcc7`/`0x4bdccd`/`0x4bdce3`): the
/// action in bits 0-15, the type in 24-29, autocast allowed in 31 and on in 30; bits 16-23 are
/// never read. The raw word is kept because the client echoes it verbatim in [`pet_action`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PetActionEntry {
    pub packed: u32,
}

impl PetActionEntry {
    /// Bits 0-15: the spell id, command state or react state.
    pub fn action(self) -> u32 {
        self.packed & 0xFFFF
    }
    /// The client's slot type, bits 24-29.
    pub fn kind(self) -> u8 {
        ((self.packed >> 24) & 0x3F) as u8
    }
    /// Client types 1-5 take the spell branch. Deviation: a type outside 1-7 reads as nothing
    /// rather than the reference's default arm, which under-pushes its returns.
    pub fn is_spell(self) -> bool {
        (PET_TYPE_SPELL_FIRST..=PET_TYPE_SPELL_LAST).contains(&self.kind())
    }
    /// Empty is a zero word (`0x4bdcbd`). vmangos's unused slots, `0x8100_0000`, take the spell
    /// branch instead and come back nil, since spell 0 has no `Spell.dbc` record.
    pub fn is_empty(self) -> bool {
        self.packed == 0
    }
    /// Bit 31; the client also requires a `Spell.dbc` record, which the catalog holder checks.
    pub fn autocast_allowed(self) -> bool {
        self.packed & PET_AUTOCAST_ALLOWED != 0
    }
    /// Bit 30: autocast is running.
    pub fn autocast_on(self) -> bool {
        self.packed & PET_AUTOCAST_ON != 0
    }
    /// Bit 30 set or cleared: `TogglePetAutocast`'s in-place flip (`0x4bcbff`), before the send.
    pub fn with_autocast(self, on: bool) -> Self {
        Self {
            packed: if on {
                self.packed | PET_AUTOCAST_ON
            } else {
                self.packed & !PET_AUTOCAST_ON
            },
        }
    }
}

impl From<u32> for PetActionEntry {
    fn from(packed: u32) -> Self {
        Self { packed }
    }
}

/// One decoded entry of `SMSG_PET_SPELLS`' trailing cooldown block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetSpellCooldown {
    pub spell_id: u32,
    pub category: u16,
    pub spell_cd_ms: u32,
    /// The category remainder, possibly carrying [`PET_COOLDOWN_PERMANENT`] in its top bits.
    pub category_cd_ms: u32,
}

/// `SMSG_PET_SPELLS`: the pet action bar's entire state in one body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PetSpells {
    /// The pet, possessed minion or charmed creature; zero means the bar is gone.
    pub pet_guid: u64,
    /// The possess or charm aura's remaining ms; 0 for a real pet.
    pub duration_ms: u32,
    /// The mode word, one dword as the client keeps it (`[0xb71468]`): react state in byte 0,
    /// command state in byte 1, [`PET_STATE_BAR_DISABLED`] at bit 27. Do not split it into bytes.
    pub state: u32,
    /// The ten bar slots, in bar order.
    pub bar: [PetActionEntry; PET_ACTION_SLOTS],
    /// The pet spellbook, packed like the bar; only a permanent pet has one (`Player.cpp:17547`).
    pub spells: Vec<PetActionEntry>,
    pub cooldowns: Vec<PetSpellCooldown>,
}

impl PetSpells {
    /// Byte 0 of [`Self::state`], a [`PET_REACT_PASSIVE`]-family value.
    pub fn react_state(&self) -> u32 {
        self.state & 0xFF
    }
    /// `state >> 8`, unmasked like the client's compare (`0x4bdf0f`): with bit 27 set it matches
    /// no command state, so every command button goes dark. Masking it would lose that.
    pub fn command_state(&self) -> u32 {
        self.state >> 8
    }
    /// Bit 27: the server has disabled the bar.
    pub fn bar_disabled(&self) -> bool {
        self.state & PET_STATE_BAR_DISABLED != 0
    }
}

/// `SMSG_PET_MODE`: the state word without the bar, for a command or reaction change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PetMode {
    pub pet_guid: u64,
    /// The same dword as [`PetSpells::state`]; the client stores both through `0x4bc930`.
    pub state: u32,
}

/// The `GlobalStrings.lua` key filling `ERR_TAME_FAILED`'s `%s`: the reference's table
/// (`0x6e6ac0`) covers `1..=11`, and anything else, vmangos's 12 included, is the default arm.
pub fn pet_tame_failure_key(reason: u8) -> &'static str {
    match reason {
        1 => "PETTAME_INVALIDCREATURE",
        2 => "PETTAME_TOOMANY",
        3 => "PETTAME_CREATUREALREADYOWNED",
        4 => "PETTAME_NOTTAMEABLE",
        5 => "PETTAME_ANOTHERSUMMONACTIVE",
        6 => "PETTAME_UNITSCANTTAME",
        7 => "PETTAME_NOPETAVAILABLE",
        8 => "PETTAME_INTERNALERROR",
        9 => "PETTAME_TOOHIGHLEVEL",
        10 => "PETTAME_DEAD",
        11 => "PETTAME_NOTDEAD",
        _ => "PETTAME_UNKNOWNERROR",
    }
}

/// Sound selector for an accepted order (vmangos `PET_TALK_SPECIAL_SPELL`, `PetHandler.cpp:523`).
pub const PET_TALK_ORDER: u32 = 0;
/// Sound selector for an attack order (vmangos `PET_TALK_ATTACK`, `Unit.cpp:8961`).
pub const PET_TALK_ATTACK: u32 = 1;

/// Body of `CMSG_PET_ACTION` (opcode 373, vmangos `Packets/Pet.cpp:9-14`): `u64` pet guid, the
/// slot's `u32` word echoed verbatim, `u64` target guid (0 when the action needs none).
pub fn pet_action(pet_guid: u64, packed: u32, target_guid: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(20);
    body.extend_from_slice(&pet_guid.to_le_bytes());
    body.extend_from_slice(&packed.to_le_bytes());
    body.extend_from_slice(&target_guid.to_le_bytes());
    body
}

/// Body of `CMSG_PET_STOP_ATTACK` (opcode 746, vmangos `Packets/Pet.cpp:33-36`): the pet guid,
/// sent by the Attack button's second press (`PetActionBarFrame.lua:258-262`).
pub fn pet_stop_attack(pet_guid: u64) -> Vec<u8> {
    pet_guid.to_le_bytes().to_vec()
}

/// Body of `CMSG_PET_CANCEL_AURA` (opcode 619, sender `0x4bd25f`): `u64` pet guid, `u32` spell.
/// The server drops it unless the guid is our pet or charm and answers a dead pet with
/// `FEEDBACK_PET_DEAD` (vmangos `SpellHandler.cpp:407-432`).
pub fn pet_cancel_aura(pet_guid: u64, spell_id: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&pet_guid.to_le_bytes());
    body.extend_from_slice(&spell_id.to_le_bytes());
    body
}

/// Body of `CMSG_PET_SET_ACTION` (opcode 372): `u64` pet guid, then one or two
/// `(u32 position, u32 packed)` pairs, counted by body size alone (24 bytes is two, vmangos
/// `Packets/Pet.cpp:52-62`), so one pair must not be padded. It carries the bar's autocast toggle
/// (one pair, bit 30 flipped, `0x4bcc1e`) and its drag (`0x4bc9a0`), whose optional first pair
/// relocates the target slot's occupant; vmangos requires it when a command or reaction moves.
pub fn pet_set_action(pet_guid: u64, entries: &[(u32, u32)]) -> Vec<u8> {
    let mut body = Vec::with_capacity(8 + 8 * entries.len());
    body.extend_from_slice(&pet_guid.to_le_bytes());
    for &(position, packed) in entries {
        body.extend_from_slice(&position.to_le_bytes());
        body.extend_from_slice(&packed.to_le_bytes());
    }
    body
}

/// Body of `CMSG_PET_ABANDON` (opcode 374, vmangos `Packets/Pet.cpp:16-19`): the pet guid. The
/// menu's Abandon and Dismiss both send it; the server deletes a hunter pet and unsummons others.
pub fn pet_abandon(pet_guid: u64) -> Vec<u8> {
    pet_guid.to_le_bytes().to_vec()
}

/// Body of `CMSG_PET_RENAME` (opcode 375, vmangos `Packets/Pet.cpp:21-25`): the pet guid, then
/// the name NUL-terminated. The 12-letter cap is the popup's; the server refuses a bad name with
/// `SMSG_PET_NAME_INVALID` rather than truncating.
pub fn pet_rename(pet_guid: u64, name: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(9 + name.len());
    body.extend_from_slice(&pet_guid.to_le_bytes());
    body.extend_from_slice(name.as_bytes());
    body.push(0);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_autocast_flip_moves_only_bit_30() {
        let on = PetActionEntry::from(3010 | (u32::from(PET_ACT_ENABLED) << 24));
        assert!(on.autocast_allowed() && on.autocast_on());

        let off = on.with_autocast(false);
        assert_eq!(off.packed, 3010 | (u32::from(PET_ACT_DISABLED) << 24));
        assert_eq!(off.action(), 3010);
        assert_eq!(off.kind(), PET_TYPE_SPELL_FIRST, "the TYPE never moves");
        assert!(off.autocast_allowed() && !off.autocast_on());
        assert_eq!(off.with_autocast(true), on);

        // Bit 31 is the server's to grant; flipping bit 30 on a passive spell does not forge it.
        let passive = PetActionEntry::from(3010 | (u32::from(PET_ACT_PASSIVE) << 24));
        assert!(!passive.with_autocast(true).autocast_allowed());
    }

    #[test]
    fn the_type_mask_collapses_the_spell_states() {
        for act in [PET_ACT_PASSIVE, PET_ACT_DISABLED, PET_ACT_ENABLED] {
            let e = PetActionEntry::from(3010 | (u32::from(act) << 24));
            assert_eq!(e.kind(), PET_TYPE_SPELL_FIRST, "{act:#04x} masks to 1");
            assert!(e.is_spell());
        }
        assert_eq!(
            PetActionEntry::from(u32::from(PET_ACT_COMMAND) << 24).kind(),
            PET_ACT_COMMAND,
            "the token types are below the mask, so they survive it unchanged"
        );
        assert_eq!(
            PetActionEntry::from(u32::from(PET_ACT_REACTION) << 24).kind(),
            PET_ACT_REACTION
        );

        // A type outside 1-7 is neither a spell nor a token.
        let odd = PetActionEntry::from(3010 | (0x33u32 << 24));
        assert!(!odd.is_spell());
        assert!(odd.kind() != PET_ACT_COMMAND && odd.kind() != PET_ACT_REACTION);
    }
}

/// `SMSG_PET_UNLEARN_CONFIRM` (`0x5e4a26`): the trainer's guid and the unlearn cost in copper,
/// which the reference latches before firing `CONFIRM_PET_UNLEARN(cost)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PetUnlearnConfirm {
    pub trainer: u64,
    pub cost: u32,
}
