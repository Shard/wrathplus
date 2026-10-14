//! The combat-log lines that come off descriptor edges, not packets, as in the reference: the
//! death line (the unit-death reflex `0x625190`; 1.12 has no death opcode), the aura lines (the
//! `UNIT_FIELD_AURA` and `UNIT_FIELD_AURAAPPLICATIONS` callbacks `0x604d00` and `0x604ea0`,
//! through `0x612320`, `0x6123f0` and `0x612450`) and the pet-loyalty line (`0x5ff860`).

use bevy::prelude::*;

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

/// Native packet slots are not Vanilla buff/debuff indices. Only spell/stack changes word
/// lines; duration refreshes, flags and caster metadata alone are quiet.
#[derive(Debug, PartialEq, Eq)]
enum PacketAuraChange {
    Added(u32, bool),
    Removed(u32),
    Stacked(u32, bool, u8),
}

fn packet_aura_changes(
    old: &[benilla_protocol::wrath_auras::NativeAura],
    now: &[benilla_protocol::wrath_auras::NativeAura],
) -> Vec<PacketAuraChange> {
    let mut changes = Vec::new();
    // Arrival precedes removal on a same-slot replacement, like the descriptor callback.
    let slots: std::collections::BTreeSet<_> = old.iter().chain(now).map(|a| a.slot).collect();
    for slot in slots {
        let before = old.iter().find(|a| a.slot == slot);
        let after = now.iter().find(|a| a.slot == slot);
        if before.map(|a| a.spell_id) != after.map(|a| a.spell_id) {
            if let Some(a) = after {
                changes.push(PacketAuraChange::Added(a.spell_id, a.is_helpful()));
            }
            if let Some(a) = before {
                changes.push(PacketAuraChange::Removed(a.spell_id));
            }
        } else if let (Some(before), Some(after)) = (before, after) {
            if after.stacks > before.stacks && after.stacks > 1 {
                changes.push(PacketAuraChange::Stacked(
                    after.spell_id,
                    after.is_helpful(),
                    after.stacks,
                ));
            }
        }
    }
    changes
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
    if let Some(packets) = packets.as_deref() {
        // Despawn/disconnect drops the shadow silently, never inventing aura removals.
        shadow.retain(|guid, _| packets.by_guid.contains_key(guid));
        for (&guid, snapshot) in &packets.by_guid {
            let Some(&entity) = ctx.index.0.get(&guid) else {
                continue;
            };
            let Ok((_, store)) = stores.get(entity) else {
                continue;
            };
            if !matches!(
                store.0.created_as(),
                Some(
                    benilla_protocol::messages::ObjectType::Unit
                        | benilla_protocol::messages::ObjectType::Player
                )
            ) {
                continue;
            }
            let next: Vec<_> = snapshot.iter().map(|a| a.aura.clone()).collect();
            // First snapshot compares against empty, matching the former synthesized add edges.
            let previous = shadow.insert(guid, next.clone()).unwrap_or_default();
            let class = ctx.classify(guid, &stores);
            for change in packet_aura_changes(&previous, &next) {
                let (spell_id, kind, family, amount) = match change {
                    PacketAuraChange::Added(spell, helpful) => (
                        spell,
                        periodic_kind(class, helpful),
                        if helpful {
                            super::AURAADDED_HELPFUL
                        } else {
                            super::AURAADDED_HARMFUL
                        },
                        0,
                    ),
                    PacketAuraChange::Removed(spell) => {
                        (spell, Some(aura_gone_kind(class)), super::AURAREMOVED, 0)
                    }
                    PacketAuraChange::Stacked(spell, helpful, count) => (
                        spell,
                        periodic_kind(class, helpful),
                        if helpful {
                            super::AURAAPPLICATIONADDED_HELPFUL
                        } else {
                            super::AURAAPPLICATIONADDED_HARMFUL
                        },
                        i64::from(count),
                    ),
                };
                let (Some(spell), Some(kind)) = (ctx.spell_name(spell_id), kind) else {
                    continue;
                };
                queue_one(
                    &mut log,
                    &ctx,
                    &poses,
                    kind,
                    family,
                    (guid, class),
                    ctx.ranges.class(class),
                    Fills {
                        spell,
                        amount,
                        ..Default::default()
                    },
                    Named::Ready,
                );
            }
        }
    } else {
        shadow.clear();
    }
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

#[cfg(test)]
mod native_death_tests {
    use super::*;
    use benilla_protocol::messages::{Field, FieldLayout, ObjectType};

    #[test]
    fn death_watch_resolves_native_health_and_preserves_alive_to_dead_gate() {
        for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
            for kind in [ObjectType::Unit, ObjectType::Player] {
                let span = layout.resolve(Field::UnitHealth).unwrap();
                let (field, offset) = layout.field_at(kind, span.index).unwrap();
                let mut edge = FieldChanged {
                    entity: Entity::PLACEHOLDER,
                    guid: 42,
                    kind,
                    layout,
                    field,
                    offset,
                    old: 1,
                    new: 0,
                };
                assert!(is_death_edge(&edge));
                for (old, new) in [(0, 0), (0, 1), (2, 1)] {
                    edge.old = old;
                    edge.new = new;
                    assert!(!is_death_edge(&edge));
                }
                edge.old = 1;
                edge.new = 0;
                edge.kind = ObjectType::Corpse;
                assert!(!is_death_edge(&edge));
                edge.kind = kind;
                let span = layout.resolve(Field::UnitMaxHealth).unwrap();
                (edge.field, edge.offset) = layout.field_at(kind, span.index).unwrap();
                assert!(!is_death_edge(&edge));
            }
        }
    }
}

