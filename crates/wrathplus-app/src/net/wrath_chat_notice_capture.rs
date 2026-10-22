//! Transport-only ordered native chat notices without a shared event. App owns any future reducer/UI feed.
//! No name resolution or UI line is inferred.
use super::NetHandlerApp;
use wrathplus_protocol::{wrath_chat_notices::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeChatNoticePackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeChatNoticePackets>()
        .net_handler(SessionEventKind::WrathChatNotice, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeChatNoticePackets>) {
    if let SessionEvent::WrathChatNotice { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeChatNoticePackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
