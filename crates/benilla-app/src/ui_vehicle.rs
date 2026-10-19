//! The vehicle UI's app half: [`WrathVehicles`] for `benilla_ui::script::wrath_vehicle`, built
//! each frame from Wire A's vehicle state ([`VehicleState`], [`VehicleWorld`]) and the seat table
//! ([`VehicleSeats`]), and the VM's vehicle verbs drained into the [`VehicleOutbox`].
//!
//! The root vehicle is the top of the passenger chain above our vehicle (`0x74c650`). Its seats
//! are flattened depth first: a slot holding a non-player passenger that is itself a vehicle
//! stands for that vehicle's seats (`0x757550`). An out-of-range party member answers the
//! seated and using predicates from its roster status bit `0x200` and its seat from the roster's
//! vehicle seat (`0x613600`).
//!
//! Unresolved, labelled: the passenger link's entering and exiting states are not held, so a
//! streamed unit is seated and using exactly while it rides.

use std::collections::HashMap;

use bevy::prelude::*;

use benilla_formats::vehicles::VehicleCatalog;
use benilla_ui::script::{
    UiScript, UnitVehicleView, VehicleAimView, VehicleCall, VehicleIndicatorView, VehicleSeatView,
    WrathVehicles,
};

use crate::names::NameCache;
use crate::net::wrath_vehicle::{VehicleOutbox, VehicleRequest, VehicleState, VehicleWorld};
use crate::net::{Objects, SelfGuid};
use crate::player::Player;
use crate::ui_party::GroupState;
use crate::ui_script::{UiFeed, UiInput};
use crate::wrath_vehicle_seats::VehicleSeats;
use benilla_assets::{AssetSet, LockRecover, WorldAssets};

/// A party roster status bit: the member rides a vehicle.
const MEMBER_STATUS_VEHICLE: u16 = 0x200;

/// The vehicle the unit rides at the top of the chain.
fn root_of(world: &VehicleWorld, mut vehicle: u64) -> u64 {
    // Guard a malformed cycle with the chain's depth bound.
    for _ in 0..8 {
        match world.riders.get(&vehicle) {
            Some(&(parent, _)) => vehicle = parent,
            None => break,
        }
    }
    vehicle
}

/// `0x757550`: the flat seats of `vehicle`.
fn flatten(
    world: &VehicleWorld,
    seats: &VehicleCatalog,
    vehicle: u64,
    root: u64,
    is_player: &impl Fn(u64) -> bool,
    name: &impl Fn(u64) -> Option<String>,
    out: &mut Vec<VehicleSeatView>,
    depth: u8,
) {
    let Some(row) = world.kits.get(&vehicle).and_then(|&id| seats.vehicle(id)) else {
        return;
    };
    let occupant = |slot: u8| {
        world
            .riders
            .iter()
            .find(|(_, &(v, s))| v == vehicle && s == slot as i8)
            .map(|(&p, _)| p)
    };
    for slot in 0u8..8 {
        let who = occupant(slot);
        let accessory = who.filter(|&p| depth < 4 && world.kits.contains_key(&p) && !is_player(p));
        if let Some(p) = accessory {
            flatten(world, seats, p, root, is_player, name, out, depth + 1);
            continue;
        }
        if row.seats[usize::from(slot)] == 0 {
            continue;
        }
        let seat = seats.seat(row.seats[usize::from(slot)]);
        out.push(VehicleSeatView {
            vehicle,
            slot,
            flags: seat.map_or(0, |s| s.flags),
            flags_b: seat.map_or(0, |s| s.flags_b),
            root: vehicle == root,
            occupant: who,
            occupant_name: who.and_then(name),
        });
    }
}

/// The seat record a riding unit sits in.
fn rider_seat<'a>(
    world: &VehicleWorld,
    seats: &'a VehicleCatalog,
    unit: u64,
) -> Option<&'a benilla_formats::vehicles::VehicleSeat> {
    let &(vehicle, slot) = world.riders.get(&unit)?;
    let id = *world.kits.get(&vehicle)?;
    seats.seat_of(id, usize::try_from(slot).ok()?)
}

