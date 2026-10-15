//! Transport-only ordered native completed-quest replies. App owns any future reducer/UI feed.
//! No quest-log mutation or inferred completion state.
use super::NetHandlerApp;
use benilla_protocol::{wrath_quests_completed::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeQuestsCompletedPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeQuestsCompletedPackets>()
        .net_handler(SessionEventKind::WrathQuestsCompleted, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeQuestsCompletedPackets>) {
    if let SessionEvent::WrathQuestsCompleted { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeQuestsCompletedPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_quests_completed_capture_tests.rs"]
mod tests;
