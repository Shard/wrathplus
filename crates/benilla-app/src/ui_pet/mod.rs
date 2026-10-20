//! The pet system: [`PetBar`], the pet action bar's state, and the app side of
//! `benilla_ui::script::pet`. The usual three commands, four spells and three reactions are server
//! data (vmangos `CharmInfo::InitPetActionBar`), not a layout: a possessed or charmed unit fills
//! the same ten words differently, so nothing here assumes them.

use bevy::prelude::*;

use crate::net::{GuidIndex, ObjectStore};
use crate::ui_action::CooldownEvents;
use crate::ui_script::UiInput;
use crate::ui_unit::UnitFeed;

pub(crate) mod bar;
pub(crate) mod drain;
pub(crate) mod menu;
mod net;
mod unit;
mod usable;

use bar::{feed_pet_bar, fire_pet_cooldown_events};
use drain::drain_pet_actions;
// A pet bar press, applied in call order by `crate::script_calls`.
pub(crate) use drain::pet_stop_on_old_target_clear;
pub(crate) use drain::PetPress;
use menu::{drain_pet_menu, feed_pet_menu};
use unit::feed_pet_unit;

#[cfg(test)]
pub(crate) mod press_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod usable_tests;

pub(crate) use crate::app_core::state::PetBar;
impl PetBar {
    /// `PetHasActionBar()`: a nonzero cached guid, with no alive or control check (`0x4bdc20`).
    pub(crate) fn has_bar(&self) -> bool {
        self.guid() != 0
    }

    /// The authoritative packet owner's GUID; native state is never copied into the legacy bar.
    pub(crate) fn guid(&self) -> u64 {
        self.native
            .as_ref()
            .map_or(self.spells.pet_guid, |pet| pet.guid)
    }

    pub(super) fn bar_disabled(&self) -> bool {
        self.native
            .as_ref()
            .map_or_else(|| self.spells.bar_disabled(), |pet| pet.flags & 0x0800 != 0)
    }

    /// No pet: every field back to its default, the cooldown list emptied with its counters kept
    /// ([`crate::spell::Cooldowns::clear_silent`]), for the teardown packet and the session end.
    pub(crate) fn clear(&mut self) {
        let mut cooldowns = std::mem::take(&mut self.cooldowns);
        cooldowns.clear_silent();
        *self = Self {
            cooldowns,
            ..Default::default()
        };
    }
}

pub(crate) struct UiPetPlugin;

impl Plugin for UiPetPlugin {
    fn build(&self, app: &mut App) {
        net::register(app);
        app.init_resource::<PetBar>().add_systems(
            Update,
            (
                // Feeds ride the unit feed, before the VM ticks; drains run after the input pass so
                // a click is sent that frame (a press is a script call, `PetPress`). The old-target
                // clear precedes the bar feed so Attack
                // goes dark the frame the selection moves.
                pet_stop_on_old_target_clear
                    .in_set(UnitFeed)
                    .before(feed_pet_bar),
                // After the pet snapshot: these feeds' events reach Lua at once, and their
                // handlers read `HasPetUI()`, which `crate::ui_pet_stats` pushes. The bar pushes
                // its cooldown triples before the cooldown events and the pet's flush fires after
                // them, so the buttons it wakes read the new list.
                feed_pet_bar
                    .in_set(UnitFeed)
                    .after(crate::ui_pet_stats::PetSnapshot)
                    .before(CooldownEvents),
                fire_pet_cooldown_events
                    .in_set(UnitFeed)
                    .after(CooldownEvents),
                feed_pet_unit
                    .in_set(UnitFeed)
                    .after(crate::ui_pet_stats::PetSnapshot),
                feed_pet_menu.in_set(UnitFeed),
                // After this frame's presses, as the one drain before it ran them ahead of the
                // toggles, stops and writes.
                drain_pet_actions
                    .after(UiInput)
                    .after(crate::script_calls::apply_script_calls),
                drain_pet_menu.after(UiInput),
            ),
        );
    }
}

/// The unit behind [`PetBar`]'s cached guid, plus our own guid for the ownership tests the
/// reference makes before trusting the pet's fields (`0x5ff780`, `0x612e33`), and the usability
/// predicate every pet-bar verb the reference guards asks of them ([`PetUnit::usable_pet`]).
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct PetUnit<'w, 's> {
    index: Res<'w, GuidIndex>,
    stores: Query<'w, 's, &'static ObjectStore>,
    self_guid: Res<'w, crate::net::SelfGuid>,
}

impl PetUnit<'_, '_> {
    /// The pet's descriptor, `None` while its object is not streamed: the reference's "no pet".
    pub(crate) fn store(&self, pet_guid: u64) -> Option<&ObjectStore> {
        let e = *self.index.0.get(&pet_guid)?;
        self.stores.get(e).ok()
    }

    /// The active player's descriptor, `None` while its object is not streamed: the lookup the
    /// usability predicate makes on the client's player guid (`0x4bcf89`, typemask `0x10`).
    pub(crate) fn player_store(&self) -> Option<&ObjectStore> {
        self.store(self.self_guid.0?)
    }

    /// The pet's entity under [`Self::store`]'s contract, for the pet paper doll's model.
    pub(crate) fn entity(&self, pet_guid: u64) -> Option<Entity> {
        let e = *self.index.0.get(&pet_guid)?;
        self.stores.contains(e).then_some(e)
    }
}

#[cfg(test)]
mod native_guid_tests {
    use super::*;

    #[test]
    fn native_owner_guid_precedes_legacy_and_clear_removes_both() {
        let mut bar = PetBar::default();
        bar.spells.pet_guid = 7;
        assert_eq!(bar.guid(), 7);
        bar.native = Some(benilla_protocol::wrath_pet::PetSpells {
            guid: 0xf000_0000_0000_0042,
            ..Default::default()
        });
        assert_eq!(bar.guid(), 0xf000_0000_0000_0042);
        assert!(bar.has_bar());
        bar.clear();
        assert_eq!(bar.guid(), 0);
        assert!(!bar.has_bar());
        assert!(bar.native.is_none());
    }
}
