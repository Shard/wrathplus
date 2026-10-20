//! Ordered native refer-a-friend receipts: level-grant proposals (who offers) and failures. App
//! owns any future reducer/UI feed.
use super::NetHandlerApp;
use benilla_protocol::{wrath_refer_friend::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeReferFriendPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeReferFriendPackets>()
        .net_handler(SessionEventKind::WrathReferFriend, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeReferFriendPackets>) {
    if let SessionEvent::WrathReferFriend { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeReferFriendPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
