//! Build 12340's `UnitTargetsVehicleInRaidUI` (`0x613780` → `0x613600`) input: the units whose
//! vehicle seat carries `VehicleSeat.dbc` `flagsB & 0x8`, `VEHICLE_SEAT_FLAG_B_TARGETS_IN_RAIDUI`.
//!
//! A streamed unit's seat is its passenger seat on a streamed vehicle: the vehicle's `Vehicle.dbc`
//! kit, then the seat index ([`VehicleWorld`]). A member who is not streamed reads the seat id
//! that `SMSG_PARTY_MEMBER_STATS` carries (`GROUP_UPDATE_FLAG_VEHICLE_SEAT`, the party cache's
//! `+0x308`).
//!
//! The seats are [`crate::wrath_vehicle_seats::VehicleSeats`].

use std::collections::HashSet;

use bevy::prelude::*;

use benilla_ui::script::UiScript;

use crate::net::wrath_vehicle::VehicleWorld;
use crate::net::GuidIndex;
use crate::ui_party::GroupState;
use crate::wrath_vehicle_seats::VehicleSeats;

/// `VEHICLE_SEAT_FLAG_B_TARGETS_IN_RAIDUI`.
const TARGETS_IN_RAID_UI: u32 = 0x8;

pub(crate) struct UiRaidVehiclesPlugin;

impl Plugin for UiRaidVehiclesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            feed_raid_vehicles
                .in_set(crate::ui_unit::UnitFeed)
                .after(crate::net::wrath_vehicle::derive_vehicle_state),
        );
    }
}

/// The qualifying guids: streamed riders by their kit's seat, other members by their stats' seat.
pub(crate) fn raid_vehicle_units(
    seats: &VehicleSeats,
    world: &VehicleWorld,
    group: &GroupState,
    streamed: &dyn Fn(u64) -> bool,
) -> HashSet<u64> {
    let flagged = |seat: Option<&benilla_formats::vehicles::VehicleSeat>| {
        seat.is_some_and(|s| s.flags_b & TARGETS_IN_RAID_UI != 0)
    };
    let mut out: HashSet<u64> = world
        .riders
        .iter()
        .filter(|(_, (vehicle, seat))| {
            let kit = world.kits.get(vehicle).copied().unwrap_or(0);
            usize::try_from(*seat).is_ok_and(|i| flagged(seats.seat_of(kit, i)))
        })
        .map(|(&guid, _)| guid)
        .collect();
    out.extend(
        group
            .wrath_stats
            .iter()
            .filter(|(&guid, stats)| {
                !streamed(guid) && flagged(stats.vehicle_seat.and_then(|id| seats.seat(id)))
            })
            .map(|(&guid, _)| guid),
    );
    out
}

fn feed_raid_vehicles(
    script: Option<NonSendMut<UiScript>>,
    seats: Option<Res<VehicleSeats>>,
    world: Option<Res<VehicleWorld>>,
    group: Option<Res<GroupState>>,
    index: Res<GuidIndex>,
) {
    let (Some(mut script), Some(seats), Some(world), Some(group)) = (script, seats, world, group)
    else {
        return;
    };
    let units = raid_vehicle_units(&seats, &world, &group, &|g| index.0.contains_key(&g));
    script.set_raid_vehicle_units(units);
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_formats::vehicles::{Vehicle, VehicleCatalog, VehicleSeat};

    fn seat(id: u32, flags_b: u32) -> VehicleSeat {
        VehicleSeat {
            id,
            flags: 0,
            attachment_id: 0,
            attachment_offset: [0.0; 3],
            vehicle_ability_display: 0,
            enter_ui_sound: 0,
            exit_ui_sound: 0,
            ui_skin: 0,
            flags_b,
        }
    }

    #[test]
    fn riders_read_their_kit_seat_and_absent_members_their_stats_seat() {
        let mut seats = [0u32; 8];
        seats[..2].copy_from_slice(&[301, 302]);
        let catalog = VehicleCatalog::from_rows(
            [Vehicle {
                id: 40,
                flags: 0,
                turn_speed: 0.0,
                pitch_speed: 0.0,
                pitch_min: 0.0,
                pitch_max: 0.0,
                seats,
                ui_locomotion_type: 0,
                ui_seat_indicator_type: 0,
                power_display_id: 0,
            }],
            [seat(301, 0x8), seat(302, 0x20)],
        );
        let mut world = VehicleWorld::default();
        world.kits.insert(0x900, 40);
        world.riders.insert(0xA, (0x900, 0));
        world.riders.insert(0xB, (0x900, 1));
        let mut group = GroupState::default();
        for (guid, seat) in [(0xC, 301), (0xD, 301), (0xE, 302)] {
            group.wrath_stats.insert(
                guid,
                benilla_protocol::wrath_group::Stats {
                    vehicle_seat: Some(seat),
                    ..Default::default()
                },
            );
        }
        // 0xD is streamed and on no vehicle, so its stale stats seat does not count.
        let units = raid_vehicle_units(&VehicleSeats(catalog), &world, &group, &|g| g == 0xD);
        assert_eq!(units, HashSet::from([0xA, 0xC]));
    }
}
