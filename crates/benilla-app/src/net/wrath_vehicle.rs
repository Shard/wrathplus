//! Build 12340 vehicles for the vehicle UI: the units that are vehicles and who sits where
//! ([`VehicleWorld`], fed by `SessionEvent::WrathVehicle`), our own seat and control
//! ([`VehicleState`], re-derived each frame), and the UI's requests ([`VehicleOutbox`]), drained
//! into the wire after the UI input. The Lua getters and the outbox producers are Experience's;
//! the seat flags come from Assets' vehicle catalog by `vehicle_id` and seat.
use super::{ClientCommand, NetCommands, NetHandlerApp, SelfGuid};
use benilla_protocol::wrath_authority::VehicleControl;
use benilla_protocol::wrath_movement_misc::Command as Misc;
use benilla_protocol::wrath_vehicle::VehicleEvent;
use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::HashMap;

/// Every unit known as a vehicle and every passenger's seat.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub(crate) struct VehicleWorld {
    /// Vehicle unit → its `Vehicle.dbc` id.
    pub(crate) kits: HashMap<u64, u32>,
    /// Passenger → (vehicle, seat); party frames answer `UnitInVehicle` from it.
    pub(crate) riders: HashMap<u64, (u64, i8)>,
    /// The unit the server handed us to drive (`SMSG_CLIENT_CONTROL_UPDATE` allowing another
    /// unit), until control returns to our body: the controller's verdict, read off the same
    /// messages.
    pub(crate) driven: Option<u64>,
}

/// Our own unit's vehicle, as the vehicle UI reads it.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub(crate) struct VehicleState {
    /// The vehicle we sit on.
    pub(crate) vehicle: Option<u64>,
    /// Its `Vehicle.dbc` id, 0 while unknown.
    pub(crate) vehicle_id: u32,
    /// Our seat on it.
    pub(crate) seat: Option<i8>,
    /// The server handed us the vehicle as our mover (a seat that drives it).
    pub(crate) controlling: bool,
    /// A possessed unit we drive that is not our vehicle (the possess bar).
    pub(crate) possessed: Option<u64>,
    /// Everyone on our vehicle, by seat.
    pub(crate) passengers: Vec<(i8, u64)>,
    /// A pending aim pitch for the driven unit, which the controller takes
    /// (`player::wire_in::apply_vehicle_aim`).
    pub(crate) aim_pitch: Option<f32>,
}

/// A vehicle UI request, drained into the wire by [`drain_vehicle_requests`]. The producers are the
/// vehicle Lua names' script-call arms (Experience); until they land only the tests build these.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum VehicleRequest {
    /// Leave: dismiss the vehicle we drive (`CMSG_DISMISS_CONTROLLED_VEHICLE`), else ask to exit
    /// (`CMSG_REQUEST_VEHICLE_EXIT`).
    Exit,
    PrevSeat,
    NextSeat,
    /// A seat on `vehicle`: ours (`CMSG_REQUEST_VEHICLE_SWITCH_SEAT`) or, while we drive, an
    /// accessory's (`CMSG_CHANGE_SEATS_ON_CONTROLLED_VEHICLE`).
    SwitchSeat {
        vehicle: u64,
        seat: u8,
    },
    EjectPassenger {
        passenger: u64,
    },
    /// The aim pitch (radians), folded into the driven unit's mover pitch. Build 12340 has no aim
    /// opcode: it rides `MovementInfo` pitch, which the writer reports only while swimming or
    /// flying (flags2 `ALWAYS_ALLOW_PITCHING` is not admitted).
    AimPitch(f32),
}

#[derive(Resource, Default, Debug)]
pub(crate) struct VehicleOutbox(pub(crate) Vec<VehicleRequest>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<VehicleWorld>()
        .init_resource::<VehicleState>()
        .init_resource::<VehicleOutbox>()
        .net_handler(SessionEventKind::WrathVehicle, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear)
        .add_systems(
            Update,
            (
                derive_vehicle_state.after(super::apply::apply_net_updates),
                drain_vehicle_requests.after(crate::ui_script::UiInput),
            )
                .chain(),
        );
}

fn receive(In(event): In<SessionEvent>, mut world: ResMut<VehicleWorld>) {
    let SessionEvent::WrathVehicle(event) = event else {
        return;
    };
    match event {
        VehicleEvent::Kit { unit, vehicle_id } => {
            world.kits.insert(unit, vehicle_id);
        }
        VehicleEvent::Seat {
            passenger,
            seat: Some(seat),
        } => {
            world.riders.insert(passenger, seat);
        }
        VehicleEvent::Seat {
            passenger,
            seat: None,
        } => {
            world.riders.remove(&passenger);
        }
        VehicleEvent::Gone { unit } => {
            world.kits.remove(&unit);
            world.riders.remove(&unit);
        }
    }
}

fn clear(
    In(event): In<SessionEvent>,
    mut world: ResMut<VehicleWorld>,
    mut state: ResMut<VehicleState>,
    mut outbox: ResMut<VehicleOutbox>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        *world = VehicleWorld::default();
        *state = VehicleState::default();
        outbox.0.clear();
    }
}

/// Our seat, our vehicle's passengers and whether we drive it.
fn derive(world: &VehicleWorld, me: Option<u64>, driven: Option<u64>) -> VehicleState {
    let seated = me.and_then(|me| world.riders.get(&me).copied());
    let vehicle = seated.map(|(vehicle, _)| vehicle);
    let mut passengers: Vec<(i8, u64)> = vehicle
        .map(|vehicle| {
            world
                .riders
                .iter()
                .filter(|(_, (on, _))| *on == vehicle)
                .map(|(&passenger, &(_, seat))| (seat, passenger))
                .collect()
        })
        .unwrap_or_default();
    passengers.sort_unstable();
    VehicleState {
        vehicle,
        vehicle_id: vehicle
            .and_then(|v| world.kits.get(&v).copied())
            .unwrap_or(0),
        seat: seated.map(|(_, seat)| seat),
        controlling: driven.is_some() && driven == vehicle,
        possessed: driven.filter(|&d| Some(d) != vehicle),
        passengers,
        aim_pitch: None,
    }
}

