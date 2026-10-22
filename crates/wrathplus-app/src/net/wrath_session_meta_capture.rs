//! Transport-only ordered native account-data receipts. App owns any future reducer/UI feed.
//! No account data is decompressed or applied.
use super::NetHandlerApp;
use wrathplus_protocol::{wrath_session_meta::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeSessionMetaPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeSessionMetaPackets>()
        .net_handler(SessionEventKind::WrathSessionMeta, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeSessionMetaPackets>) {
    if let SessionEvent::WrathSessionMeta { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeSessionMetaPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
