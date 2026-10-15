//! Sole lossless guild-bank capture owner. App Migration owns future state/feeds/drains;
//! this FIFO does not infer permissions, emit UI events, or initiate queries.
use crate::net::NetHandlerApp;
use benilla_protocol::{wrath_guild_bank::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeGuildBankPackets(pub(crate) VecDeque<Packet>);

pub(crate) struct UiGuildBankPlugin;
impl Plugin for UiGuildBankPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativeGuildBankPackets>()
            .net_handler(SessionEventKind::WrathGuildBank, receive)
            .net_handler(SessionEventKind::Disconnected, clear);
    }
}
fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeGuildBankPackets>) {
    if let SessionEvent::WrathGuildBank { packet } = event {
        packets.0.push_back(packet);
    }
}
fn clear(In(_): In<SessionEvent>, mut packets: ResMut<NativeGuildBankPackets>) {
    packets.0.clear();
}

#[cfg(test)]
#[path = "ui_guild_bank_tests.rs"]
mod tests;
