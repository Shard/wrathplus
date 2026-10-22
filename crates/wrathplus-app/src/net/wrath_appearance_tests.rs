use super::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

#[test]
fn appearance_replies_queue_in_order_and_session_boundaries_clear_them() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    for (opcode, body) in [
        (0x427u16, Vec::new()),
        (0x428, 0u32.to_le_bytes().to_vec()),
        (0x373, [3u32.to_le_bytes(), 1u32.to_le_bytes()].concat()),
    ] {
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &body, 0).unwrap()
        else {
            panic!("{opcode:#x}")
        };
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    assert_eq!(
        app.world().resource::<AppearanceReceipts>().0,
        [
            Packet::EnableBarberShop,
            Packet::BarberShopResult { result: 0 },
            Packet::TitleEarned {
                bit_index: 3,
                earned: true
            },
        ]
    );
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<AppearanceReceipts>().0.is_empty());
}
