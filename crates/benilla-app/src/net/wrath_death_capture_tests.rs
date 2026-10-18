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

#[test]
fn the_release_location_feeds_the_map_and_map_minus_one_clears_it() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let mut body = 1u32.to_le_bytes().to_vec();
    for f in [10.0f32, 20.0, 30.0] {
        body.extend(f.to_le_bytes());
    }
    dispatch(&mut app, &mut adapter, 0x378, &body);
    assert_eq!(app.world().resource::<DeathRelease>().0, Some((1, 10.0, 20.0)));
    assert!(captured(&app).is_empty(), "the location does not enter the capture");
    let mut clear = (-1i32).to_le_bytes().to_vec();
    clear.extend([0u8; 12]);
    dispatch(&mut app, &mut adapter, 0x378, &clear);
    assert_eq!(app.world().resource::<DeathRelease>().0, None);
}
