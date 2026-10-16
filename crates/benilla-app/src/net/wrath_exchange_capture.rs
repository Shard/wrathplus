//! Transport-only ordered native auction receipts without a shared event. App owns any future reducer/UI feed.
//! No pending-sales state or UI feed is inferred.
use super::NetHandlerApp;
use benilla_protocol::{wrath_exchange::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeExchangePackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeExchangePackets>()
        .net_handler(SessionEventKind::WrathExchange, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeExchangePackets>) {
    if let SessionEvent::WrathExchange { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeExchangePackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_exchange_capture_tests.rs"]
mod tests;
