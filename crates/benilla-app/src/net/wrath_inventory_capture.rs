//! Ordered native item/inventory receipts. Refund replies feed `wrath_item_refunds`,
//! equipment-set traffic `wrath_equipment_sets` and socket results `wrath_sockets`; read and
//! item-text receipts stay here.
use super::NetHandlerApp;
use benilla_protocol::{wrath_inventory::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeInventoryPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    super::wrath_item_refunds::register(app);
    super::wrath_equipment_sets::register(app);
    super::wrath_sockets::register(app);
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

fn clear(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeInventoryPackets>,
    mut refunds: ResMut<super::wrath_item_refunds::ItemRefunds>,
    mut sets: ResMut<super::wrath_equipment_sets::EquipmentSets>,
    mut sockets: ResMut<super::wrath_sockets::SocketSession>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
        super::wrath_item_refunds::clear(&mut refunds);
        super::wrath_equipment_sets::clear(&mut sets);
        super::wrath_sockets::clear(&mut sockets);
    }
}

#[cfg(test)]
#[path = "wrath_inventory_capture_tests.rs"]
mod tests;
