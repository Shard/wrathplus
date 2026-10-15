use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
#[test]
fn adapter_fifo_is_lossless_repeated_ordered_and_disconnect_scoped() {
    let mut app = App::new();
    app.add_plugins(UiArenaTeamPlugin);
    let mut adapter = WrathEventAdapter::new();
    let body = b"inviter\0guild\0".to_vec();
    let list = benilla_protocol::wrath_arena_team::decode(0x350, &body).unwrap();
    let mut expected = Vec::new();
    for (opcode, bytes) in [
        (0x350, body.clone()),
        (0x350, vec![255, 0, 254, 0]),
        (0x350, body.clone()),
        (0x350, vec![0, 0]),
    ] {
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &bytes, 0).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::WrathArenaTeam { .. }]
        ));
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    expected.extend([
        list.clone(),
        Packet::Invite {
            inviter: vec![255],
            team: vec![254],
        },
        list,
        Packet::Invite {
            inviter: vec![],
            team: vec![],
        },
    ]);
    assert_eq!(
        app.world()
            .resource::<NativeArenaTeamPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        expected
    );
    for end in 0..body.len() {
        assert!(matches!(
            adapter.dispatch(0x350, &body[..end], 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
    let mut body = body;
    body.push(0);
    assert!(matches!(
        adapter.dispatch(0x350, &body, 0).unwrap(),
        WrathDispatch::Unhandled { .. }
    ));
    for bad in [vec![], vec![3, 0]] {
        assert!(matches!(
            adapter.dispatch(0x350, &bad, 0).unwrap(),
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
    assert!(app
        .world()
        .resource::<NativeArenaTeamPackets>()
        .0
        .is_empty());
}
#[test]
fn built_client_has_one_native_owner_separate_from_vanilla() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathArenaTeam];
    assert_eq!(owners.len(), 1);
    assert!(owners[0].contains("ui_arena_team::receive"));
}