pub(crate) fn derive_vehicle_state(
    mut world: ResMut<VehicleWorld>,
    me: Res<SelfGuid>,
    mut control: MessageReader<super::ClientControlMessage>,
    mut state: ResMut<VehicleState>,
) {
    for c in control.read() {
        match (me.0 == Some(c.mover), c.allow_move) {
            (true, true) => world.driven = None,
            (false, true) => world.driven = Some(c.mover),
            _ => {}
        }
    }
    let next = VehicleState {
        aim_pitch: state.aim_pitch,
        ..derive(&world, me.0, world.driven)
    };
    if *state != next {
        *state = next;
    }
}

/// The wire command for a request, given our vehicle state; `None` when it does not apply.
fn command_for(request: VehicleRequest, state: &VehicleState) -> Option<ClientCommand> {
    Some(match request {
        VehicleRequest::Exit if state.controlling => {
            ClientCommand::NativeVehicleControl(VehicleControl::Dismiss)
        }
        VehicleRequest::Exit => ClientCommand::NativeMovementMisc(Misc::VehicleExit),
        VehicleRequest::PrevSeat => ClientCommand::NativeMovementMisc(Misc::VehiclePrevSeat),
        VehicleRequest::NextSeat => ClientCommand::NativeMovementMisc(Misc::VehicleNextSeat),
        VehicleRequest::SwitchSeat { vehicle, seat }
            if state.controlling && Some(vehicle) != state.vehicle =>
        {
            ClientCommand::NativeVehicleControl(VehicleControl::ChangeSeats {
                accessory: vehicle,
                seat: seat as i8,
            })
        }
        VehicleRequest::SwitchSeat { vehicle, seat } => {
            ClientCommand::NativeMovementMisc(Misc::VehicleSwitchSeat { vehicle, seat })
        }
        VehicleRequest::EjectPassenger { passenger } => {
            ClientCommand::NativeMovementMisc(Misc::EjectPassenger { passenger })
        }
        VehicleRequest::AimPitch(_) => return None,
    })
}

pub(crate) fn drain_vehicle_requests(
    mut outbox: ResMut<VehicleOutbox>,
    mut state: ResMut<VehicleState>,
    commands: Res<NetCommands>,
) {
    if outbox.0.is_empty() {
        return;
    }
    for request in std::mem::take(&mut outbox.0) {
        if let VehicleRequest::AimPitch(pitch) = request {
            if state.controlling && pitch.is_finite() {
                state.aim_pitch = Some(pitch);
            }
            continue;
        }
        if let Some(command) = command_for(request, &state) {
            let _ = commands.0.send(command);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const ME: u64 = 0x10;
    use crate::net::test_support::vehicle::CAR;
    const TURRET: u64 = 0xF150_0000_0000_0008;

    pub(crate) fn world() -> VehicleWorld {
        let mut world = VehicleWorld::default();
        world.kits.insert(CAR, 312);
        world.riders.insert(ME, (CAR, 0));
        world.riders.insert(0x20, (CAR, 2));
        world.riders.insert(0x30, (TURRET, 0));
        world
    }

    #[test]
    fn our_seat_passengers_and_control_derive_from_the_riders() {
        let state = derive(&world(), Some(ME), Some(CAR));
        assert_eq!(state.vehicle, Some(CAR));
        assert_eq!((state.vehicle_id, state.seat), (312, Some(0)));
        assert!(state.controlling);
        assert_eq!(state.possessed, None);
        assert_eq!(state.passengers, vec![(0, ME), (2, 0x20)]);
        // A possessed unit while on foot.
        let mut on_foot = world();
        on_foot.riders.remove(&ME);
        let state = derive(&on_foot, Some(ME), Some(0x99));
        assert_eq!((state.vehicle, state.controlling), (None, false));
        assert_eq!(state.possessed, Some(0x99));
        assert!(state.passengers.is_empty());
    }

    #[test]
    fn requests_pick_the_native_opcode_by_control() {
        let driving = derive(&world(), Some(ME), Some(CAR));
        let riding = derive(&world(), Some(ME), None);
        assert!(matches!(
            command_for(VehicleRequest::Exit, &driving),
            Some(ClientCommand::NativeVehicleControl(VehicleControl::Dismiss))
        ));
        assert!(matches!(
            command_for(VehicleRequest::Exit, &riding),
            Some(ClientCommand::NativeMovementMisc(Misc::VehicleExit))
        ));
        assert!(matches!(
            command_for(
                VehicleRequest::SwitchSeat {
                    vehicle: TURRET,
                    seat: 1
                },
                &driving
            ),
            Some(ClientCommand::NativeVehicleControl(
                VehicleControl::ChangeSeats {
                    accessory: TURRET,
                    seat: 1
                }
            ))
        ));
        assert!(matches!(
            command_for(
                VehicleRequest::SwitchSeat {
                    vehicle: CAR,
                    seat: 1
                },
                &driving
            ),
            Some(ClientCommand::NativeMovementMisc(Misc::VehicleSwitchSeat {
                vehicle: CAR,
                seat: 1
            }))
        ));
        assert!(command_for(VehicleRequest::AimPitch(0.2), &driving).is_none());
    }
}
