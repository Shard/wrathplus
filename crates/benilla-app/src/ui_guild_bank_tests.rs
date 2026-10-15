use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn adapter_fifo_retains_repeats_interleaving_and_resets_on_disconnect() {
    let mut app = App::new();
    app.add_plugins(UiGuildBankPlugin);
    let mut adapter = WrathEventAdapter::new();
    let mut body = vec![255; 61];
    body[12] = 0; // Six rows remain even when no tabs are purchased.
    let permission = benilla_protocol::wrath_guild_bank::decode(0x3fd, &body).unwrap();
    let expected = vec![
        Packet::RemainingMoney { remaining: -1 },
        permission.clone(),
        Packet::RemainingMoney {
            remaining: i32::MIN,
        },
        permission,
    ];
    dispatch(&mut app, &mut adapter, 0x3fe, &[255; 4]);
    dispatch(&mut app, &mut adapter, 0x3fd, &body);
    dispatch(&mut app, &mut adapter, 0x3fe, &[0, 0, 0, 128]);
    dispatch(&mut app, &mut adapter, 0x3fd, &body);
    assert_eq!(
        app.world()
            .resource::<NativeGuildBankPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        expected
    );
    for end in 0..body.len() {
        dispatch(&mut app, &mut adapter, 0x3fd, &body[..end]);
    }
    body.push(0);
    dispatch(&mut app, &mut adapter, 0x3fd, &body);
    for end in 0..4 {
        dispatch(&mut app, &mut adapter, 0x3fe, &vec![0; end]);
    }
    dispatch(&mut app, &mut adapter, 0x3fe, &[0; 5]);
    assert_eq!(app.world().resource::<NativeGuildBankPackets>().0.len(), 4);
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app
        .world()
        .resource::<NativeGuildBankPackets>()
        .0
        .is_empty());
    dispatch(&mut app, &mut adapter, 0x3fe, &[0; 4]);
    assert_eq!(
        app.world().resource::<NativeGuildBankPackets>().0.front(),
        Some(&Packet::RemainingMoney { remaining: 0 })
    );
}

#[test]
fn built_client_registers_exactly_one_guild_bank_owner() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathGuildBank];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("ui_guild_bank::receive"));
}