#[allow(clippy::too_many_arguments)]
fn feed_vehicles(
    script: Option<NonSendMut<UiScript>>,
    state: Res<VehicleState>,
    world: Res<VehicleWorld>,
    seats: Option<Res<VehicleSeats>>,
    me: Res<SelfGuid>,
    group: Option<Res<GroupState>>,
    objects: Objects,
    names: Res<NameCache>,
    player: Res<Player>,
    mut last: Local<crate::ui_script::VmMemo<Option<WrathVehicles>>>,
) {
    let (Some(mut script), Some(seats)) = (script, seats) else {
        return;
    };
    let seats = &seats.0;
    let is_player = |g: u64| {
        objects.object(g).and_then(|f| f.object_type())
            == Some(benilla_protocol::messages::ObjectType::Player)
    };
    // Unresolved, labelled: the reference names a creature occupant too (`0x72a000`); the
    // player-name cache answers players only.
    let name = |g: u64| names.peek(g).map(str::to_string);
    let mut units: HashMap<u64, UnitVehicleView> = HashMap::new();
    for (&rider, &(vehicle, _)) in &world.riders {
        let controlling = objects.object(rider).and_then(|f| f.unit_charm()) == Some(vehicle);
        let root = root_of(&world, vehicle);
        let mut flat = Vec::new();
        flatten(&world, seats, root, root, &is_player, &name, &mut flat, 0);
        units.insert(
            rider,
            UnitVehicleView {
                seated: true,
                using: true,
                controlling,
                seat: rider_seat(&world, seats, rider).map(|s| (s.flags, s.ui_skin)),
                seats: flat,
            },
        );
    }
    if let Some(group) = group.as_deref() {
        for (&guid, stats) in &group.wrath_stats {
            if units.contains_key(&guid) || objects.object(guid).is_some() {
                continue;
            }
            let riding = stats.status.is_some_and(|s| s & MEMBER_STATUS_VEHICLE != 0);
            let seat = stats
                .vehicle_seat
                .filter(|&id| id > 0)
                .and_then(|id| seats.seat(id))
                .map(|s| (s.flags, s.ui_skin));
            if riding || seat.is_some() {
                units.insert(
                    guid,
                    UnitVehicleView {
                        seated: riding,
                        using: riding,
                        seat,
                        ..Default::default()
                    },
                );
            }
        }
    }
    // Our own seat (`0x5fb560`): seated, and the mover our vehicle or ourselves.
    let my_seat =
        me.0.and_then(|g| rider_seat(&world, seats, g))
            .filter(|_| state.vehicle.is_some() && state.possessed.is_none());
    let drives_root = state.controlling && state.vehicle.is_some_and(|v| root_of(&world, v) == v);
    // The aiming unit (`0x5f9d20`): the vehicle when our seat controls it, else ourselves.
    let aim_unit = match (state.vehicle, my_seat) {
        (Some(v), Some(s)) if s.can_control() => Some(v),
        _ => me.0,
    };
    let aim = aim_unit
        .and_then(|u| world.kits.get(&u))
        .and_then(|&id| seats.vehicle(id))
        .map(|v| VehicleAimView {
            flags: v.flags,
            pitch_min: v.pitch_min,
            pitch_max: v.pitch_max,
            pitch: player.mover_pitch(),
        });
    let fresh = WrathVehicles {
        units,
        player_seat_flags: my_seat.map(|s| s.flags),
        drives_root,
        aim,
        has_mover: me.0.is_some(),
    };
    let last = last.get(&script);
    if last.as_ref() != Some(&fresh) {
        script.set_wrath_vehicles(fresh.clone());
        *last = Some(fresh);
    }
}

/// The seat-indicator table, pushed to each VM once.
#[derive(Resource, Default)]
struct VehicleIndicators(HashMap<u32, VehicleIndicatorView>);

fn load_indicators(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let loaded = benilla_formats::load_vehicle_indicators(&mut assets.chain.lock_recover());
    match loaded {
        Ok(table) => commands.insert_resource(VehicleIndicators(
            table
                .into_iter()
                .map(|(id, i)| {
                    (
                        id,
                        VehicleIndicatorView {
                            texture: i.texture,
                            seats: i.seats,
                        },
                    )
                })
                .collect(),
        )),
        Err(e) => warn!("vehicles: VehicleUIIndicator tables failed to load — no seat art: {e:#}"),
    }
}

fn push_indicators(
    script: Option<NonSendMut<UiScript>>,
    table: Option<Res<VehicleIndicators>>,
    mut done: Local<crate::ui_script::VmMemo<bool>>,
) {
    let (Some(mut script), Some(table)) = (script, table) else {
        return;
    };
    let done = done.get(&script);
    if !*done {
        script.set_vehicle_indicators(table.0.clone());
        *done = true;
    }
}

/// The VM's vehicle verbs, into Wire A's outbox in call order.
/// `VehicleExit`'s refusal rides the VM's engine-error queue, which the action feed fires.
fn drain_vehicle_calls(script: Option<NonSendMut<UiScript>>, mut outbox: ResMut<VehicleOutbox>) {
    let Some(mut script) = script else { return };
    for call in script.take_vehicle_calls() {
        outbox.0.push(match call {
            VehicleCall::Exit => VehicleRequest::Exit,
            VehicleCall::NextSeat => VehicleRequest::NextSeat,
            VehicleCall::PrevSeat => VehicleRequest::PrevSeat,
            VehicleCall::SwitchSeat { vehicle, seat } => {
                VehicleRequest::SwitchSeat { vehicle, seat }
            }
            VehicleCall::Eject { passenger } => VehicleRequest::EjectPassenger { passenger },
            VehicleCall::AimPitch(p) => VehicleRequest::AimPitch(p),
        });
    }
}

pub(crate) struct UiVehiclePlugin;

impl Plugin for UiVehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_indicators.after(AssetSet::Open))
            .add_systems(
                Update,
                (
                    push_indicators.in_set(UiFeed),
                    feed_vehicles
                        .in_set(UiFeed)
                        .after(crate::net::wrath_vehicle::derive_vehicle_state),
                    drain_vehicle_calls
                        .after(UiInput)
                        .before(crate::net::wrath_vehicle::drain_vehicle_requests),
                ),
            );
    }
}
