use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

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
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<AppearanceReceipts>().0.is_empty());
}

#[test]
fn built_client_has_one_owner() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathAppearance];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_appearance::receive"));
}

#[test]
fn the_barber_window_closes_when_the_player_leaves_the_chair() {
    let mut chair = BarberChair::default();
    assert!(!chair_step(&mut chair, Some(0)), "nothing open");
    chair.open = true;
    assert!(
        !chair_step(&mut chair, Some(0)),
        "not seated yet: the chair's stand state is late"
    );
    assert!(!chair_step(&mut chair, Some(5)));
    assert!(!chair_step(&mut chair, Some(5)));
    assert!(chair_step(&mut chair, Some(0)), "stood up");
    assert!(!chair.open && !chair_step(&mut chair, Some(0)));
}
