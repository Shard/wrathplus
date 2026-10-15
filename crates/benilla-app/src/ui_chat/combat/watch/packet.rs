//! Native packet-snapshot aura changes and combat-log lines.
use super::*;

/// Native packet slots are not Vanilla buff/debuff indices. Only spell/stack changes word
/// lines; duration refreshes, flags and caster metadata alone are quiet.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PacketAuraChange {
    Added(u32, bool),
    Removed(u32),
    Stacked(u32, bool, u8),
}

pub(super) fn packet_aura_changes(
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

pub(super) fn aura_lines(
    ctx: &WatchCtx,
    stores: &Query<(Entity, &ObjectStore)>,
    poses: &Query<&Transform>,
    packets: Option<&crate::net::wrath_auras::PacketAuras>,
    shadow: &mut std::collections::HashMap<u64, Vec<benilla_protocol::wrath_auras::NativeAura>>,
    log: &mut ChatLog,
) {
    if let Some(packets) = packets {
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
                            super::super::AURAADDED_HELPFUL
                        } else {
                            super::super::AURAADDED_HARMFUL
                        },
                        0,
                    ),
                    PacketAuraChange::Removed(spell) => (
                        spell,
                        Some(aura_gone_kind(class)),
                        super::super::AURAREMOVED,
                        0,
                    ),
                    PacketAuraChange::Stacked(spell, helpful, count) => (
                        spell,
                        periodic_kind(class, helpful),
                        if helpful {
                            super::super::AURAAPPLICATIONADDED_HELPFUL
                        } else {
                            super::super::AURAAPPLICATIONADDED_HARMFUL
                        },
                        i64::from(count),
                    ),
                };
                let (Some(spell), Some(kind)) = (ctx.spell_name(spell_id), kind) else {
                    continue;
                };
                queue_one(
                    log,
                    ctx,
                    poses,
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
}
