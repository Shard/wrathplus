use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x329, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn ordered_replies_keep_zero_duplicates_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let body = [255; 12];
    dispatch(&mut app, &mut adapter, &body);
    dispatch(&mut app, &mut adapter, &[0; 12]);
    dispatch(&mut app, &mut adapter, &body);
    for end in 0..body.len() {
        dispatch(&mut app, &mut adapter, &body[..end]);
    }
    let mut trailing = body.to_vec();
    trailing.push(0);
    dispatch(&mut app, &mut adapter, &trailing);
    assert_eq!(
        app.world()
            .resource::<NativeInstanceControlPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            benilla_protocol::wrath_instance_control::decode(0x329, &[255; 12]).unwrap(),
            benilla_protocol::wrath_instance_control::decode(0x329, &[0; 12]).unwrap(),
            benilla_protocol::wrath_instance_control::decode(0x329, &[255; 12]).unwrap(),
        ]
    );
}

#[test]
fn one_owner_and_both_session_boundaries_clear_captured_replies() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathInstanceControl];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_instance_control_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    for event in [
        SessionEvent::Connected {
            self_guid: 1,
            name: "test".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        },
        SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, &[0; 12]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(app
            .world()
            .resource::<NativeInstanceControlPackets>()
            .0
            .is_empty());
    }
}

#[test]
fn every_family_opcode_dispatches_one_raw_packet_and_rejects_bad_extent() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let rows = [
        (0x147, 9),
        (0x214, 4),
        (0x2cb, 4),
        (0x2fa, 16),
        (0x31e, 4),
        (0x31f, 8),
        (0x320, 4),
        (0x329, 12),
        (0x32b, 4),
        (0x33b, 8),
        (0x396, 4),
        (0x4eb, 12),
        (0x506, 0),
    ];
    let mut expected = Vec::new();
    for (opcode, size) in rows {
        let mut body = vec![0; size];
        if opcode == 0x214 {
            body = vec![7, 0, 0, 0];
        }
        expected.push(benilla_protocol::wrath_instance_control::decode(opcode, &body).unwrap());
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &body, 0).unwrap()
        else {
            panic!("{opcode:x}")
        };
        assert_eq!(session_events.len(), 1);
        crate::net::handlers::dispatch(app.world_mut(), session_events);
        body.push(0);
        assert!(matches!(
            adapter.dispatch(opcode, &body, 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
    assert_eq!(
        app.world()
            .resource::<NativeInstanceControlPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        expected
    );
}
