//! Whole-client tests of `crate::net::wrath_movement_capture`, from `net/wrath_movement_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_movement_capture::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::wrath_movement_misc::Packet;
use wrathplus_protocol::SessionEvent;
use wrathplus_protocol::SessionEventKind;
use bevy::prelude::*;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeMovementPackets>()
        .0
        .iter()
        .cloned()
        .collect()
}

#[test]
fn one_owner_keeps_ordered_duplicates_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathMovement];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_movement_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    let dismount = Packet::Dismount { unit: 5 };
    for event in [
        SessionEvent::Connected {
            self_guid: 1,
            name: "test".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        },
        SessionEvent::Disconnected {
            reason: "test".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, 0x03ac, &[0x01, 0x05]);
        dispatch(&mut app, &mut adapter, 0x03ac, &[0x01, 0x05, 0]);
        dispatch(&mut app, &mut adapter, 0x03ac, &[0x01, 0x05]);
        assert_eq!(captured(&app), vec![dismount.clone(), dismount.clone()]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}
