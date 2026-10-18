//! Native quest receipts. A PvP-kill tick re-asks the givers' markers, as `SMSG_QUESTUPDATE_ADD_KILL`
//! does; its count lives in the quest-log slot (`crate::ui_quest_log`), whose diff toasts it. The
//! POI responses stay an ordered transport FIFO until the map takes them; no POIs are inferred.
use super::NetHandlerApp;
use benilla_protocol::{wrath_quest::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeQuestPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeQuestPackets>()
        .net_handler(SessionEventKind::WrathQuest, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeQuestPackets>,
    mut quest: ResMut<crate::ui_quest::QuestGiver>,
) {
    match event {
        SessionEvent::WrathQuest {
            packet: Packet::PvpKill { quest: id, current, required },
        } => {
            debug!("net: quest {id} PvP kills at {current}/{required}");
            quest.bump_reask();
        }
        SessionEvent::WrathQuest { packet } => packets.0.push_back(packet),
        _ => {}
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeQuestPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_quest_capture_tests.rs"]
mod tests;
