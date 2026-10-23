//! Build 12340's `UnitReaction` (`007251c0`), the rank `0..=7` one unit holds toward another,
//! which `CanAttack 00729740`, `CanAssist 007293d0` and every reaction colour read.
//!
//! - Player-controlled pairs first: their controllers (`00718b70`, [`controller`]) decide a duel
//!   (opposing teams hostile, one team friendly), a shared controller (friendly), or a pair of
//!   which one is the active player and the other in its party or raid (`0052c680`/`005726f0`:
//!   the template comparator alone, `0071f770`). Then mutual FFA is hostile.
//! - The active player (or its pet) toward a templated unit: a forced reaction (`005d06a0`,
//!   [`ForcedReactions`]), else, unless the controller ignores reputation (`UNIT_FIELD_FLAGS_2`
//!   `0x4`), a reputation faction answers by its at-war bit, after the contested-guard leg.
//! - Otherwise `0071f770`: toward the active player, the contested guard, a forced reaction, then
//!   the player's standing (`005d0600`, uncapped: Exalted reads 7); else the template comparator
//!   (`00715440`, [`wrathplus_formats::FactionTemplate::reaction_toward`]). A missing row reads 3.
//!
//! Unresolved, labelled: `0071f5c0`'s one-controller-missing leg compares a charmer/creator guid
//! with the global `0x00c22b60`, which is not identified; it is skipped (never hostile here).

use bevy::ecs::system::SystemParam;
use bevy::prelude::Res;

use wrathplus_formats::{faction_flags, reputation_rank, template_flags, FactionCatalog};
use wrathplus_protocol::types::Field;

use super::{controller, Context};
use crate::net::wrath_faction_capture::ForcedReactions;
use crate::net::wrath_vehicle::VehicleWorld;
use crate::net::Reputations;
use crate::app_core::session::ObjectStore;
use crate::spell::group_relation::GroupRoster;
use crate::wrath_vehicle_seats::VehicleSeats;

const PC: u32 = 8;
const IGNORE_REPUTATION: u32 = 0x4;
const PLAYER_FLAGS_CONTESTED_PVP: u32 = 0x100;
const PVP_FFA: u8 = 0x4;
/// The `Vehicle.dbc` `Flags` bit `00729740` tests on a free-standing vehicle (`0x7298ad`); it has no
/// AzerothCore name.
const VEHICLE_FLAG_GUARD: u32 = 0x2000_0000;

/// The session tables the reaction reads beside the two units, borrowed from their resources.
#[derive(Clone, Copy, Default)]
pub struct ReactionState<'a> {
    pub(crate) roster: Option<&'a GroupRoster>,
    pub(crate) forced: Option<&'a ForcedReactions>,
    pub(crate) vehicles: Option<&'a VehicleWorld>,
    pub(crate) seats: Option<&'a VehicleSeats>,
}

impl ReactionState<'_> {
    /// `00729740`'s vehicle leg (`0x7298a9`-`0x7298e3`): `unit` drives a vehicle kit whose
    /// `Vehicle.dbc` flags carry `0x20000000` and rides no other vehicle (`0074c650` returns the
    /// unit itself only then).
    pub(crate) fn guarded_vehicle(&self, unit: &ObjectStore) -> bool {
        let (Some(vehicles), Some(seats), Some(guid)) = (self.vehicles, self.seats, guid(unit))
        else {
            return false;
        };
        !vehicles.riders.contains_key(&guid)
            && vehicles
                .kits
                .get(&guid)
                .and_then(|&id| seats.0.vehicle(id))
                .is_some_and(|row| row.flags & VEHICLE_FLAG_GUARD != 0)
    }
}

/// The resources behind a [`ReactionState`], for a system param that resolves relations.
#[derive(SystemParam)]
pub(crate) struct WorldInputs<'w> {
    roster: Option<Res<'w, GroupRoster>>,
    forced: Option<Res<'w, ForcedReactions>>,
    vehicles: Option<Res<'w, VehicleWorld>>,
    seats: Option<Res<'w, VehicleSeats>>,
}

impl WorldInputs<'_> {
    pub(crate) fn state(&self) -> ReactionState<'_> {
        ReactionState {
            roster: self.roster.as_deref(),
            forced: self.forced.as_deref(),
            vehicles: self.vehicles.as_deref(),
            seats: self.seats.as_deref(),
        }
    }
}

/// Everything `UnitReaction` reads: the faction tables, the standings, the live context and the
/// active player's store (`004d3790`).
#[derive(Clone, Copy)]
pub(crate) struct Reactions<'a> {
    pub(crate) factions: Option<&'a FactionCatalog>,
    pub(crate) reputations: &'a Reputations,
    pub(crate) ctx: Context<'a>,
    pub(crate) me: Option<&'a ObjectStore>,
}

fn guid(store: &ObjectStore) -> Option<u64> {
    store
        .0
        .get_field_guid(Field::ObjectGuid, 0)
        .filter(|&g| g != 0)
}

