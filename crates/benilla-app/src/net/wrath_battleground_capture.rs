//! Transport-only ordered native battleground, arena and battlefield receipts. App owns any future reducer/UI feed.
//! No queue, score, rating or battlefield state is inferred.
use super::NetHandlerApp;
use benilla_protocol::{wrath_battleground::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeBattlegroundPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeBattlegroundPackets>()
        .net_handler(SessionEventKind::WrathBattleground, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeBattlegroundPackets>) {
    if let SessionEvent::WrathBattleground { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeBattlegroundPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_battleground_capture_tests.rs"]
mod tests;