#[cfg(test)]
mod loyalty_tests {
    use super::*;
    use benilla_protocol::messages::{Field, FieldLayout, ObjectType};

    #[test]
    fn loyalty_edges_are_vanilla_only_not_wrath_pet_talent_points() {
        for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
            for kind in [ObjectType::Unit, ObjectType::Player] {
                let span = layout.resolve(Field::UnitBytes1).unwrap();
                let (field, offset) = layout.field_at(kind, span.index).unwrap();
                let mut edge = FieldChanged {
                    entity: Entity::PLACEHOLDER,
                    guid: 42,
                    kind,
                    layout,
                    field,
                    offset,
                    old: 2 << 8,
                    new: 3 << 8,
                };
                let vanilla = layout == FieldLayout::Vanilla5875;
                assert_eq!(loyalty_levels(&edge), vanilla.then_some((2, 3)));
                edge.old = 3 << 8;
                edge.new = 2 << 8;
                assert_eq!(loyalty_levels(&edge), vanilla.then_some((3, 2)));
                for (old, new) in [(0, 2 << 8), (2 << 8, 0), (2 << 8, (2 << 8) | 1)] {
                    edge.old = old;
                    edge.new = new;
                    assert_eq!(loyalty_levels(&edge), None);
                }
                edge.old = 2 << 8;
                edge.new = 3 << 8;
                edge.kind = ObjectType::Corpse;
                assert_eq!(loyalty_levels(&edge), None);
                edge.kind = kind;
                let span = layout.resolve(Field::UnitBytes2).unwrap();
                (edge.field, edge.offset) = layout.field_at(kind, span.index).unwrap();
                assert_eq!(loyalty_levels(&edge), None);
            }
        }
    }
}

#[cfg(test)]
mod descriptor_aura_tests {
    use super::*;
    use benilla_protocol::messages::{Field, FieldLayout, ObjectType};

