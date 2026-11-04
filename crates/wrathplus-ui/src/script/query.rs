//! The host's live game state, asked inside the Lua call: `IsSpellInRange`, `IsUsableSpell` and
//! the item twins compute their answer from this frame's world when Lua asks, as the reference's
//! verbs read the object manager in the call (`0x541c60` → `0x809610`). The text measure
//! ([`super::TextMeasure`]) is the same pattern.

use mlua::Lua;

/// What an item verb hands the provider: the item entry and its use spell (the template's first
/// trigger-0 spell, `0x510ef0`; 0 for none or an uncached template).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemAsk {
    pub entry: u32,
    pub spell: u32,
}

/// The host's game state as the VM sees it. An implementation reads only its own data: it never
/// re-enters Lua, never touches the model, and never fails. Every default is `None`, which is
/// "no player": the verbs then answer the reference's no-player shape, or keep the pushed path
/// they had, so a VM with no provider (tests, the glue) behaves as before.
pub trait GameQuery {
    /// `0x809610`'s answer for `spell` cast by the player (or its pet, `pet`) at the unit `unit`:
    /// in range, out of range, or `None` (nil) when it cannot be asked.
    fn spell_in_range(&self, spell: u32, pet: bool, unit: u64) -> Option<bool> {
        let _ = (spell, pet, unit);
        None
    }

    /// `IsUsableSpell`'s pair for `spell` (the pet's for `pet`): `0x80b5d0`'s cast legs, then
    /// `0x8017e0`'s power check, as `(usable, not_enough_mana)`; `None` without the caster or the
    /// spell's row.
    fn spell_usable(&self, spell: u32, pet: bool) -> Option<(bool, bool)> {
        let _ = (spell, pet);
        None
    }

    /// `SpellHasRange`'s `0x801650` with no target, the range spell mod applied, for the player
    /// (or the pet, `pet`); `None` keeps the VM's static row.
    fn spell_has_range(&self, spell: u32, pet: bool) -> Option<bool> {
        let _ = (spell, pet);
        None
    }

    /// The cooldown manager's record for `spell` (`0x807980`, item 0) as `(start, duration,
    /// enable)` in `GetTime` seconds, `(0, 0, true)` when none runs; `None` keeps the VM's path.
    fn spell_cooldown(&self, spell: u32) -> Option<(f64, f64, bool)> {
        let _ = spell;
        None
    }

    /// `IsUsableItem`'s pair (`0x51c450`): unusable while not held (`0x754d00`) or while the use
    /// spell's record is on hold (`0x80ac10`); usable with no use spell or row (`0x51c61d`); else
    /// the use spell's cast legs and power check, as [`Self::spell_usable`].
    fn item_usable(&self, item: ItemAsk) -> Option<(bool, bool)> {
        let _ = item;
        None
    }

    /// The same for an item's use spell, keyed on the spell and the item (`0x807980`).
    fn item_cooldown(&self, item: ItemAsk) -> Option<(f64, f64, bool)> {
        let _ = item;
        None
    }
}

/// A cooldown as the verbs push it at `now`: a lapsed one reads the no-cooldown `(0, 0, 1)`, so
/// a re-feed cannot replay the finish flash; an on-hold one keeps its start with enable 0.
pub(super) fn cooldown_triple(
    now: f64,
    (start, duration, enabled): (f64, f64, bool),
) -> (f64, f64, i32) {
    if start + duration > now || !enabled {
        (start, duration, i32::from(enabled))
    } else {
        (0.0, 0.0, 1)
    }
}

/// The installed provider, its own app-data slot beside the model.
#[derive(Default)]
pub(super) struct QueryHost(Option<Box<dyn GameQuery>>);

impl super::UiScript {
    /// Install the host's game state; [`GameQuery`] says what it must not do.
    pub fn set_game_query(&mut self, query: Box<dyn GameQuery>) {
        let mut host = self.lua.app_data_mut::<QueryHost>().expect("query host");
        host.0 = Some(query);
    }
}

/// Ask the provider; `None` without one. Take no model borrow across it: the provider does not
/// need one, and a shared borrow of another slot is all this takes.
pub(super) fn ask<R>(lua: &Lua, f: impl FnOnce(&dyn GameQuery) -> Option<R>) -> Option<R> {
    let host = lua.app_data_ref::<QueryHost>()?;
    f(host.0.as_deref()?)
}
