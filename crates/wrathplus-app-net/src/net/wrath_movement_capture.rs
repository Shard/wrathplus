//! Ordered native movement notices without a shared event (`SMSG_DISMOUNT`; the mount state itself
//! is the unit's descriptor). App owns any future reducer.
use super::NetHandlerApp;
use bevy::prelude::*;
use std::collections::VecDeque;
use wrathplus_protocol::{wrath_movement_misc::Packet, SessionEvent, SessionEventKind};

#[derive(Resource, Default)]
pub struct NativeMovementPackets(pub VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeMovementPackets>()
        .net_handler(SessionEventKind::WrathMovement, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeMovementPackets>) {
    if let SessionEvent::WrathMovement { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeMovementPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
