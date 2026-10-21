//! The unit-token resolver, world state only (the guid index, the pet bar's guid, the hover pick,
//! the group roster and the object stores), so world systems resolve `"target"`, `"party1"` or
//! `"mouseover"` without the UI layer; `ui_unit` re-exports it from its old path.

use benilla_ui::script::{parse_unit_token, UnitBase, UnitTokenParse};
use bevy::prelude::*;

use crate::app_core::group::{raid_row_guid, GroupState};
use crate::app_core::state::PetBar;
use crate::net::{Guid, GuidIndex, ObjectStore, SelfPlayer};
use crate::target::{Hovered, HoveredObject, Selection};

/// The one unit-token resolver, the reference's `0x515970`: case-insensitive compares, then the
/// object manager; a caller wanting a type tests the resolved unit. [`Selection`] is a parameter
/// because [`crate::target::SelectCommit`] holds it as `ResMut`. `npc` is recognised but
/// unresolved here, a quiet nil.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct UnitTokens<'w, 's> {
    /// `Option`, like the two below: a UI-only harness lacks their plugins; a bare `Res` panics.
    index: Option<Res<'w, GuidIndex>>,
    pet: Option<Res<'w, PetBar>>,
    hovered: Option<Res<'w, Hovered>>,
    hovered_go: Option<Res<'w, HoveredObject>>,
    group: Res<'w, GroupState>,
    pub(crate) stores: Query<'w, 's, &'static ObjectStore>,
    me: Query<'w, 's, (Entity, &'static Guid), With<SelfPlayer>>,
}

impl UnitTokens<'_, '_> {
    fn me(&self) -> Option<(Entity, u64)> {
        self.me.iter().next().map(|(e, g)| (e, g.0))
    }

    /// The guid lookup (`0x468460`), `pub(crate)` for `TargetLastEnemy`, which starts from a guid.
    pub(crate) fn held(&self, guid: u64) -> Option<(Entity, u64)> {
        Some((*self.index.as_ref()?.0.get(&guid)?, guid))
    }

    /// Resolve `token` as `0x515970` does: its base, then a hop for each `target` after it. The
    /// text is read by [`parse_unit_token`], which the VM's resolver shares, so both take the same
    /// tokens; a token that names nobody, or a base or hop the object manager does not hold, is
    /// `None`.
    pub(crate) fn resolve(&self, token: &str, selection: &Selection) -> Option<(Entity, u64)> {
        let UnitTokenParse::Unit { base, hops } = parse_unit_token(token) else {
            return None;
        };
        let mut unit = self.base(base, selection)?;
        for _ in 0..hops {
            unit = self.target_of(unit.0)?;
        }
        Some(unit)
    }

    /// The held unit a token's base names, before any `target` hop.
    fn base(&self, base: UnitBase, selection: &Selection) -> Option<(Entity, u64)> {
        match base {
            UnitBase::Player => self.me(),
            UnitBase::Target => selection.target.zip(selection.guid),
            // The same pick `ui_tooltip` pushes `"mouseover"` from.
            UnitBase::Mouseover => self
                .hovered
                .as_ref()?
                .mouseover(&self.hovered_go.as_deref().copied().unwrap_or_default()),
            // Off the pet bar's cached guid, as the `"pet"` snapshot reads it.
            UnitBase::Pet => {
                let guid = self.pet.as_ref()?.guid();
                (guid != 0).then(|| self.held(guid)).flatten()
            }
            UnitBase::PartyPet(row) => self
                .party_member(row)
                .and_then(|member| self.pet_of(member, false)),
            UnitBase::RaidPet(row) => self
                .raid_member(row)
                .and_then(|member| self.pet_of(member, true)),
            UnitBase::Raid(row) => self.raid_member(row).and_then(|guid| self.held(guid)),
            UnitBase::Party(row) => self.party_member(row).and_then(|guid| self.held(guid)),
            // The interaction NPC is unresolved here, a quiet nil; so are Wrath's bases, which
            // this client does not yet track.
            UnitBase::Npc
            | UnitBase::Focus
            | UnitBase::Vehicle
            | UnitBase::Boss(_)
            | UnitBase::Arena(_)
            | UnitBase::ArenaPet(_) => None,
        }
    }

    /// One `target` hop (`0x515a0f`-`0x515a25`): the held unit's `UNIT_FIELD_TARGET`, held in turn.
    /// The lookup takes the unit typemask, so a hop off any other object, or off a unit with no
    /// target, is nobody.
    fn target_of(&self, unit: Entity) -> Option<(Entity, u64)> {
        let store = self.stores.get(unit).ok().filter(|s| s.is_unit())?;
        self.held(store.0.unit_target()?)
    }

    /// The guid in 0-based party slot `row`, the one `partyN` and `partypetN` share.
    fn party_member(&self, row: u32) -> Option<u64> {
        let slot = usize::try_from(row).ok()?;
        self.group.party_slots().nth(slot).map(|m| m.guid)
    }

    /// The guid on 0-based raid row `row`, the one `raidN` and `GetRaidRosterInfo` share.
    fn raid_member(&self, row: u32) -> Option<u64> {
        let index = usize::try_from(row).ok()?.checked_add(1)?;
        raid_row_guid(&self.group, self.me().map(|(_, g)| g), index)
    }

    /// A group member's pet, held: `CHARM`, else `SUMMON`, off the member's descriptor while it is
    /// held, else off the roster record's pet guid (`partypetN` `0x4e81d0`, `raidpetN` `0x491960`).
    /// A pet the object manager does not hold names no unit here, as any unheld unit.
    fn pet_of(&self, member: u64, raid: bool) -> Option<(Entity, u64)> {
        let live = self
            .held(member)
            .and_then(|(entity, _)| self.stores.get(entity).ok())
            .map(|store| &store.0);
        let pet = if raid {
            self.group.raid_pet_guid(member, live)
        } else {
            self.group.party_pet_guid(member, live)
        };
        self.held(pet?)
    }
}
