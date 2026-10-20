//! The combat-log lines that come off descriptor edges, not packets, as in the reference: the
//! death line (the unit-death reflex `0x625190`; 1.12 has no death opcode), the aura lines (the
//! `UNIT_FIELD_AURA` and `UNIT_FIELD_AURAAPPLICATIONS` callbacks `0x604d00` and `0x604ea0`,
//! through `0x612320`, `0x6123f0` and `0x612450`) and the pet-loyalty line (`0x5ff860`).

use bevy::prelude::*;

mod packet;
#[cfg(test)]
mod tests;

use super::{
    aura_gone_kind, classify, death_kind, in_range, periodic_kind, CombatLogRanges, Family, Fills,
    Named, PendingCombat, UnitClass, Variant,
};
use crate::net::{FieldChanged, GuidIndex, ObjectStore, Reputations, SelfGuid};
use crate::target::ring::Factions;
use crate::ui_chat::{ChatEventKind, ChatLog};
use crate::ui_party::GroupState;

/// What the three watchers share to classify, name and range-gate a line.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct WatchCtx<'w> {
    pub self_guid: Res<'w, SelfGuid>,
    pub group: Res<'w, GroupState>,
    pub index: Res<'w, GuidIndex>,
    pub factions: Option<Res<'w, Factions>>,
    pub reputations: Res<'w, Reputations>,
    pub spells: Option<Res<'w, crate::ui_action::Spells>>,
    /// The live display ranges: the reference's `0x8629e0` table plus `CombatDeathLogRange`.
    pub ranges: Res<'w, CombatLogRanges>,
}

impl WatchCtx<'_> {
    fn classify(&self, guid: u64, stores: &Query<(Entity, &ObjectStore)>) -> UnitClass {
        classify(
            guid,
            &self.self_guid,
            Some(&self.group),
            &self.index,
            stores,
            self.factions.as_deref(),
            &self.reputations,
        )
    }

    /// The gates every spell-driven line takes, aura lines included: `Attributes & 0x180` or an
    /// empty name drops the line.
    fn spell_name(&self, spell_id: u32) -> Option<String> {
        let Some(display) = self.spells.as_ref().and_then(|s| s.catalog.get(spell_id)) else {
            return Some(String::new());
        };
        const DO_NOT_DISPLAY_OR_LOG: u32 = 0x180;
        if display.attributes & DO_NOT_DISPLAY_OR_LOG != 0 || display.name.is_empty() {
            return None;
        }
        Some(display.name.clone())
    }

    /// The one-sided range test 15 reference formatters run in place of the two-ended `0x626630`.
    /// The death line passes `CombatDeathLogRange`, which `0x62c160` reads at `0x62c19c`; the aura
    /// lines pass their class's range.
    fn in_range(&self, guid: u64, range: f32, poses: &Query<&Transform>) -> bool {
        in_range(guid, range, &self.self_guid, &self.index, poses)
    }
}

/// Queue one single-endpoint line, gated the way its formatter gates.
fn queue_one(
    log: &mut ChatLog,
    ctx: &WatchCtx,
    poses: &Query<&Transform>,
    kind: ChatEventKind,
    family: Family,
    subject: (u64, UnitClass),
    range: f32,
    fills: Fills,
    named: Named,
) {
    if !ctx.in_range(subject.0, range, poses) {
        return;
    }
    log.push_combat(PendingCombat {
        kind,
        family,
        variant: Variant::of(subject.1, UnitClass::Creature),
        subject: subject.0,
        object: 0,
        fills,
        named,
        tries: 0,
    });
}

/// The creating spell's (`UNIT_CREATED_BY_SPELL`) `Spell.dbc` `Effect[0]` values that word a death
/// as "destroyed", `0x62c320`'s byte table: totems, summoned objects, guardians.
const DESTROYED_EFFECTS: [u32; 10] = [50, 74, 87, 88, 89, 90, 104, 105, 106, 107];

fn is_death_edge(edge: &FieldChanged) -> bool {
    edge.is_unit()
        && edge.named_field(benilla_protocol::messages::Field::UnitHealth)
        && edge.old > 0
        && edge.new == 0
}

/// The unit-death line (`0x625190` → `0x62c160`), off the `UNIT_FIELD_HEALTH` edge as the
/// reference's watcher `0x6046f0` takes it (alive to dead at `0x6047a3`); a unit that streams in
/// dead prints nothing.
///
/// Not modelled: the reference prints the XP-award line in place of the death line while
/// `[CGUnit+0xc58]` bit 3, set in the `SMSG_LOG_XPGAIN` chain, is set (`0x62c23e`); the bit's
/// clearing site is untraced, so a kill that awards experience prints both lines here.
pub(crate) fn death_lines(
    ctx: WatchCtx,
    stores: Query<(Entity, &ObjectStore)>,
    mut edges: MessageReader<FieldChanged>,
    poses: Query<&Transform>,
    mut log: ResMut<ChatLog>,
) {
    for e in edges.read() {
        if !is_death_edge(e) {
            continue;
        }
        let Ok((_, store)) = stores.get(e.entity) else {
            continue;
        };
        let guid = e.guid;
        let class = ctx.classify(guid, &stores);
        // A summoned thing is destroyed, a living one dies, by its creating spell's first effect.
        let destroyed = store
            .0
            .unit_created_by_spell()
            .filter(|&id| id != 0)
            .and_then(|id| ctx.spells.as_ref()?.catalog.get(id))
            .is_some_and(|d| DESTROYED_EFFECTS.contains(&d.effects[0]));
        let family = match (class, destroyed) {
            (UnitClass::Me, _) => super::UNITDIES,
            (_, true) => super::UNITDESTROYEDOTHER,
            (_, false) => super::UNITDIES,
        };
        queue_one(
            &mut log,
            &ctx,
            &poses,
            death_kind(class),
            family,
            (guid, class),
            ctx.ranges.death(),
            Fills::default(),
            Named::Ready,
        );
    }
}

/// The aura lines (`0x62b480`, `0x62b800`), off the native aura packets
/// ([`packet::aura_lines`]): 12340 descriptors carry no aura slots.
pub(crate) fn aura_lines(
    ctx: WatchCtx,
    stores: Query<(Entity, &ObjectStore)>,
    poses: Query<&Transform>,
    packets: Option<Res<crate::net::wrath_auras::PacketAuras>>,
    mut shadow: Local<
        std::collections::HashMap<u64, Vec<benilla_protocol::wrath_auras::NativeAura>>,
    >,
    mut log: ResMut<ChatLog>,
) {
    packet::aura_lines(
        &ctx,
        &stores,
        &poses,
        packets.as_deref(),
        &mut shadow,
        &mut log,
    );
}