    #[test]
    fn aura_watch_uses_real_vanilla_spans_and_class_gates() {
        let layout = FieldLayout::Vanilla5875;
        for (field, applications) in [
            (Field::UnitAura, false),
            (Field::UnitAuraApplications, true),
        ] {
            let span = layout.resolve(field).unwrap();
            for offset in 0..span.width {
                for kind in [ObjectType::Unit, ObjectType::Player] {
                    let (field, offset) = layout.field_at(kind, span.index + offset).unwrap();
                    let mut edge = FieldChanged {
                        entity: Entity::PLACEHOLDER,
                        guid: 42,
                        kind,
                        layout,
                        field,
                        offset,
                        old: 0,
                        new: 1,
                    };
                    assert_eq!(descriptor_aura_slot(&edge, applications), Some(offset));
                    assert_eq!(descriptor_aura_slot(&edge, !applications), None);
                    edge.kind = ObjectType::Corpse;
                    assert_eq!(descriptor_aura_slot(&edge, applications), None);
                }
            }
            assert!(FieldLayout::Wrath12340.resolve(field).is_none());
        }
        for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
            let span = layout.resolve(Field::UnitHealth).unwrap();
            let (field, offset) = layout.field_at(ObjectType::Unit, span.index).unwrap();
            let edge = FieldChanged {
                entity: Entity::PLACEHOLDER,
                guid: 42,
                kind: ObjectType::Unit,
                layout,
                field,
                offset,
                old: 1,
                new: 0,
            };
            assert_eq!(descriptor_aura_slot(&edge, false), None);
            assert_eq!(descriptor_aura_slot(&edge, true), None);
        }
    }
}

#[cfg(test)]
mod packet_aura_tests {
    use super::*;
    use benilla_protocol::wrath_auras::NativeAura;

    fn aura(slot: u8, spell_id: u32, stacks: u8) -> NativeAura {
        NativeAura {
            slot,
            spell_id,
            stacks,
            flags: 0x10,
            level: 80,
            caster_guid: None,
            max_duration_ms: Some(1000),
            remaining_ms: Some(900),
        }
    }

    #[test]
    fn native_slots_add_replace_remove_and_stack_without_vanilla_slot_limits() {
        use PacketAuraChange::*;
        let a = aura(200, 42, 1);
        assert_eq!(
            packet_aura_changes(&[], &[a.clone()]),
            vec![Added(42, true)]
        );
        let mut stacked = a.clone();
        stacked.stacks = 3;
        assert_eq!(
            packet_aura_changes(&[a.clone()], &[stacked.clone()]),
            vec![Stacked(42, true, 3)]
        );
        assert!(packet_aura_changes(&[stacked], &[a.clone()]).is_empty());
        let mut replacement = aura(200, 99, 4);
        replacement.flags = 0x80;
        assert_eq!(
            packet_aura_changes(&[a.clone()], &[replacement]),
            vec![Added(99, false), Removed(42)]
        );
        assert_eq!(packet_aura_changes(&[a], &[]), vec![Removed(42)]);
        assert!(packet_aura_changes(&[], &[]).is_empty());
    }

    #[test]
    fn refreshes_and_metadata_do_not_repeat_lines_and_polarity_comes_from_flags() {
        use PacketAuraChange::*;
        let a = aura(0, 42, 1);
        let mut refresh = a.clone();
        refresh.remaining_ms = Some(1000);
        refresh.caster_guid = Some(77);
        refresh.flags = 0x80;
        refresh.level = 79;
        assert!(packet_aura_changes(&[a], &[refresh.clone()]).is_empty());
        refresh.stacks = 2;
        assert_eq!(packet_aura_changes(&[], &[refresh]), vec![Added(42, false)]);
    }
}

