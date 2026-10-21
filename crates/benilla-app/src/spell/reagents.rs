//! The reference's pre-send totem and reagent check (`0x6e4000`), every cast path's, and the
//! failing slot the action bar's usable state reads (`ui_action`).

use bevy::prelude::*;

use crate::app_core::errors::CastErrors;
use crate::inventory::{count_of, InventoryScope};
use crate::net::ObjectStore;

/// The reference's pre-send check `0x6e4000`, run on every cast path before any packet: the first
/// missing totem (a presence test), then the first short reagent (a count), refuses locally as
/// `0x78` or `0x5c` and returns `true`. It is the only source of "Requires Mining Pick": vmangos
/// answers a pickless cast with `ITEM_GONE` (`Spell.cpp:7301`). A missing self store skips the
/// check, as the reference's active-player gate does.
pub(crate) fn reagent_totem_refusal(
    spell_id: u32,
    def: Option<&benilla_formats::SpellDisplay>,
    self_store: Option<&ObjectStore>,
    objects: &crate::net::Objects,
    errors: &mut CastErrors,
) -> bool {
    let (Some(d), Some(store)) = (def, self_store) else {
        return false;
    };
    // Totems before reagents, the reference's order.
    let reason = if first_missing_totem(d, store, objects).is_some() {
        0x78
    } else if first_short_reagent(d, store, objects).is_some() {
        0x5c
    } else {
        return false;
    };
    debug!("cast {spell_id} refused locally — missing totem/reagent ({reason:#04x})");
    errors.push_local(spell_id, reason);
    true
}

/// The first totem item absent from our bags: `0x6e4000`'s failing slot.
pub(crate) fn first_missing_totem(
    d: &benilla_formats::SpellDisplay,
    store: &ObjectStore,
    objects: &crate::net::Objects,
) -> Option<u32> {
    d.totems
        .iter()
        .copied()
        .filter(|&t| t != 0)
        .find(|&t| count_of(&store.0, objects, t, InventoryScope::CARRIED) == 0)
}

/// The first reagent we hold too few of: `0x6e4000`'s failing slot.
pub(crate) fn first_short_reagent(
    d: &benilla_formats::SpellDisplay,
    store: &ObjectStore,
    objects: &crate::net::Objects,
) -> Option<u32> {
    d.reagents
        .iter()
        .copied()
        .filter(|&(id, _)| id != 0)
        .find(|&(id, n)| count_of(&store.0, objects, id, InventoryScope::CARRIED) < n)
        .map(|(id, _)| id)
}
