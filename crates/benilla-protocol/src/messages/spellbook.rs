//! Spell-book and cooldown messages, all inbound. A normal cast's cooldown is tracked by the
//! client; the server sends cooldowns only for school lockouts, pets, item procs and GM resets.

/// One active cooldown from `SMSG_INITIAL_SPELLS` (vmangos `SendInitialSpells`). A permanent one,
/// re-armed by the server, is `spell_cd_ms == 1` with the category word's top bit set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellCooldown {
    /// A `u16` on the 1.12 wire, a `u32` on 3.3.5's.
    pub spell_id: u32,
    pub item_id: u16,
    pub category: u16,
    pub spell_cd_ms: u32,
    pub category_cd_ms: u32,
}
