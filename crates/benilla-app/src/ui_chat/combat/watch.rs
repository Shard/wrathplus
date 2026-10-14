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

/// Descriptor aura spans only exist in Vanilla. Wrath aura notifications come from packets.
fn descriptor_aura_slot(edge: &FieldChanged, applications: bool) -> Option<u16> {
    use benilla_protocol::messages::{Field, FieldLayout};
    if edge.layout != FieldLayout::Vanilla5875 {
        return None;
    }
    if applications {
        edge.unit_slot(Field::UnitAuraApplications, 12)
    } else {
        edge.unit_slot(Field::UnitAura, 48)
    }
}

/// The aura lines (`0x62b480`, `0x62b800`), one per field edge:
///
/// - a `UNIT_FIELD_AURA` slot gaining a spell words `AURAADDED*` at the periodic chat types
///   (`0x604d00`'s add arm);
/// - a slot losing one words `AURAREMOVED*` at chat types `0x41`-`0x43`; a replace words both,
///   the arrival first;
/// - a `UNIT_FIELD_AURAAPPLICATIONS` byte rising on a slot whose spell did not move words
///   `AURAAPPLICATIONADDED*` (`0x604ea0`). The reference gates it on `StackAmount > 1`, which
///   holds for any count that rises; a first application moves the spell, so it is skipped.
///
/// Harmful is the slot index, `0x20`-`0x2f` (`0x61238e`), not `UNIT_FIELD_AURAFLAGS`.
pub(crate) fn aura_lines(
    ctx: WatchCtx,
    stores: Query<(Entity, &ObjectStore)>,
    mut edges: MessageReader<FieldChanged>,
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
    /// Slots below it are helpful (`0x61238e`).
    const FIRST_HARMFUL_SLOT: u16 = 0x20;
    let harmful = |slot: u16| slot >= FIRST_HARMFUL_SLOT;

    // The whole batch first: the applications leg skips slots whose `AURA` dword moved in the
    // same block.
    let batch: Vec<FieldChanged> = edges
        .read()
        .copied()
        // An authoritative packet snapshot, even empty, supersedes transitional synthesis.
        .filter(|e| {
            !packets
                .as_deref()
                .is_some_and(|p| p.by_guid.contains_key(&e.guid))
        })
        .collect();
    let slot_edges: Vec<&FieldChanged> = batch
        .iter()
        .filter(|e| descriptor_aura_slot(e, false).is_some())
        .collect();
    let slot_moved = |entity: Entity, slot: u16| {
        slot_edges
            .iter()
            .any(|e| e.entity == entity && descriptor_aura_slot(e, false) == Some(slot))
    };

    for e in &slot_edges {
        let slot = descriptor_aura_slot(e, false).unwrap();
        let class = ctx.classify(e.guid, &stores);
        // Arrival first, so a replace words the new aura before the old one's removal.
        if e.new != 0 {
            if let (Some(spell), Some(kind)) =
                (ctx.spell_name(e.new), periodic_kind(class, !harmful(slot)))
            {
                queue_one(
                    &mut log,
                    &ctx,
                    &poses,
                    kind,
                    if harmful(slot) {
                        super::AURAADDED_HARMFUL
                    } else {
                        super::AURAADDED_HELPFUL
                    },
                    (e.guid, class),
                    ctx.ranges.class(class),
                    Fills {
                        spell,
                        ..Default::default()
                    },
                    Named::Ready,
                );
            }
        }
        if e.old != 0 {
            if let Some(spell) = ctx.spell_name(e.old) {
                queue_one(
                    &mut log,
                    &ctx,
                    &poses,
                    aura_gone_kind(class),
                    super::AURAREMOVED,
                    (e.guid, class),
                    ctx.ranges.class(class),
                    Fills {
                        spell,
                        ..Default::default()
                    },
                    Named::Ready,
                );
            }
        }
    }

    // Stack counts: four slots per dword, one byte each, holding `stack - 1`.
    for e in batch
        .iter()
        .filter(|e| descriptor_aura_slot(e, true).is_some())
    {
        let word = descriptor_aura_slot(e, true).unwrap();
        let Ok((_, store)) = stores.get(e.entity) else {
            continue; // gone in the same drain
        };
        for byte in 0..4u16 {
            let slot = word * 4 + byte;
            let (before, now) = ((e.old >> (byte * 8)) & 0xff, (e.new >> (byte * 8)) & 0xff);
            // A rise, to two or more, on a slot whose spell stayed put.
            if now <= before || now == 0 || slot_moved(e.entity, slot) {
                continue;
            }
            let Some(aura) = store.0.unit_aura(slot as u8) else {
                continue;
            };
            let Some(spell) = ctx.spell_name(aura.spell_id) else {
                continue;
            };
            let class = ctx.classify(e.guid, &stores);
            let Some(kind) = periodic_kind(class, !harmful(slot)) else {
                continue;
            };
            queue_one(
                &mut log,
                &ctx,
                &poses,
                kind,
                if harmful(slot) {
                    super::AURAAPPLICATIONADDED_HARMFUL
                } else {
                    super::AURAAPPLICATIONADDED_HELPFUL
                },
                (e.guid, class),
                ctx.ranges.class(class),
                Fills {
                    spell,
                    amount: i64::from(now + 1),
                    ..Default::default()
                },
                Named::Ready,
            );
        }
    }
}

/// Loyalty was removed in Wrath; its Bytes1 byte 1 holds pet talent points instead.
fn loyalty_levels(edge: &FieldChanged) -> Option<(u8, u8)> {
    use benilla_protocol::messages::{Field, FieldLayout};
    if edge.layout != FieldLayout::Vanilla5875
        || !edge.is_unit()
        || !edge.named_field(Field::UnitBytes1)
    {
        return None;
    }
    let before = ((edge.old >> 8) & 0xff) as u8;
    let level = ((edge.new >> 8) & 0xff) as u8;
    (level != before && level != 0 && before != 0).then_some((before, level))
}

/// The pet-loyalty line (`0x5ff860` → `0x62d440`) on `UNIT_FIELD_BYTES_1` byte 1 moving, for your
/// own pet only, at chat type `0x19` `COMBAT_MISC_INFO`. The reference prints the text through a
/// bare `"%s"`, which a family with no slots matches.
pub(crate) fn pet_loyalty_lines(
    ctx: WatchCtx,
    stores: Query<(Entity, &ObjectStore)>,
    mut edges: MessageReader<FieldChanged>,
    mut log: ResMut<ChatLog>,
) {
    for e in edges.read() {
        let Some((before, level)) = loyalty_levels(e) else {
            continue;
        };
        if ctx.classify(e.guid, &stores) != UnitClass::MyPet {
            continue;
        }
        log.push_combat(PendingCombat {
            kind: ChatEventKind::CombatMiscInfo,
            family: if level > before {
                super::PET_LOYALTY_GAIN
            } else {
                super::PET_LOYALTY_LOSS
            },
            variant: Variant::OtherOther,
            subject: 0,
            object: 0,
            fills: Fills::default(),
            named: Named::Ready,
            tries: 0,
        });
    }
}
