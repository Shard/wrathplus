use super::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
fn body(flag: u8, name: &[u8]) -> Vec<u8> {
    let mut b = 42u32.to_le_bytes().to_vec();
    b.extend(name);
    b.push(0);
    b.extend(u32::MAX.to_le_bytes());
    b.push(flag);
    if flag != 0 {
        b.extend([0, 255, 0, 0, 0, 0]);
    }
    b
}
#[test]
fn adapter_fifo_preserves_raw_response_order_duplicates_and_clears_on_disconnect() {
    let mut app = App::new();
    app.add_plugins(UiNativePetNamePlugin);
    let mut adapter = WrathEventAdapter::new();
    let mut expected = Vec::new();
    for (flag, text) in [
        (0, b"\xff".as_slice()),
        (255, b"".as_slice()),
        (0, b"\xff".as_slice()),
    ] {
        let opcode = 0x53;
        let bytes = body(flag, text);
        expected.push(wrathplus_protocol::wrath_pet_name::decode(&bytes).unwrap());
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &bytes, 0).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::WrathPetName { .. }]
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
            .resource::<NativePetNamePackets>()
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
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<NativePetNamePackets>().0.is_empty());
}
