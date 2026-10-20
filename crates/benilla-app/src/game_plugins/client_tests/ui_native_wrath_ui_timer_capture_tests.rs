//! Whole-client tests of `crate::ui_native::wrath_ui_timer_capture`, from `ui_native/wrath_ui_timer_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_ui_timer_capture::tests::dispatch;
use crate::ui_native::wrath_ui_timer_capture::*;
use benilla_protocol::wrath_events::WrathEventAdapter;
use benilla_protocol::SessionEvent;
use benilla_protocol::SessionEventKind;

#[test]
fn one_owner_and_both_session_boundaries_clear_captured_replies() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathUiTimer];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_ui_timer_capture::receive"));
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
            end: benilla_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, &[0; 4]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(app.world().resource::<NativeUiTimerPackets>().0.is_empty());
    }
}