impl Reactions<'_> {
    fn is_me(&self, unit: &ObjectStore) -> bool {
        self.me.is_some_and(|me| {
            std::ptr::eq(me, unit) || guid(me).is_some_and(|g| guid(unit) == Some(g))
        })
    }

    /// `0052c680 || 005726f0`: the active player's party slots or raid roster, or the player.
    fn in_group(&self, unit: &ObjectStore) -> bool {
        let Some(g) = guid(unit) else {
            return self.is_me(unit);
        };
        let me = self.me.and_then(guid);
        self.ctx
            .world
            .roster
            .is_some_and(|roster| roster.in_group(me, g))
            || self.is_me(unit)
    }

    fn forced(&self, faction: u32) -> Option<u8> {
        self.ctx.world.forced?.rank(faction)
    }

    /// The reputation slot's at-war bit and total standing (base for the player plus the wire's).
    fn standing(
        &self,
        catalog: &FactionCatalog,
        faction: u32,
        me: &ObjectStore,
    ) -> Option<(bool, i32)> {
        let info = catalog.reputation_faction(faction)?;
        let (flags, standing) = usize::try_from(info.rep_index)
            .ok()
            .and_then(|i| self.reputations.0.get(i))
            .copied()
            .unwrap_or((0, 0));
        let base = info.base_for(
            me.0.unit_race().unwrap_or(0),
            me.0.unit_class().unwrap_or(0),
        );
        Some((flags & faction_flags::AT_WAR != 0, base + standing))
    }

    /// `0071f770(template of from, to)`.
    fn template_reaction(&self, from: &ObjectStore, to: &ObjectStore) -> u8 {
        let rows = self.factions.and_then(|catalog| {
            let a = catalog.template(from.0.unit_faction_template()?)?;
            let b = catalog.template(to.0.unit_faction_template()?)?;
            Some((catalog, a, b))
        });
        let Some((catalog, a, b)) = rows else {
            return 3;
        };
        if to.0.unit_flags() & PC != 0 {
            if let Some(owner) = controller(to, self.ctx.stores).filter(|c| self.is_me(c)) {
                if a.flags & template_flags::CONTESTED_GUARD != 0
                    && owner.0.player_flags() & PLAYER_FLAGS_CONTESTED_PVP != 0
                {
                    return 1;
                }
                if let Some(rank) = self.forced(a.faction) {
                    return rank;
                }
                if to.0.get(Field::UnitFlags2).unwrap_or(0) & IGNORE_REPUTATION == 0 {
                    if let Some((_, total)) = self.standing(catalog, a.faction, owner) {
                        return reputation_rank(total);
                    }
                }
            }
        }
        a.reaction_toward(b) as u8
    }
}

/// `UnitReaction 007251c0`: `from`'s reaction toward `to`, `0..=7`.
pub(crate) fn unit_reaction(rx: &Reactions, from: &ObjectStore, to: &ObjectStore) -> u8 {
    if std::ptr::eq(from, to) || guid(from).is_some_and(|g| guid(to) == Some(g)) {
        return 4;
    }
    let both_pc = from.0.unit_flags() & PC != 0 && to.0.unit_flags() & PC != 0;
    if both_pc {
        let stores = rx.ctx.stores;
        // A missing controller reaches `0071f5c0`, whose leg is unresolved (module docs).
        if let (Some(c1), Some(c2)) = (controller(from, stores), controller(to, stores)) {
            let (t1, t2) = (c1.0.player_duel_team(), c2.0.player_duel_team());
            if t1 != 0 && t2 != 0 && c1.0.player_duel_arbiter() == c2.0.player_duel_arbiter() {
                return if t1 != t2 { 1 } else { 4 };
            }
            if std::ptr::eq(c1, c2) || guid(c1).is_some_and(|g| guid(c2) == Some(g)) {
                return 4;
            }
            if (rx.is_me(c1) && rx.in_group(c2)) || (rx.is_me(c2) && rx.in_group(c1)) {
                return rx.template_reaction(from, to);
            }
        }
        let ffa = |u: &ObjectStore| u.0.unit_pvp_state_flags().unwrap_or(0) & PVP_FFA != 0;
        if ffa(from) && ffa(to) {
            return 1;
        }
    }
    if from.0.unit_flags() & PC != 0 {
        if let Some(rank) = reputation_leg(rx, from, to) {
            return rank;
        }
    }
    rx.template_reaction(from, to)
}

/// `0x7253ca`-`0x72540f`: the active player (or what it controls) toward a templated unit.
fn reputation_leg(rx: &Reactions, from: &ObjectStore, to: &ObjectStore) -> Option<u8> {
    let owner = controller(from, rx.ctx.stores).filter(|c| rx.is_me(c))?;
    let catalog = rx.factions?;
    let row = catalog.template(to.0.unit_faction_template()?)?;
    if let Some(rank) = rx.forced(row.faction) {
        return Some(rank);
    }
    if owner.0.get(Field::UnitFlags2).unwrap_or(0) & IGNORE_REPUTATION != 0 {
        return None;
    }
    let (at_war, _) = rx.standing(catalog, row.faction, owner)?;
    if row.flags & template_flags::CONTESTED_GUARD != 0
        && owner.0.player_flags() & PLAYER_FLAGS_CONTESTED_PVP != 0
    {
        return Some(1);
    }
    Some(if at_war { 1 } else { 4 })
}

#[cfg(test)]
#[path = "wrath_reaction_tests.rs"]
mod tests;
