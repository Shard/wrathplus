//! Proven branch topology of 12340 CanAttack 00729740 / CanAssist 007293d0.
//!
//! This is not full native reaction parity or server spell legality. Reaction's group/global
//! helpers and full forced-reputation comparator remain unresolved; callers supply the existing
//! directional reaction ranks. FLAGS2 bit16 / vehicle metadata guards are also untraced and
//! deliberately not guessed here. Missing creature metadata contributes no flags.

use crate::net::ObjectStore;
use benilla_protocol::messages::{ObjectType, OwnerFallback};

/// Live, caller-owned inputs: never a second object cache. The lookup is keyed for every
/// controller hop, so a missing referenced owner stays missing rather than reusing a cached one.
#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub(crate) names: Option<&'a crate::names::NameCache>,
    pub(crate) lookup: &'a dyn Fn(u64) -> Option<ObjectStore>,
}

impl Context<'_> {
    pub(crate) fn creature_flags(&self, unit: &ObjectStore) -> u32 {
        if unit.0.object_type() != Some(ObjectType::Unit) {
            return 0;
        }
        unit.0
            .object_entry()
            .and_then(|entry| self.names?.creature_record(entry))
            .map_or(0, |record| record.type_flags)
    }
}

const PC: u32 = 8;
const RAID_UNIT: u32 = 0x0400_0000;

/// Resolves CHARMEDBY, otherwise CREATEDBY, at most twice (00718b70). An unresolved
/// nonzero owner never falls back to the original unit. Direct players need no lookup.
pub(super) fn controller(
    unit: &ObjectStore,
    lookup: &mut impl FnMut(u64) -> Option<ObjectStore>,
) -> Option<ObjectStore> {
    let mut current = unit.clone();
    for _ in 0..2 {
        let Some(owner) = current.0.unit_owner(OwnerFallback::CreatedBy) else {
            return (current.0.object_type() == Some(ObjectType::Player)).then_some(current);
        };
        current = lookup(owner)?;
        if current.0.object_type() == Some(ObjectType::Player) {
            return Some(current);
        }
    }
    None
}

fn same_duel(actor: &ObjectStore, target: &ObjectStore) -> bool {
    actor.0.player_duel_team() != 0
        && target.0.player_duel_team() != 0
        && actor.0.player_duel_arbiter() == target.0.player_duel_arbiter()
}

/// Creature flags are query-response type_flags, never UNIT_FLAGS or NPC service bits.
pub(super) fn attack(
    actor: &ObjectStore,
    target: &ObjectStore,
    actor_creature_flags: u32,
    toward: u8,
    reverse: u8,
    mut lookup: impl FnMut(u64) -> Option<ObjectStore>,
) -> bool {
    let (a, t) = (actor.0.unit_flags(), target.0.unit_flags());
    let (apc, tpc) = (a & PC != 0, t & PC != 0);
    if actor.0.object_type() == Some(ObjectType::Player) && actor.0.player_flags() & (1 << 19) != 0
    {
        return false;
    }
    if target.0.object_type() == Some(ObjectType::Player)
        && target.0.player_flags() & 0x10 != 0
        && actor_creature_flags & 2 == 0
    {
        return false;
    }
    if t & ((1 << 1) | (1 << 20) | (1 << 7) | (1 << 16) | (1 << 25)) != 0 {
        return false;
    }
    if (apc && t & 0x100 != 0)
        || (!apc && t & 0x200 != 0)
        || (tpc && a & 0x100 != 0)
        || (!tpc && a & 0x200 != 0)
    {
        return false;
    }
    let friendly = toward >= 4 || actor_creature_flags & RAID_UNIT != 0;
    let sanctuary = actor.0.unit_is_in_sanctuary() || target.0.unit_is_in_sanctuary();
    match (apc, tpc) {
        (false, false) => toward < 2 || reverse < 2,
        (true, true) => {
            if friendly {
                return false;
            }
            if let (Some(ac), Some(tc)) = (
                controller(actor, &mut lookup),
                controller(target, &mut lookup),
            ) {
                if same_duel(&ac, &tc) {
                    return true;
                }
                if target.0.unit_is_pvp() {
                    return !sanctuary;
                }
                if actor.0.unit_is_ffa_pvp() && target.0.unit_is_ffa_pvp() {
                    return true;
                }
                let raw = actor.0.unit_pvp_state_flags().unwrap_or(0)
                    | target.0.unit_pvp_state_flags().unwrap_or(0);
                if raw & 2 == 0 {
                    return false;
                }
            }
            !sanctuary
        }
        _ => {
            !(apc && target.0.unit_is_in_sanctuary() || tpc && actor.0.unit_is_in_sanctuary())
                && !friendly
        }
    }
}

