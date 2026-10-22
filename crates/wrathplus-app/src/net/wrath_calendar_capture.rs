//! Transport-only ordered native calendar receipts. App owns any future reducer/UI feed.
//! No calendar, invite, lockout, holiday or pending state is inferred.
use super::NetHandlerApp;
use wrathplus_protocol::{wrath_calendar::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeCalendarPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeCalendarPackets>()
        .net_handler(SessionEventKind::WrathCalendar, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeCalendarPackets>) {
    if let SessionEvent::WrathCalendar { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeCalendarPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
