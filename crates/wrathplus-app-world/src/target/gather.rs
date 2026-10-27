//! A creature corpse's gather spell, as 12340's `0x53bce0` picks it: the trade the corpse's
//! template names (`0x715e50`, [`gather_kind`]) and the spell learned for that trade
//! ([`LearnedAbilities::gather`]). The skin cursor, the right-click and the arrival all ask here,
//! so a miner over a boar sees no gather cursor and casts nothing.

use bevy::ecs::system::SystemParam;
use bevy::prelude::Res;

use crate::app_core::gather::gather_kind;
use crate::app_core::session::ObjectStore;
use crate::app_core::state::LearnedAbilities;
use wrathplus_protocol::types::ObjectType;

#[derive(SystemParam)]
pub(crate) struct GatherSpells<'w> {
    pub(crate) learned: Res<'w, LearnedAbilities>,
    /// The template cache, for `type_flags`; absent or uncached reads as Skinning, as the
    /// reference's `0x715e50` does for a unit with no template.
    names: Option<Res<'w, crate::names::NameCache>>,
}

impl GatherSpells<'_> {
    /// The corpse's gather trade, `0..=3`.
    pub(crate) fn kind(&self, unit: &ObjectStore) -> usize {
        let flags = (unit.0.object_type() == Some(ObjectType::Unit))
            .then(|| unit.0.object_entry())
            .flatten()
            .and_then(|entry| self.names.as_ref()?.creature_record(entry))
            .map_or(0, |record| record.type_flags);
        gather_kind(flags)
    }

    /// The spell a right-click on this corpse casts, if its trade is learned.
    pub(crate) fn spell_for(&self, unit: &ObjectStore) -> Option<u32> {
        self.learned.gather[self.kind(unit)]
    }
}
