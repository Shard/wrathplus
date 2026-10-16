//! Transport-only ordered native character-screen service replies; App owns the result feed.
//! A roster change rides `CharacterList`, published before the reply.
use super::NetHandlerApp;
use benilla_protocol::{wrath_char_services::Reply, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeCharServiceReplies(pub(crate) VecDeque<Reply>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeCharServiceReplies>()
        .net_handler(SessionEventKind::WrathCharServiceResult, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeCharServiceReplies>) {
    if let SessionEvent::WrathCharServiceResult { reply } = event {
        packets.0.push_back(reply);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeCharServiceReplies>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_char_service_capture_tests.rs"]
mod tests;
