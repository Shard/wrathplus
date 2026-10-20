//! Whole-client tests of `crate::net::wrath_exchange_capture`, from `net/wrath_exchange_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_exchange_capture::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use benilla_protocol::wrath_exchange::Packet;
use benilla_protocol::SessionEvent;
use benilla_protocol::SessionEventKind;
use bevy::prelude::*;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeExchangePackets>()
        .0
        .iter()
        .cloned()
        .collect()
}

#[test]
fn one_owner_keeps_ordered_duplicates_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathExchange];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_exchange_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    let empty = Packet::AuctionPendingSales { rows: vec![] };
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
        dispatch(&mut app, &mut adapter, 0x490, &[0; 4]);
        dispatch(&mut app, &mut adapter, 0x490, &[0; 3]);
        dispatch(&mut app, &mut adapter, 0x490, &[0; 5]);
        dispatch(&mut app, &mut adapter, 0x490, &[0; 4]);
        assert_eq!(captured(&app), vec![empty.clone(), empty.clone()]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}

/// Trade, mail results, auction replies and the master-loot list reach the existing feeds.
#[test]
fn shared_replies_reach_their_existing_owners_not_a_capture() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    for kind in [
        SessionEventKind::TradeStatus,
        SessionEventKind::TradeStatusExtended,
        SessionEventKind::SendMailResult,
        SessionEventKind::AuctionHello,
        SessionEventKind::AuctionCommandResult,
        SessionEventKind::AuctionListResult,
        SessionEventKind::AuctionOwnerListResult,
        SessionEventKind::AuctionBidderListResult,
        SessionEventKind::AuctionBidderNotification,
        SessionEventKind::AuctionOwnerNotification,
        SessionEventKind::LootMasterList,
    ] {
        assert!(
            census.get(&kind).is_some_and(|o| !o.is_empty()),
            "{kind:?} has no owner"
        );
    }
}
