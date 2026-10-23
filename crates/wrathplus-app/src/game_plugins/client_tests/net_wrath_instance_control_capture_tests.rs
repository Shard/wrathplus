//! Whole-client tests of `crate::net::wrath_instance_control_capture`, from `net/wrath_instance_control_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::test_support::instance_control::dispatch;
use crate::net::wrath_instance_control_capture::*;
use wrathplus_protocol::wrath_events::WrathEventAdapter;
use wrathplus_protocol::SessionEvent;
use wrathplus_protocol::SessionEventKind;

#[test]
fn one_owner_and_both_session_boundaries_clear_captured_replies() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathInstanceControl];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_instance_control_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
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
        dispatch(&mut app, &mut adapter, &[0; 12]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(app
            .world()
            .resource::<NativeInstanceControlPackets>()
            .0
            .is_empty());
    }
}
