use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
#[test]
fn adapter_fifo_is_lossless_repeated_ordered_and_disconnect_scoped() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let body = b"inviter\0guild\0".to_vec();
    let list = benilla_protocol::wrath_guild::decode(0x83, &body).unwrap();
    let mut expected = Vec::new();
    for (opcode, bytes) in [
        (0x83, body.clone()),
        (0x92, vec![2, 1, 255, 0]),
        (0x83, body.clone()),
        (0x92, vec![255, 0]),
    ] {
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &bytes, 0).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::WrathGuild { .. }]
        ));
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    expected.extend([
        list.clone(),
        Packet::Event {
            event: 2,
            params: vec![vec![255]],
            guid: None,
        },
        list,
        Packet::Event {
            event: 255,
            params: vec![],
            guid: None,
        },
    ]);
    assert_eq!(
        app.world()
            .resource::<NativeGuildPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        expected
    );
    for end in 0..body.len() {
        assert!(matches!(
            adapter.dispatch(0x83, &body[..end], 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
    let mut body = body;
    body.push(0);
    assert!(matches!(
        adapter.dispatch(0x83, &body, 0).unwrap(),
        WrathDispatch::Unhandled { .. }
    ));
    for bad in [vec![], vec![3, 0]] {
        assert!(matches!(
            adapter.dispatch(0x92, &bad, 0).unwrap(),
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
    assert!(app.world().resource::<NativeGuildPackets>().0.is_empty());
}
#[test]
fn built_client_has_one_native_owner_separate_from_vanilla() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathGuild];
    assert_eq!(owners.len(), 1);
    assert!(owners[0].contains("ui_guild::wrath::receive"));
    assert!(census[&SessionEventKind::GuildRoster][0].contains("ui_guild::net::on_roster"));
}
