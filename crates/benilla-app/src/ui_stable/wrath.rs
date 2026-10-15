//! Sole lossless stable capture owner. App Migration owns future state/feeds/drains;
//! this FIFO does not infer permissions, emit UI events, or initiate queries.
use crate::net::NetHandlerApp;
use benilla_protocol::{wrath_stable::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeStablePackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeStablePackets>()
        .net_handler(SessionEventKind::WrathStable, receive)
        .net_handler(SessionEventKind::Disconnected, clear);
}
fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeStablePackets>) {
    if let SessionEvent::WrathStable { packet } = event {
        packets.0.push_back(packet);
    }
}
fn clear(In(_): In<SessionEvent>, mut packets: ResMut<NativeStablePackets>) {
    packets.0.clear();
}

#[cfg(test)]
#[path = "wrath_tests.rs"]
mod tests;
