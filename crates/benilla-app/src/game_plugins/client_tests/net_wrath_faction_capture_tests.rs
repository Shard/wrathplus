//! Whole-client tests of `crate::net::wrath_faction_capture`, from `net/wrath_faction_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_faction_capture::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use benilla_protocol::SessionEvent;
use benilla_protocol::SessionEventKind;
use bevy::prelude::*;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x2a5, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn one_owner_replaces_the_forced_list_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathFaction];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_faction_capture::receive"));
    let one = [1, 0, 0, 0, 72, 0, 0, 0, 4, 0, 0, 0];
    let two = [2, 0, 0, 0, 72, 0, 0, 0, 7, 0, 0, 0, 72, 0, 0, 0, 1, 0, 0, 0];
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
        dispatch(&mut app, &mut adapter, &two);
        dispatch(&mut app, &mut adapter, &one[..11]);
        let forced = app.world().resource::<ForcedReactions>();
        assert_eq!(
            forced.rank(72),
            Some(7),
            "the first row naming a faction wins"
        );
        dispatch(&mut app, &mut adapter, &one);
        let forced = app.world().resource::<ForcedReactions>();
        assert_eq!(forced.0, vec![(72, 4)], "each packet replaces the list");
        assert_eq!(forced.rank(47), None);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(app.world().resource::<ForcedReactions>().0.is_empty());
    }
}
