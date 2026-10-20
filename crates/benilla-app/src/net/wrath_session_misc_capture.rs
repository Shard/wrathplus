//! Ordered native session receipts without a shared event: the daily-reset countdown, play-time
//! warnings, inebriation and the feature-system status. App owns any future reducer/UI feed.
use super::NetHandlerApp;
use benilla_protocol::{wrath_session_misc::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeSessionMiscPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeSessionMiscPackets>()
        .net_handler(SessionEventKind::WrathSessionMisc, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeSessionMiscPackets>) {
    if let SessionEvent::WrathSessionMisc { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeSessionMiscPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
