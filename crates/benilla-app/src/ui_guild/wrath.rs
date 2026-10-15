//! Sole lossless guild capture owner. App Migration owns future state/feeds/drains;
//! this FIFO does not infer permissions, emit UI events, or initiate queries.
use crate::net::NetHandlerApp;
use benilla_protocol::{wrath_guild::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeGuildPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeGuildPackets>()
        .net_handler(SessionEventKind::WrathGuild, receive)
        .net_handler(SessionEventKind::Disconnected, clear);
}
fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeGuildPackets>) {
    if let SessionEvent::WrathGuild { packet } = event {
        packets.0.push_back(packet);
    }
}
fn clear(In(_): In<SessionEvent>, mut packets: ResMut<NativeGuildPackets>) {
    packets.0.clear();
}

#[cfg(test)]
#[path = "wrath_tests.rs"]
mod tests;
