//! Ordered native completed-quest replies (`SMSG 0x501`) and the stored list: each reply replaces
//! `GetQuestsCompleted`'s list (`0x5b5190`), which the feed (`crate::ui_native::
//! wrath_quests_completed_capture`) publishes with `QUEST_QUERY_COMPLETE` (event `0x296`).
//! No quest-log mutation or inferred completion state.
use crate::net::NetHandlerApp;
use wrathplus_protocol::{wrath_quests_completed::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

/// The last reply's ids, re-pushed into every new VM: the reference keeps one list, replaced by a
/// reply (`0x5b5190`) and emptied when the world handlers register (`0x5b6df0` calls the same
/// clear `0x5b4cf0`), so a `ReloadUI` still reads it and a new session does not.
#[derive(Resource, Default)]
pub(crate) struct QuestsCompleted {
    pub(crate) ids: Option<Vec<u32>>,
    /// A session boundary emptied the list; the live VM has not been told yet.
    pub(crate) cleared: bool,
}

#[derive(Resource, Default)]
pub(crate) struct NativeQuestsCompletedPackets(pub(crate) VecDeque<Packet>);

pub(crate) fn register(app: &mut App) {
    // A reply that lands before the in-game UI exists waits in the FIFO for the feed.
    app.init_resource::<NativeQuestsCompletedPackets>()
        .init_resource::<QuestsCompleted>()
        .net_handler(SessionEventKind::WrathQuestsCompleted, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeQuestsCompletedPackets>) {
    if let SessionEvent::WrathQuestsCompleted { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeQuestsCompletedPackets>,
    mut stored: ResMut<QuestsCompleted>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
        *stored = QuestsCompleted {
            ids: None,
            cleared: true,
        };
    }
}

#[cfg(test)]
#[path = "wrath_quests_completed_capture_tests.rs"]
pub(crate) mod tests;
