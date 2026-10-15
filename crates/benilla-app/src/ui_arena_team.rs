//! Sole lossless arena-team capture owner. App Migration owns future state/feeds/drains;
//! this FIFO does not infer permissions, emit UI events, or initiate queries.
use crate::net::NetHandlerApp;
use benilla_protocol::{wrath_arena_team::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeArenaTeamPackets(pub(crate) VecDeque<Packet>);

pub(crate) struct UiArenaTeamPlugin;
impl Plugin for UiArenaTeamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativeArenaTeamPackets>()
            .net_handler(SessionEventKind::WrathArenaTeam, receive)
            .net_handler(SessionEventKind::Disconnected, clear);
    }
}
fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeArenaTeamPackets>) {
    if let SessionEvent::WrathArenaTeam { packet } = event {
        packets.0.push_back(packet);
    }
}
fn clear(In(_): In<SessionEvent>, mut packets: ResMut<NativeArenaTeamPackets>) {
    packets.0.clear();
}

#[cfg(test)]
#[path = "ui_arena_team_tests.rs"]
mod tests;
