//! Transport-only ordered native auction receipts without a shared event: 0490 pending sales.
//! No pending-sales state or UI feed is inferred, by design: build 12340 has no Lua reader for
//! auction pending sales (no such API string in the pinned Wow.exe), and AzerothCore always sends
//! zero rows.
use super::NetHandlerApp;
use wrathplus_protocol::{wrath_exchange::Packet, SessionEvent, SessionEventKind};
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
