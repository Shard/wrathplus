//! Whole-client tests of `crate::net::wrath_world_objects_capture`, from `net/wrath_world_objects_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_world_objects_capture::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::wrath_world_objects::Packet;
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
        .resource::<NativeWorldObjectsPackets>()
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
    let owners = &census[&SessionEventKind::WrathWorldObjects];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_world_objects_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    let offer = Packet::PhaseShift { mask: 5 };
    let failed = Packet::TriggerMovie { movie: 2 };
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
        dispatch(&mut app, &mut adapter, 0x47c, &[5, 0, 0, 0]);
        dispatch(&mut app, &mut adapter, 0x47c, &[5, 0, 0, 0, 0]);
        dispatch(&mut app, &mut adapter, 0x464, &[2, 0, 0, 0]);
        dispatch(&mut app, &mut adapter, 0x47c, &[5, 0, 0, 0]);
        assert_eq!(
            captured(&app),
            vec![offer.clone(), failed.clone(), offer.clone()]
        );
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}
