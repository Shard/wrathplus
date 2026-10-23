//! Sole native WHO/WHOIS capture FIFO. No UI, shared projection, or automatic queries.
//! WHOIS may contain private account data; never log captured packets.
use crate::net::NetHandlerApp;
use wrathplus_protocol::{wrath_who::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub struct NativeWhoPackets(pub VecDeque<Packet>);
pub struct UiNativeWhoPlugin;
impl Plugin for UiNativeWhoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativeWhoPackets>()
            .net_handler(SessionEventKind::WrathWho, receive)
            .net_handler(SessionEventKind::Disconnected, clear);
    }
}
fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeWhoPackets>) {
    if let SessionEvent::WrathWho { packet } = event {
        packets.0.push_back(packet);
    }
}
fn clear(In(_): In<SessionEvent>, mut packets: ResMut<NativeWhoPackets>) {
    packets.0.clear();
}
#[cfg(test)]
#[path = "wrath_who_capture_tests.rs"]
mod tests;
