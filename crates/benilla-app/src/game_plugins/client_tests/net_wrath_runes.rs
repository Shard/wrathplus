//! Whole-client tests of `crate::net::wrath_runes`, from `net/wrath_runes.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_runes::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use benilla_protocol::SessionEvent;
use benilla_protocol::SessionEventKind;
use bevy::prelude::*;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn resync_convert_and_power_build_the_six_runes_and_disconnect_clears() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathRunes];
    assert_eq!(owners.len(), 1, "{owners:?}");
    let mut adapter = WrathEventAdapter::new();
    let mut resync = 6u32.to_le_bytes().to_vec();
    resync.extend([0, 255, 0, 255, 1, 0, 1, 51, 2, 255, 2, 255]);
    dispatch(&mut app, &mut adapter, 0x0487, &resync);
    dispatch(&mut app, &mut adapter, 0x0486, &[0, 3]);
    dispatch(&mut app, &mut adapter, 0x0488, &0b1000u32.to_le_bytes()[..]);
    let runes = app.world().resource::<NativeRunes>().0.unwrap();
    assert_eq!(
        runes[0],
        Rune {
            rune_type: 3,
            passed: 255
        }
    );
    assert_eq!(
        runes[2],
        Rune {
            rune_type: 1,
            passed: 0
        }
    );
    // Rune 3 was a third of the way through (51); the power mask readies it.
    assert_eq!(
        runes[3],
        Rune {
            rune_type: 1,
            passed: 255
        }
    );
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert_eq!(app.world().resource::<NativeRunes>().0, None);
}
