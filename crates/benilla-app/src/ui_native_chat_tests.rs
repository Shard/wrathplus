use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
fn body(gm: bool, text: &[u8]) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(u32::MAX.to_le_bytes());
    b.extend(42u64.to_le_bytes());
    b.extend(0x12345678u32.to_le_bytes());
    if gm {
        b.extend(2u32.to_le_bytes());
        b.extend([255, 0]);
    }
    b.extend(100u64.to_le_bytes());
    b.extend(((text.len() + 1) as u32).to_le_bytes());
    b.extend(text);
    b.extend([0, 0x83]);
    b
}
#[test]
fn adapter_fifo_preserves_raw_gm_order_duplicates_and_clears_on_disconnect() {
    let mut app = App::new();
    app.add_plugins(UiNativeChatPlugin);
    let mut adapter = WrathEventAdapter::new();
    let mut expected = Vec::new();
    for (gm, text) in [
        (false, b"\xff".as_slice()),
        (true, b"".as_slice()),
        (false, b"\xff".as_slice()),
    ] {
        let opcode = if gm { 0x3b3 } else { 0x96 };
        let bytes = body(gm, text);
        expected.push(benilla_protocol::wrath_chat::decode(opcode, &bytes).unwrap());
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &bytes, 0).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::WrathChat { .. }]
        ));
        crate::net::handlers::dispatch(app.world_mut(), session_events);
        for n in 0..bytes.len() {
            assert!(matches!(
                adapter.dispatch(opcode, &bytes[..n], 0).unwrap(),
                WrathDispatch::Unhandled { .. }
            ));
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(matches!(
            adapter.dispatch(opcode, &extra, 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
    assert_eq!(
        app.world()
            .resource::<NativeChatPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        expected
    );
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<NativeChatPackets>().0.is_empty());
}
#[test]
fn full_client_has_exactly_one_native_chat_owner_separate_from_shared() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathChat];
    assert_eq!(owners.len(), 1);
    assert!(owners[0].contains("ui_native_chat::receive"));
    assert!(!census[&SessionEventKind::Chat]
        .iter()
        .any(|owner| owner.contains("ui_native_chat")));
}
