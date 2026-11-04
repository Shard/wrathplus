//! The host's live game state, asked inside the Lua call: `IsSpellInRange`, `IsUsableSpell` and
//! the item twins compute their answer from this frame's world when Lua asks, as the reference's
//! verbs read the object manager in the call (`0x541c60` → `0x809610`). The text measure
//! ([`super::TextMeasure`]) is the same pattern.

use mlua::Lua;

/// What an item verb hands the provider: the item entry, its use spell (the template's first
/// trigger-0 spell, `0x510ef0`) and that spell slot's category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemAsk {
    pub entry: u32,
    pub spell: u32,
    pub category: u32,
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
