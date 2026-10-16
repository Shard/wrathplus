use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeDeathPackets>()
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
    let owners = &census[&SessionEventKind::WrathDeath];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_death_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    let deleted = Packet::PreResurrect { guid: 0x6f42 };
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
        dispatch(&mut app, &mut adapter, 0x494, &[0x03, 0x42, 0x6f]);
        dispatch(&mut app, &mut adapter, 0x494, &[0x03, 0x42]);
        dispatch(&mut app, &mut adapter, 0x494, &[0x03, 0x42, 0x6f, 0]);
        dispatch(&mut app, &mut adapter, 0x494, &[0x03, 0x42, 0x6f]);
        assert_eq!(captured(&app), vec![deleted.clone(), deleted.clone()]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}
