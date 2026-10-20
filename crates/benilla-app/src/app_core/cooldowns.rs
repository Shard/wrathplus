//! The spell cooldown store every layer reads: the action bars, the pet bar and the cast gates.
//! `spell::cooldowns` keeps its systems and re-exports these from its old path.

use std::time::{Duration, Instant};

use bevy::prelude::*;

/// One timer; a zero duration is untracked.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Timer {
    pub(crate) start: Instant,
    pub(crate) duration: Duration,
}

/// One `SpellHistory` node (`0x6e12c0`).
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub(crate) spell_id: u32,
    /// The cast item's entry, 0 for a spell: records match on the spell and item pair.
    pub(crate) item_id: u32,
    pub(crate) recovery: Timer,
    pub(crate) category: u32,
    /// The category's `SpellCategory` row has flags bit 0x2, so it matches every query
    /// (`0x6e1563`); wand Shoot's 351 is the only one.
    pub(crate) category_wildcard: bool,
    pub(crate) category_recovery: Timer,
    /// The recovery timers hold their durations but have not started.
    pub(crate) on_hold: bool,
    pub(crate) gcd_category: u32,
    pub(crate) gcd: Timer,
}

/// One cooldown list, the client's `SpellHistory`: the player's is this resource (`0xcecaec`), the
/// pet's is [`crate::ui_pet::PetBar::cooldowns`] (`0xcecb04`), which the pet bar, the pet book and
/// the pet's `SMSG_SPELL_GO` leg read and write. Each list's edge feeds its own events: the
/// player's `feed_action_state`, the pet's `ui_pet::bar::fire_pet_cooldown_events`.
#[derive(Resource, Default)]
pub(crate) struct Cooldowns {
    pub(crate) records: Vec<Record>,
    /// Bumped on every mutation the reference flushes (`0x4b31b0`), the flush's edge. Only counts
    /// up, across [`Self::clear_silent`] too, so a watcher reads a reset as no mutation.
    pub(crate) generation: u64,
    /// Bumped when [`Self::prune`] removes records. Kept apart from [`Self::generation`]: gated
    /// feeds must see an expiry, but the reference never fires the event for one.
    pub(crate) expiry_epoch: u64,
}
