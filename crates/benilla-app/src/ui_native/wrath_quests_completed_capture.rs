//! Ordered native completed-quest replies (`SMSG 0x501`) and their UI feed: the VM's
//! `QueryQuestsCompleted` calls go out as `CMSG 0x500`, and each reply replaces
//! `GetQuestsCompleted`'s list, then fires `QUEST_QUERY_COMPLETE` (`0x5b5190`, event `0x296`).
//! No quest-log mutation or inferred completion state.
use crate::net::{ClientCommand, NetCommands, NetHandlerApp};
use benilla_protocol::{wrath_quests_completed::Packet, SessionEvent, SessionEventKind};
use benilla_ui::script::{ScriptDialect, UiScript};
use bevy::prelude::*;
use std::collections::VecDeque;

/// The last reply's ids, re-pushed into every new VM: the reference keeps one list, replaced by a
/// reply (`0x5b5190`) and emptied when the world handlers register (`0x5b6df0` calls the same
/// clear `0x5b4cf0`), so a `ReloadUI` still reads it and a new session does not.
#[derive(Resource, Default)]
pub(crate) struct QuestsCompleted {
    pub(crate) ids: Option<Vec<u32>>,
    /// A session boundary emptied the list; the live VM has not been told yet.
    cleared: bool,
}

#[derive(Resource, Default)]
pub(crate) struct NativeQuestsCompletedPackets(pub(crate) VecDeque<Packet>);

pub(crate) fn register(app: &mut App) {
    app.init_resource::<NativeQuestsCompletedPackets>()
        .init_resource::<QuestsCompleted>()
        // In the feed phase, and not before the in-game UI exists: a reply that lands earlier
        // waits in the FIFO instead of firing into the boot VM.
        .add_systems(
            Update,
            feed_quests_completed
                .in_set(crate::ui_script::UiFeed)
                .run_if(crate::ui_script::ingame_ui_up),
        )
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

/// Send each drained `QueryQuestsCompleted`, then apply each reply in order: the list, then the
/// event. A new VM gets the stored list with no event.
fn feed_quests_completed(
    script: Option<NonSendMut<UiScript>>,
    mut packets: ResMut<NativeQuestsCompletedPackets>,
    mut stored: ResMut<QuestsCompleted>,
    commands: Option<Res<NetCommands>>,
    mut memo: Local<crate::ui_script::VmMemo<()>>,
) {
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        packets.0.clear();
        return;
    }
    if std::mem::take(&mut stored.cleared) {
        script.set_wrath_quests_completed(Vec::new());
    }
    if memo.get_reset(&script).1 {
        if let Some(ids) = &stored.ids {
            script.set_wrath_quests_completed(ids.clone());
        }
    }
    for _ in 0..script.take_wrath_quests_completed_queries() {
        if let Some(commands) = &commands {
            let _ = commands.0.send(ClientCommand::NativeQuestsCompletedQuery);
        }
    }
    while let Some(packet) = packets.0.pop_front() {
        script.set_wrath_quests_completed(packet.quest_ids.clone());
        stored.ids = Some(packet.quest_ids);
        script.fire_event("QUEST_QUERY_COMPLETE", vec![]);
    }
}

#[cfg(test)]
#[path = "wrath_quests_completed_capture_tests.rs"]
mod tests;
