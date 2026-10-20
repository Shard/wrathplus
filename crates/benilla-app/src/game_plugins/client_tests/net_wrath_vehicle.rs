//! Whole-client tests of `crate::net::wrath_vehicle`, from `net/wrath_vehicle.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_vehicle::tests::CAR;
use crate::net::wrath_vehicle::*;
use benilla_protocol::wrath_vehicle::VehicleEvent;
use benilla_protocol::SessionEvent;
use benilla_protocol::SessionEventKind;
use bevy::prelude::*;

#[test]
fn one_owner_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathVehicle];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_vehicle::receive"));
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![
            SessionEvent::WrathVehicle(VehicleEvent::Kit {
                unit: CAR,
                vehicle_id: 312,
            }),
            SessionEvent::WrathVehicle(VehicleEvent::Seat {
                passenger: 0x20,
                seat: Some((CAR, 1)),
            }),
        ],
    );
    assert_eq!(app.world().resource::<VehicleWorld>().riders.len(), 1);
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::WrathVehicle(VehicleEvent::Gone {
            unit: 0x20,
        })],
    );
    assert!(app.world().resource::<VehicleWorld>().riders.is_empty());
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert_eq!(
        *app.world().resource::<VehicleWorld>(),
        VehicleWorld::default()
    );
}
