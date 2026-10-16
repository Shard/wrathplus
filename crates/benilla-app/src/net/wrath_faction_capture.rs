//! Transport-only ordered native forced faction reactions. App owns any future reducer/UI feed.
//! No reaction overrides are applied to unit colours or hostility.
use super::NetHandlerApp;
use benilla_protocol::{wrath_faction::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeFactionPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeFactionPackets>()
        .net_handler(SessionEventKind::WrathFaction, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeFactionPackets>) {
    if let SessionEvent::WrathFaction { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeFactionPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_faction_capture_tests.rs"]
mod tests;
