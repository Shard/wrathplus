//! Transport-only ordered native mirror-image replies. App owns any future reducer/UI feed.
//! No rendering, UI feed or inferred appearance state.
use super::NetHandlerApp;
use benilla_protocol::{wrath_mirror_image::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeMirrorImagePackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeMirrorImagePackets>()
        .net_handler(SessionEventKind::WrathMirrorImage, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeMirrorImagePackets>) {
    if let SessionEvent::WrathMirrorImage { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeMirrorImagePackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_mirror_image_capture_tests.rs"]
mod tests;
