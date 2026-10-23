//! Transport-only ordered native inspect replies. App owns any future reducer/UI feed.
//! No inspected talents, glyphs or gear are applied to any frame.
use super::NetHandlerApp;
use wrathplus_protocol::{wrath_inspect::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub struct NativeInspectPackets(pub VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeInspectPackets>()
        .net_handler(SessionEventKind::WrathInspect, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeInspectPackets>) {
    if let SessionEvent::WrathInspect { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeInspectPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
