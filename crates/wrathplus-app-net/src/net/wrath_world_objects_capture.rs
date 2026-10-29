//! Ordered native world-presentation receipts without a shared event: game-object page text,
//! light overrides, phase shifts, movies, dance moves and building damage. App owns any future
//! reducer/UI feed.
use super::NetHandlerApp;
use bevy::prelude::*;
use std::collections::VecDeque;
use wrathplus_protocol::{wrath_world_objects::Packet, SessionEvent, SessionEventKind};

#[derive(Resource, Default)]
pub struct NativeWorldObjectsPackets(pub VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeWorldObjectsPackets>()
        .net_handler(SessionEventKind::WrathWorldObjects, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeWorldObjectsPackets>) {
    if let SessionEvent::WrathWorldObjects { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeWorldObjectsPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
