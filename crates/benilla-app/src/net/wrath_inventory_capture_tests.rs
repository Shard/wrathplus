use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use benilla_protocol::wrath_inventory::decode;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeInventoryPackets>()
        .0
        .iter()
        .cloned()
        .collect()
}

fn packet(opcode: u16, body: &[u8]) -> Packet {
    match decode(opcode, body).unwrap() {
        SessionEvent::WrathInventory { packet } => packet,
        other => panic!("{other:?}"),
    }
}

#[test]
fn ordered_receipts_keep_duplicates_across_opcodes_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let guid = 0x0400_0000_0000_0102u64.to_le_bytes();
    let rows: [(u16, &[u8]); 4] = [(0x4d6, &[4]), (0x0ae, &guid), (0x4d6, &[0]), (0x4d6, &[4])];
    for (opcode, body) in rows {
        dispatch(&mut app, &mut adapter, opcode, body);
        for end in 0..body.len() {
            dispatch(&mut app, &mut adapter, opcode, &body[..end]);
        }
        dispatch(&mut app, &mut adapter, opcode, &[body, &[0]].concat());
    }
    assert_eq!(
        captured(&app),
        rows.iter().map(|(o, b)| packet(*o, b)).collect::<Vec<_>>()
    );
}

#[test]
fn one_owner_and_both_session_boundaries_clear_captured_receipts() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathInventory];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_inventory_capture::receive"));
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
        dispatch(&mut app, &mut adapter, 0x4d6, &[4]);
        assert_eq!(captured(&app).len(), 1);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}

/// The shared-event opcodes reach the existing bank consumers, not the capture FIFO.
#[test]
fn bank_replies_feed_the_existing_bank_state() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let mut adapter = WrathEventAdapter::new();
    let banker = 0xf130_0000_0001_0002u64;
    dispatch(&mut app, &mut adapter, 0x1b8, &banker.to_le_bytes());
    dispatch(&mut app, &mut adapter, 0x1ba, &1u32.to_le_bytes());
    assert_eq!(
        app.world().resource::<crate::ui_bank::BankOpen>().banker,
        Some(banker)
    );
    assert_eq!(
        app.world().resource::<crate::ui_bank::BankErrors>().0,
        vec![1]
    );
    assert!(captured(&app).is_empty());
}
