use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
#[test]
fn adapter_fifo_is_lossless_repeated_ordered_and_disconnect_scoped() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let mut body = 99u64.to_le_bytes().to_vec();
    body.extend([1, 255]);
    for word in [7u32, 8, 9] {
        body.extend(word.to_le_bytes());
    }
    body.extend([255, 0, 254]);
    let list = benilla_protocol::wrath_stable::decode(0x26f, &body).unwrap();
    let mut expected = Vec::new();
    for (opcode, bytes) in [
        (0x26f, body.clone()),
        (0x273, vec![255]),
        (0x26f, body.clone()),
        (0x273, vec![0]),
    ] {
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &bytes, 0).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::WrathStable { .. }]
        ));
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    expected.extend([
        list.clone(),
        Packet::Result { result: 255 },
        list,
        Packet::Result { result: 0 },
    ]);
    assert_eq!(
        app.world()
            .resource::<NativeStablePackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        expected
    );
    for end in 0..body.len() {
        assert!(matches!(
            adapter.dispatch(0x26f, &body[..end], 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
    body.push(0);
    assert!(matches!(
        adapter.dispatch(0x26f, &body, 0).unwrap(),
        WrathDispatch::Unhandled { .. }
    ));
    for bad in [vec![], vec![0, 0]] {
        assert!(matches!(
            adapter.dispatch(0x273, &bad, 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<NativeStablePackets>().0.is_empty());
}
