//! Whole-client tests of `crate::ui_party::wrath`, from `ui_party/wrath.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::ui_party::GroupState;
use benilla_protocol::wrath_group::{self as g};

#[test]
fn real_dispatch_has_registered_owner_and_lifecycle() {
    use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
    use benilla_protocol::wrath_world::WrathServerPacket;
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let mut adapter = WrathEventAdapter::new();
    let mut body = vec![0, 0, 0, 0];
    body.extend(42u64.to_le_bytes());
    body.extend(1u32.to_le_bytes());
    body.extend(1u32.to_le_bytes());
    body.extend(b"Ally\0");
    body.extend(7u64.to_le_bytes());
    body.extend([1, 0, 0, 4]);
    body.extend(7u64.to_le_bytes());
    body.push(0);
    body.extend(0u64.to_le_bytes());
    body.extend([2, 0, 0, 0]);
    for (opcode, body) in [
        (g::SMSG_GROUP_LIST, body),
        (
            g::SMSG_PARTY_MEMBER_STATS,
            vec![1, 7, 2, 0, 0, 0, 0x20, 0xa1, 7, 0],
        ),
    ] {
        let WrathDispatch::Handled { session_events, .. } = adapter
            .dispatch_packet(&WrathServerPacket { opcode, body }, 1)
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(session_events.len(), 1);
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    assert_eq!(
        app.world().resource::<GroupState>().wrath_stats[&7].cur_hp,
        Some(500000)
    );
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![benilla_protocol::SessionEvent::WrathGroup {
            packet: Box::new(g::Packet::Uninvite),
        }],
    );
    let state = app.world().resource::<GroupState>();
    assert!(!state.in_group);
    assert!(state.wrath_stats.is_empty());
}
