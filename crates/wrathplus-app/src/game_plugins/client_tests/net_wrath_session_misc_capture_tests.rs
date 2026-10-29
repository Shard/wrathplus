//! Whole-client tests of `crate::net::wrath_session_misc_capture`, from `net/wrath_session_misc_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_session_misc_capture::*;
use bevy::prelude::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::wrath_session_misc::Packet;
use wrathplus_protocol::SessionEvent;
use wrathplus_protocol::SessionEventKind;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeSessionMiscPackets>()
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
    let owners = &census[&SessionEventKind::WrathSessionMisc];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_session_misc_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    let offer = Packet::FeatureSystemStatus {
        complaints: 2,
        voice_chat: 0,
    };
    let failed = Packet::PlayTimeWarning {
        flags: 2,
        remaining: 0,
    };
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
        dispatch(&mut app, &mut adapter, 0x3c9, &[2, 0]);
        dispatch(&mut app, &mut adapter, 0x3c9, &[2, 0, 0]);
        dispatch(&mut app, &mut adapter, 0x2f5, &[2, 0, 0, 0, 0, 0, 0, 0]);
        dispatch(&mut app, &mut adapter, 0x3c9, &[2, 0]);
        assert_eq!(
            captured(&app),
            vec![offer.clone(), failed.clone(), offer.clone()]
        );
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}
