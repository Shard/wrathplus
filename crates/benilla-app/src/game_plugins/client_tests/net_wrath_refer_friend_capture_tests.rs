//! Whole-client tests of `crate::net::wrath_refer_friend_capture`, from `net/wrath_refer_friend_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_refer_friend_capture::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use benilla_protocol::wrath_refer_friend::Packet;
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

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeReferFriendPackets>()
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
    let owners = &census[&SessionEventKind::WrathReferFriend];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_refer_friend_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    let offer = Packet::ProposeLevelGrant { granter: 5 };
    let failed = Packet::Failure {
        error: 2,
        name: None,
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
            end: benilla_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, 0x41f, &[1, 5]);
        dispatch(&mut app, &mut adapter, 0x41f, &[1, 5, 0]);
        dispatch(&mut app, &mut adapter, 0x421, &[2, 0, 0, 0]);
        dispatch(&mut app, &mut adapter, 0x41f, &[1, 5]);
        assert_eq!(
            captured(&app),
            vec![offer.clone(), failed.clone(), offer.clone()]
        );
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}
