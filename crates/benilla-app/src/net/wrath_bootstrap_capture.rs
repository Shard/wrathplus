//! Build 12340's auth bootstrap packets, kept in arrival order: `SMSG_ADDON_INFO`'s banned list
//! (whose MD5 match against the loaded addons is not built) and `SMSG_CLIENTCACHE_VERSION` (no
//! persistent `WDB` cache here to invalidate). Which addons the reply marked secure already reaches
//! `SessionEvent::Connected`'s `addon_info`.
use super::NetHandlerApp;
use benilla_protocol::{wrath_bootstrap::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeBootstrapPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeBootstrapPackets>()
        .net_handler(SessionEventKind::WrathBootstrap, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeBootstrapPackets>) {
    if let SessionEvent::WrathBootstrap { packet } = event {
        packets.0.push_back(packet);
    }
}

/// A session boundary clears; the bootstrap is published right after `Connected`.
fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeBootstrapPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
