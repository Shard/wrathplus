use super::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
#[test]
fn native_fifo_preserves_order_duplicates_and_reset_without_shared_projection() {
    let mut app = App::new();
    app.add_plugins(UiNativeWhoPlugin);
    let mut adapter = WrathEventAdapter::new();
    let mut expected = Vec::new();
    // Entirely synthetic WHOIS fixture, never a real account reply.
    for (opcode, body) in [
        (0x63, vec![0; 8]),
        (0x65, b"synthetic-only\0".to_vec()),
        (0x63, vec![0; 8]),
    ] {
        expected.push(wrathplus_protocol::wrath_who::decode(opcode, &body).unwrap());
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &body, 0).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::WrathWho { .. }]
        ));
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    assert_eq!(
        app.world()
            .resource::<NativeWhoPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        expected
    );
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "synthetic".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<NativeWhoPackets>().0.is_empty());
}
#[test]
fn malformed_packets_are_unhandled_without_leaking_body() {
    let mut adapter = WrathEventAdapter::new();
    for opcode in [0x63, 0x65] {
        assert!(matches!(
            adapter.dispatch(opcode, &[], 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
}
