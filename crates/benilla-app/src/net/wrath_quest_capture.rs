//! Transport-only ordered native quest POI and PvP-kill receipts. App owns any future reducer/UI feed.
//! No map POIs or objective counters are inferred.
use super::NetHandlerApp;
use benilla_protocol::{wrath_quest::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeQuestPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeQuestPackets>()
        .net_handler(SessionEventKind::WrathQuest, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeQuestPackets>) {
    if let SessionEvent::WrathQuest { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeQuestPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_quest_capture_tests.rs"]
mod tests;
