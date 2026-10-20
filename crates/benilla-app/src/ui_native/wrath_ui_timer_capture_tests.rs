use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

pub(crate) fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x4f7, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn ordered_replies_keep_zero_duplicates_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let body = [255; 4];
    dispatch(&mut app, &mut adapter, &body);
    dispatch(&mut app, &mut adapter, &[0; 4]);
    dispatch(&mut app, &mut adapter, &body);
    for end in 0..body.len() {
        dispatch(&mut app, &mut adapter, &body[..end]);
    }
    let mut trailing = body.to_vec();
    trailing.push(0);
    dispatch(&mut app, &mut adapter, &trailing);
    assert_eq!(
        app.world()
            .resource::<NativeUiTimerPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            Packet { time: u32::MAX },
            Packet { time: 0 },
            Packet { time: u32::MAX },
        ]
    );
}

fn connected() -> SessionEvent {
    SessionEvent::Connected {
        self_guid: 1,
        name: "Me".into(),
        billing_time_rested: 0,
        tutorial_flags: None,
        addon_info: None,
    }
}

/// `0x5488f0`: a query at once on entering the world, then every 20 s; each reply sets the
/// offset the VM's `server_now` reads; leaving the world stops the ticker and forgets it.
#[test]
fn the_ticker_queries_every_twenty_seconds_and_replies_set_the_offset() {
    use benilla_ui::script::{ScriptDialect, UiScript};
    use std::time::Duration;
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.add_plugins(bevy::state::app::StatesPlugin)
        .insert_state(crate::char_select::ClientState::InWorld)
        .insert_resource(NetCommands(tx))
        .init_resource::<Time>()
        .insert_non_send_resource(UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap());
    register(&mut app);
    let advance = |app: &mut App, secs: f64| {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f64(secs));
        app.update();
    };
    let sent = |rx: &crossbeam_channel::Receiver<ClientCommand>| {
        rx.try_iter()
            .filter(|c| matches!(c, ClientCommand::NativeUiTimerQuery))
            .count()
    };
    advance(&mut app, 1.0);
    assert_eq!(sent(&rx), 0, "no ticker outside the world");

    crate::net::handlers::dispatch(app.world_mut(), vec![connected()]);
    advance(&mut app, 0.0);
    assert_eq!(sent(&rx), 1, "the first query at once");
    advance(&mut app, 19.0);
    assert_eq!(sent(&rx), 0);
    advance(&mut app, 1.0);
    assert_eq!(sent(&rx), 1, "then every 20 s");

    let server = (local_time() + 3600) as u32;
    let mut adapter = WrathEventAdapter::new();
    dispatch(&mut app, &mut adapter, &server.to_le_bytes());
    advance(&mut app, 0.0);
    let offset = app.world().resource::<ServerClock>().offset.unwrap();
    assert!((offset - 3600).abs() <= 1, "{offset}");
    let now = app
        .world()
        .non_send_resource::<UiScript>()
        .wrath_server_now()
        .unwrap();
    assert!((now - server as i64).abs() <= 1);

    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: String::new(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    advance(&mut app, 40.0);
    assert_eq!(sent(&rx), 0, "the ticker stops with the world");
    assert_eq!(
        app.world()
            .non_send_resource::<UiScript>()
            .wrath_server_now(),
        None
    );
}
