//! Ordered native Help Request receipts without a shared event: the GM response, the survey offer
//! and the complaint receipt. App owns any future reducer/UI feed.
use super::NetHandlerApp;
use benilla_protocol::{wrath_help::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeHelpPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeHelpPackets>()
        .net_handler(SessionEventKind::WrathHelp, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeHelpPackets>) {
    if let SessionEvent::WrathHelp { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeHelpPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_help_capture_tests.rs"]
mod tests;
