//! Whole-client tests of `crate::net::wrath_bootstrap_capture`, from `net/wrath_bootstrap_capture.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_bootstrap_capture::*;
use benilla_protocol::wrath_bootstrap::Packet;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use benilla_protocol::SessionEvent;
use benilla_protocol::SessionEventKind;
use std::collections::VecDeque;

#[test]
fn one_owner_keeps_the_bootstrap_published_after_connected() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathBootstrap];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_bootstrap_capture::receive"));
    let connected = || SessionEvent::Connected {
        self_guid: 1,
        name: "test".into(),
        billing_time_rested: 0,
        tutorial_flags: None,
        addon_info: Some(vec![]),
    };
    crate::net::handlers::dispatch(app.world_mut(), vec![connected()]);
    let mut adapter = WrathEventAdapter::new();
    for (opcode, body) in [(0x02ef, 0u32.to_le_bytes()), (0x04ab, 3u32.to_le_bytes())] {
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &body, 0).unwrap()
        else {
            panic!("{opcode:#x}");
        };
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    assert_eq!(
        app.world().resource::<NativeBootstrapPackets>().0,
        VecDeque::from([
            Packet::AddonInfo { banned: vec![] },
            Packet::ClientCacheVersion { version: 3 },
        ])
    );
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app
        .world()
        .resource::<NativeBootstrapPackets>()
        .0
        .is_empty());
}
