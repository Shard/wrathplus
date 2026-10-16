//! Transport-only ordered native show-mailbox notices. App owns any future reducer/UI feed.
//! `SMSG_SEND_MAIL_RESULT` means what 1.12's does and feeds the shared `SendMailResult` through
//! `wrath_exchange`; do not mutate mailbox state here.
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
