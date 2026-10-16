//! Transport-only ordered native achievement receipts. App owns any future reducer/UI feed.
//! No achievement, criteria, points or toast state is inferred.
use super::NetHandlerApp;
use benilla_protocol::{wrath_achievement::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeAchievementPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeAchievementPackets>()
        .net_handler(SessionEventKind::WrathAchievement, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeAchievementPackets>) {
    if let SessionEvent::WrathAchievement { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeAchievementPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_achievement_capture_tests.rs"]
mod tests;
