//! Transport-only ordered native instance-control packets. App owns any future reducer/UI feed.
//! No inferred instance state, UI feed or timers.
use super::NetHandlerApp;
use bevy::prelude::*;
use std::collections::VecDeque;
use wrathplus_protocol::{wrath_instance_control::Packet, SessionEvent, SessionEventKind};

#[derive(Resource, Default)]
pub struct NativeInstanceControlPackets(pub VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeInstanceControlPackets>()
        .net_handler(SessionEventKind::WrathInstanceControl, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeInstanceControlPackets>) {
    if let SessionEvent::WrathInstanceControl { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeInstanceControlPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_instance_control_capture_tests.rs"]
pub mod tests;
