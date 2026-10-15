//! Transport-only ordered native mail receipts. App owns any future reducer/UI feed.
//! Do not narrow these into the legacy mail events or mutate mailbox state here.
use super::NetHandlerApp;
use benilla_protocol::{wrath_mail_mutations::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeMailMutationPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeMailMutationPackets>()
        .net_handler(SessionEventKind::WrathMailMutation, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeMailMutationPackets>) {
    if let SessionEvent::WrathMailMutation { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeMailMutationPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_mail_capture_tests.rs"]
mod tests;
