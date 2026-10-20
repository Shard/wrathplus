//! The completed-quest list's feed: the VM's `QueryQuestsCompleted` calls go out as `CMSG 0x500`,
//! and each reply the wire layer captured (`crate::net::wrath_quests_completed_capture`) replaces
//! `GetQuestsCompleted`'s list, then fires `QUEST_QUERY_COMPLETE` (`0x5b5190`, event `0x296`).
pub(crate) use crate::net::wrath_quests_completed_capture::{
    NativeQuestsCompletedPackets, QuestsCompleted,
};
use crate::net::{ClientCommand, NetCommands};
use benilla_ui::script::{ScriptDialect, UiScript};
use bevy::prelude::*;

pub(crate) fn register(app: &mut App) {
    // In the feed phase, and not before the in-game UI exists: a reply that lands earlier
    // waits in the FIFO instead of firing into the boot VM.
    app.add_systems(
        Update,
        feed_quests_completed
            .in_set(crate::ui_script::UiFeed)
            .run_if(crate::ui_script::ingame_ui_up),
    );
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
