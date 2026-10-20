//! Ordered native achievement receipts. The player's own progress and the asked comparison feed
//! `wrath_achievements`; other players' earns and server firsts stay here.
use super::NetHandlerApp;
use benilla_protocol::{wrath_achievement::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeAchievementPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    super::wrath_achievements::register(app);
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

fn clear(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeAchievementPackets>,
    mut achievements: ResMut<super::wrath_achievements::Achievements>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
        super::wrath_achievements::clear(&mut achievements);
    }
}