pub(super) fn assist(
    actor: &ObjectStore,
    target: &ObjectStore,
    actor_creature_flags: u32,
    target_creature_flags: u32,
    toward: u8,
    mut lookup: impl FnMut(u64) -> Option<ObjectStore>,
) -> bool {
    let (apc, tpc) = (
        actor.0.unit_flags() & PC != 0,
        target.0.unit_flags() & PC != 0,
    );
    let t = target.0.unit_flags();
    if t & (1 << 25) != 0 || t & if apc { 0x100 } else { 0x200 } != 0 {
        return false;
    }
    if toward < 4 && actor_creature_flags & RAID_UNIT == 0 {
        return false;
    }
    if tpc {
        if let (Some(ac), Some(tc)) = (
            controller(actor, &mut lookup),
            controller(target, &mut lookup),
        ) {
            if tc.0.player_duel_team() != 0
                && (ac.0.player_duel_arbiter() != tc.0.player_duel_arbiter()
                    || ac.0.player_duel_team() != tc.0.player_duel_team())
            {
                return false;
            }
        }
        if target.0.unit_is_ffa_pvp() && !actor.0.unit_is_ffa_pvp() {
            return false;
        }
        return !(actor.0.unit_is_in_sanctuary()
            && !target.0.unit_is_in_sanctuary()
            && target.0.unit_is_pvp());
    }
    !apc || target.0.unit_is_pvp() || target_creature_flags & (0x1000 | RAID_UNIT) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::messages::{Field, FieldLayout, ObjectFields};

    fn unit(player: bool, flags: u32, pvp: u8, owner: u64) -> ObjectStore {
        let layout = FieldLayout::Wrath12340;
        let mut pairs = vec![];
        for (field, value) in [
            (Field::ObjectType, if player { 0x19 } else { 9 }),
            (Field::UnitFlags, flags),
            (Field::UnitBytes2, u32::from(pvp) << 8),
        ] {
            pairs.push((layout.resolve(field).unwrap().index, value));
        }
        let index = layout.resolve(Field::UnitCreatedby).unwrap().index;
        pairs.extend([(index, owner as u32), (index + 1, (owner >> 32) as u32)]);
        ObjectStore(ObjectFields::from_native_pairs(layout, &pairs))
    }

    #[test]
    fn controller_resolves_two_distinct_hops_and_never_reuses_a_missing_owner() {
        let pet = unit(false, PC, 0, 11);
        let guardian = unit(false, PC, 0, 22);
        let player = unit(true, PC, 0, 0);
        let mut visits = Vec::new();
        let resolved = controller(&pet, &mut |guid| {
            visits.push(guid);
            match guid {
                11 => Some(guardian.clone()),
                22 => Some(player.clone()),
                _ => None,
            }
        });
        assert_eq!(visits, [11, 22]);
        assert_eq!(resolved.unwrap().0.object_type(), Some(ObjectType::Player));
        assert!(controller(&unit(true, PC, 0, 99), &mut |_| None).is_none());
        assert!(controller(&pet, &mut |_| Some(guardian.clone())).is_none());
        assert!(controller(&player, &mut |_| panic!("direct player lookup")).is_some());
    }

    #[test]
    fn assist_creature_metadata_is_not_a_unit_flag_or_owner_pvp_shortcut() {
        let actor = unit(true, PC, 0, 0);
        let target = unit(false, 0, 0, 99);
        for metadata in [0, 0x1000, RAID_UNIT] {
            assert_eq!(
                assist(&actor, &target, 0, metadata, 4, |_| Some(unit(
                    true, PC, 1, 0
                ))),
                metadata != 0,
            );
        }
        assert!(!assist(&actor, &unit(false, 0x1000, 0, 0), 0, 0, 4, |_| {
            None
        }));
        assert!(assist(&actor, &unit(false, 0, 1, 0), 0, 0, 4, |_| None));
        assert!(assist(&actor, &target, RAID_UNIT, 0x1000, 1, |_| None));
        assert!(!assist(&actor, &target, 0, 0x1000, 3, |_| None));
    }

    #[test]
    fn native_player_attack_ffa_precedes_sanctuary_but_target_pvp_does_not() {
        let actor = unit(true, PC, 4 | 8, 0);
        let target = unit(true, PC, 4, 0);
        assert!(attack(&actor, &target, 0, 1, 1, |_| None));
        assert!(!attack(&actor, &unit(true, PC, 1 | 4, 0), 0, 1, 1, |_| {
            None
        }));
        assert!(!attack(
            &unit(true, PC, 0, 0),
            &unit(true, PC, 0, 0),
            0,
            1,
            1,
            |_| None
        ));
        assert!(attack(
            &unit(true, PC, 2, 0),
            &unit(true, PC, 0, 0),
            0,
            1,
            1,
            |_| None
        ));
    }
}
