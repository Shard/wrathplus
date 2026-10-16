//! Transport-only ordered native UI timer replies. App owns any future reducer/UI feed.
//! No clock synchronization, UI feed or inferred timer state.
use super::NetHandlerApp;
use benilla_protocol::{wrath_ui_timer::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeUiTimerPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeUiTimerPackets>()
        .net_handler(SessionEventKind::WrathUiTimer, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeUiTimerPackets>) {
    if let SessionEvent::WrathUiTimer { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeUiTimerPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_ui_timer_capture_tests.rs"]
mod tests;
