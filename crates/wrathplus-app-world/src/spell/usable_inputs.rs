//! The usable walk's inputs as plain data ([`super::usable::UsableCtx`]): the current target's
//! facts and the equipment slots, read live by the action bar or from the Lua query snapshot, so
//! `IsUsableAction` and `IsUsableSpell` run one walk.

use crate::app_core::session::ObjectStore;
use crate::app_core::state::Factions;
use crate::items::Items;
use crate::net::{NetCommands, Objects, Reputations};
use crate::target::{can_assist, can_attack, RelationContext};

use super::usable::{SlotItem, WornItem, EQUIPMENT_SLOTS};

/// What legs 10/10b read of the current target (`0xb4e2d8`): its aura-state word, and
/// `CanAttack 0x606980` and `CanAssist 0x6066f0` from the player, asked once a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetFacts {
    pub aura_state: u32,
    pub can_attack: bool,
    pub can_assist: bool,
}

impl TargetFacts {
    pub fn of(
        target: &ObjectStore,
        me: &ObjectStore,
        factions: Option<&Factions>,
        reputations: &Reputations,
        relations: RelationContext,
    ) -> TargetFacts {
        TargetFacts {
            aura_state: target.0.unit_aura_state(),
            can_attack: can_attack(Some(target), factions, reputations, Some(me), relations),
            can_assist: can_assist(Some(target), factions, reputations, Some(me), relations),
        }
    }
}

/// The equipment slots as leg 4's search (`0x5f0c50`) reads them, slot by slot, so a reader that
/// asks for a missing template asks only for the slots the search reaches.
pub trait WornLookup {
    /// Equipment index `slot`'s item.
    fn slot(&self, slot: u8) -> SlotItem;
    /// The hand the disarm ladder hides (`0x5f0c69`/`0x5f0c91`), if any.
    fn disarmed(&self) -> Option<u8>;
}

/// The live slots, asking the server for a template not yet cached (the action bar, the tooltip).
pub struct WornAsking<'a, 'w, 's> {
    pub store: &'a ObjectStore,
    pub objects: &'a Objects<'w, 's>,
    pub items: &'a Items,
    pub commands: &'a NetCommands,
}

impl WornLookup for WornAsking<'_, '_, '_> {
    fn slot(&self, slot: u8) -> SlotItem {
        let guid = self.store.0.player_inv_slot(slot).unwrap_or(0);
        SlotItem::of(guid, |guid| {
            let o = self.objects.object(guid)?;
            let t = self
                .items
                .template(o.object_entry()?, guid, self.commands)?;
            Some(WornItem::new(t, o))
        })
    }

    fn disarmed(&self) -> Option<u8> {
        crate::items::disarmed_equipment_slot(self.store, self.objects, self.items, self.commands)
    }
}

/// The live slots from the cached records alone, sending nothing (the cast ladder, the snapshot).
pub struct WornCached<'a, 'w, 's> {
    pub store: &'a ObjectStore,
    pub objects: &'a Objects<'w, 's>,
    pub items: &'a Items,
}

impl WornLookup for WornCached<'_, '_, '_> {
    fn slot(&self, slot: u8) -> SlotItem {
        let guid = self.store.0.player_inv_slot(slot).unwrap_or(0);
        SlotItem::of(guid, |guid| {
            let o = self.objects.object(guid)?;
            let t = self.items.template_cached(o.object_entry()?)?;
            Some(WornItem::new(t, o))
        })
    }

    fn disarmed(&self) -> Option<u8> {
        crate::items::disarmed_equipment_slot_cached(self.store, self.objects, self.items)
    }
}

/// A frame's copy of the slots ([`WornCached`]'s answers), which the Lua query snapshot owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WornSlots {
    pub slots: [SlotItem; EQUIPMENT_SLOTS as usize],
    pub disarmed: Option<u8>,
}

impl WornSlots {
    pub fn copy(from: &dyn WornLookup) -> WornSlots {
        WornSlots {
            slots: std::array::from_fn(|i| from.slot(i as u8)),
            disarmed: from.disarmed(),
        }
    }
}

impl WornLookup for WornSlots {
    fn slot(&self, slot: u8) -> SlotItem {
        self.slots
            .get(usize::from(slot))
            .copied()
            .unwrap_or(SlotItem::Empty)
    }

    fn disarmed(&self) -> Option<u8> {
        self.disarmed
    }
}
