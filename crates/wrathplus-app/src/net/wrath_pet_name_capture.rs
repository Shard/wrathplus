//! Sole lossless native pet-name capture owner. App Migration owns future state/feeds/drains;
//! this FIFO does not infer permissions, emit UI events, or initiate queries.
use crate::net::NetHandlerApp;
use wrathplus_protocol::{wrath_pet_name::Response, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativePetNamePackets(pub(crate) VecDeque<Response>);

pub(crate) struct UiNativePetNamePlugin;
impl Plugin for UiNativePetNamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativePetNamePackets>()
            .net_handler(SessionEventKind::WrathPetName, receive)
            .net_handler(SessionEventKind::Disconnected, clear);
    }
}
fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativePetNamePackets>) {
    if let SessionEvent::WrathPetName { response } = event {
        packets.0.push_back(response);
    }
}
fn clear(In(_): In<SessionEvent>, mut packets: ResMut<NativePetNamePackets>) {
    packets.0.clear();
}

#[cfg(test)]
#[path = "wrath_pet_name_capture_tests.rs"]
mod tests;
