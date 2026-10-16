//! Ordered native spell-control receipts without a shared event: another unit's channel start
//! and update, spell impacts and created totems. App owns any future reducer/UI feed.
use super::NetHandlerApp;
use benilla_protocol::{wrath_spell_control::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeSpellControlPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeSpellControlPackets>()
        .net_handler(SessionEventKind::WrathSpellControl, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeSpellControlPackets>) {
    if let SessionEvent::WrathSpellControl { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeSpellControlPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_spell_control_capture_tests.rs"]
mod tests;
