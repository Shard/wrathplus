//! Ordered native character-screen service replies (rename, customize, faction and race change,
//! declined names, realm split); `char_select::delete_result` drains them into the glue dialog.
//! A roster change rides `CharacterList`, published before the reply.
use super::NetHandlerApp;
use wrathplus_protocol::{wrath_char_services::Reply, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub struct NativeCharServiceReplies(pub VecDeque<Reply>);

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
