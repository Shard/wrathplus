//! Transport-only ordered native item/inventory receipts. App owns any future reducer/UI feed.
//! No equipment-set, refund, socket, read or item-text state is inferred.
use super::NetHandlerApp;
use benilla_protocol::{wrath_inventory::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeInventoryPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeInventoryPackets>()
        .net_handler(SessionEventKind::WrathInventory, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeInventoryPackets>) {
    if let SessionEvent::WrathInventory { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeInventoryPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_inventory_capture_tests.rs"]
mod tests;
