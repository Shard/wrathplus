//! Whole-client tests of `crate::net::wrath_inventory_capture`, from `net/wrath_inventory_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_inventory_capture::tests::{captured, dispatch};
use benilla_protocol::wrath_events::WrathEventAdapter;
use benilla_protocol::SessionEvent;
use benilla_protocol::SessionEventKind;

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
