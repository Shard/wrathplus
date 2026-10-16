use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x3f4, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn one_owner_keeps_ordered_duplicates_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathInspect];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_inspect_capture::receive"));
    // Talent inspection off: packed GUID 9, no points, no groups, no items.
    let off = [0x01, 0x09, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
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
        dispatch(&mut app, &mut adapter, &off);
        dispatch(&mut app, &mut adapter, &off[..11]);
        dispatch(&mut app, &mut adapter, &off);
        assert_eq!(app.world().resource::<NativeInspectPackets>().0.len(), 2);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(app.world().resource::<NativeInspectPackets>().0.is_empty());
    }
}
