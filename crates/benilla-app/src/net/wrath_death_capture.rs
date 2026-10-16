//! Transport-only ordered native death and corpse receipts. App owns any future reducer/UI feed.
//! No release location, resurrection or corpse state is inferred.
use super::NetHandlerApp;
use benilla_protocol::{wrath_death::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeDeathPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeDeathPackets>()
        .net_handler(SessionEventKind::WrathDeath, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeDeathPackets>) {
    if let SessionEvent::WrathDeath { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeDeathPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_death_capture_tests.rs"]
mod tests;
