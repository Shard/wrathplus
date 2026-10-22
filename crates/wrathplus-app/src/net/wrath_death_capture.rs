//! Ordered native death and corpse receipts. The release location (`SMSG_DEATH_RELEASE_LOC`) feeds
//! the world map's `GetDeathReleasePosition` (`crate::ui_native::wrath_death_capture`);
//! pre-resurrect and corpse-map-position receipts stay here (AC answers the latter with zeros).
use crate::net::NetHandlerApp;
use wrathplus_protocol::{wrath_death::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeDeathPackets(pub(crate) VecDeque<Packet>);

/// The spirit healer's location, `(map, x, y)`; map -1 clears it.
#[derive(Resource, Default)]
pub(crate) struct DeathRelease(pub(crate) Option<(u32, f32, f32)>);

pub(crate) fn register(app: &mut App) {
    app.init_resource::<NativeDeathPackets>()
        .init_resource::<DeathRelease>()
        .net_handler(SessionEventKind::WrathDeath, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeDeathPackets>,
    mut release: ResMut<DeathRelease>,
) {
    match event {
        SessionEvent::WrathDeath {
            packet: Packet::ReleaseLocation { map, position },
        } => {
            release.0 = u32::try_from(map)
                .ok()
                .map(|map| (map, position[0], position[1]));
        }
        SessionEvent::WrathDeath { packet } => packets.0.push_back(packet),
        _ => {}
    }
}

fn clear(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeDeathPackets>,
    mut release: ResMut<DeathRelease>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
        release.0 = None;
    }
}

#[cfg(test)]
#[path = "wrath_death_capture_tests.rs"]
pub(crate) mod tests;
