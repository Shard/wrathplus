//! Transport-only ordered native mirror-image replies. App owns any future reducer/UI feed.
//! No rendering, UI feed or inferred appearance state.
//!
//! The request: the unit display build (`0x730100`) of a unit whose `UNIT_FIELD_FLAGS_2` carries
//! `UNIT_FLAG2_MIRROR_IMAGE` (`0x10`) asks for its caster's look once per object (`0x716f10`:
//! `[unit+0xa30] |= 0x20000`, then `CMSG_GET_MIRRORIMAGE_DATA` with the unit's guid).
use super::{Guid, NetCommands, NetHandlerApp, ObjectStore};
use wrathplus_protocol::types::Field;
use wrathplus_protocol::{wrath_mirror_image::Packet, SessionEvent, SessionEventKind};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use std::collections::VecDeque;

/// `UNIT_FLAG2_MIRROR_IMAGE` (AzerothCore `UnitDefines.h`).
const MIRROR_IMAGE: u32 = 0x10;

#[derive(Resource, Default)]
pub(crate) struct NativeMirrorImagePackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeMirrorImagePackets>()
        .net_handler(SessionEventKind::WrathMirrorImage, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear)
        .add_systems(Update, query_mirror_images);
}

/// Send `CMSG_GET_MIRRORIMAGE_DATA` for each unit object that shows the mirror-image flag, once
/// per object; a despawned object's latch goes with it.
fn query_mirror_images(
    units: Query<(Entity, &ObjectStore, &Guid), Changed<ObjectStore>>,
    mut gone: RemovedComponents<ObjectStore>,
    commands: Option<Res<NetCommands>>,
    mut asked: Local<EntityHashSet>,
) {
    for entity in gone.read() {
        asked.remove(&entity);
    }
    let Some(commands) = commands else {
        return;
    };
    for (entity, store, guid) in &units {
        let flagged = store.is_unit()
            && store
                .0
                .get(Field::UnitFlags2)
                .is_some_and(|f| f & MIRROR_IMAGE != 0);
        if flagged && asked.insert(entity) {
            let _ = commands
                .0
                .send(super::ClientCommand::NativeMirrorImageQuery { guid: guid.0 });
        }
    }
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeMirrorImagePackets>) {
    if let SessionEvent::WrathMirrorImage { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeMirrorImagePackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_mirror_image_capture_tests.rs"]
pub(crate) mod tests;