#[cfg(test)]
mod packet_aura_system_tests {
    use super::*;
    use crate::net::wrath_auras::{PacketAuras, StampedAura};
    use benilla_protocol::messages::{Field, FieldLayout, ObjectFields, ObjectType};
    use benilla_protocol::wrath_auras::NativeAura;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.insert_resource(SelfGuid(Some(42)))
            .init_resource::<GroupState>()
            .init_resource::<GuidIndex>()
            .init_resource::<Reputations>()
            .init_resource::<CombatLogRanges>()
            .init_resource::<ChatLog>()
            .init_resource::<PacketAuras>()
            .add_message::<FieldChanged>()
            .add_systems(Update, aura_lines);
        let entity = app
            .world_mut()
            .spawn(ObjectStore(
                ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &[])
                    .into_created(ObjectType::Player),
            ))
            .id();
        app.world_mut()
            .resource_mut::<GuidIndex>()
            .0
            .insert(42, entity);
        (app, entity)
    }

    fn set(app: &mut App, stacks: Option<u8>) {
        let auras = stacks
            .map(|stacks| StampedAura {
                received_at: 1.0,
                aura: NativeAura {
                    slot: 200,
                    spell_id: 77,
                    flags: 0x10,
                    level: 80,
                    stacks,
                    caster_guid: None,
                    max_duration_ms: None,
                    remaining_ms: None,
                },
            })
            .into_iter()
            .collect();
        app.world_mut()
            .resource_mut::<PacketAuras>()
            .by_guid
            .insert(42, auras);
    }

    fn take(app: &mut App) -> usize {
        app.update();
        let mut log = app.world_mut().resource_mut::<ChatLog>();
        let count = log.pending_len();
        log.clear_session();
        count
    }

    #[test]
    fn authoritative_packets_suppress_projected_duplicates_even_when_empty() {
        let (mut app, entity) = app();
        let edge = |old, new| FieldChanged {
            entity,
            guid: 42,
            kind: ObjectType::Player,
            layout: FieldLayout::Vanilla5875,
            field: Field::UnitAura,
            offset: 0,
            old,
            new,
        };
        set(&mut app, Some(1));
        app.world_mut()
            .resource_mut::<Messages<FieldChanged>>()
            .write(edge(0, 77));
        assert_eq!(
            take(&mut app),
            1,
            "first packet adds once, not again via synthesis"
        );
        assert_eq!(take(&mut app), 0, "held snapshot is quiet");
        set(&mut app, Some(3));
        assert_eq!(take(&mut app), 1, "native stacks rise");
        set(&mut app, None);
        app.world_mut()
            .resource_mut::<Messages<FieldChanged>>()
            .write(edge(77, 0));
        assert_eq!(take(&mut app), 1, "authoritative empty removes once");
        assert_eq!(take(&mut app), 0);
    }

    #[test]
    fn packet_lines_keep_spell_name_attribute_classification_and_range_gates() {
        for (name, attributes, range, expected) in [
            ("Visible", 0, 100_000.0, 1),
            ("", 0, 100_000.0, 0),
            ("Hidden", 0x80, 100_000.0, 0),
            ("Hidden", 0x100, 100_000.0, 0),
            ("Visible", 0, 0.0, 0),
        ] {
            let (mut app, _) = app();
            let mut spells = crate::ui_action::Spells::empty_for_tests();
            spells.catalog = benilla_formats::SpellCatalog::from_displays(
                [(
                    77,
                    benilla_formats::SpellDisplay {
                        name: name.into(),
                        attributes,
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
            );
            app.insert_resource(spells);
            app.world_mut().resource_mut::<CombatLogRanges>().class[UnitClass::Me as usize] = range;
            set(&mut app, Some(1));
            assert_eq!(take(&mut app), expected, "{name}/{attributes:#x}/{range}");
        }
        let (mut app, _) = app();
        app.world_mut().resource_mut::<SelfGuid>().0 = None;
        set(&mut app, Some(1));
        assert_eq!(
            take(&mut app),
            0,
            "unknown classification has no periodic line"
        );
    }

    #[test]
    fn missing_objects_non_units_and_despawn_are_quiet() {
        let (mut app, entity) = app();
        app.world_mut().entity_mut(entity).insert(ObjectStore(
            ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &[])
                .into_created(ObjectType::GameObject),
        ));
        set(&mut app, Some(1));
        assert_eq!(take(&mut app), 0, "non-unit gate");
        app.world_mut().entity_mut(entity).insert(ObjectStore(
            ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &[])
                .into_created(ObjectType::Player),
        ));
        assert_eq!(take(&mut app), 1);
        app.world_mut()
            .resource_mut::<PacketAuras>()
            .by_guid
            .clear();
        app.world_mut().despawn(entity);
        app.world_mut().resource_mut::<GuidIndex>().0.clear();
        assert_eq!(take(&mut app), 0, "despawn is not an aura-removal line");
        set(&mut app, Some(1));
        assert_eq!(take(&mut app), 0, "missing object");
    }
}
