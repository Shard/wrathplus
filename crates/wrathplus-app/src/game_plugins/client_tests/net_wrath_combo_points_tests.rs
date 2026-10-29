//! Whole-client tests of `crate::net::wrath_combo_points`, from `net/wrath_combo_points_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_combo_points::*;
use bevy::prelude::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::SessionEvent;
use wrathplus_protocol::SessionEventKind;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(0x039D, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn combo(app: &App) -> NativeComboPoints {
    *app.world().resource::<NativeComboPoints>()
}

#[test]
fn the_packet_is_the_wrath_source_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::ComboPoints];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_combo_points::receive"));
    let mut adapter = WrathEventAdapter::new();
    // Packed target 0x05, three points.
    dispatch(&mut app, &mut adapter, &[0x01, 0x05, 3]);
    let banked_now = NativeComboPoints {
        points: 3,
        target: 5,
    };
    assert_eq!(combo(&app), banked_now);
    // A store without the 1.12 byte (every build-12340 player) reads the packet's pair.
    assert_eq!(banked(&combo(&app)), (3, 5));
    // Malformed: ignored, the last good pair stands.
    dispatch(&mut app, &mut adapter, &[0x01, 0x05]);
    assert_eq!(combo(&app), banked_now);
    // A finisher spends them: empty target, zero points.
    dispatch(&mut app, &mut adapter, &[0x00, 0]);
    assert_eq!(combo(&app), NativeComboPoints::default());
    dispatch(&mut app, &mut adapter, &[0x01, 0x05, 3]);
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert_eq!(combo(&app), NativeComboPoints::default());
}
