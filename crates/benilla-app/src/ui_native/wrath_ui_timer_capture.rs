//! The server clock's ticker and feed: `0x5488f0` sends `CMSG 0x4F6` every 20 s, the first at once
//! after the world reset `0x548570`; each reply the wire layer captured
//! (`crate::net::wrath_ui_timer_capture`) sets the offset the UI reads through
//! `UiScript::wrath_server_now`.
//!
//! Not built (labelled): the same ticker fires `WORLD_STATE_UI_TIMER_UPDATE` (`0x28b`) each second
//! while a type-3 world-state UI entry is active; that waits on Native UI's Wrath world-state UI
//! projection.
pub(crate) use crate::net::wrath_ui_timer_capture::{NativeUiTimerPackets, ServerClock};
use crate::net::{ClientCommand, NetCommands};
use benilla_ui::script::{ScriptDialect, UiScript};
use bevy::prelude::*;

/// `0x5488f0`'s query period (`iVar1 + 20000` ms).
const QUERY_PERIOD_SECS: f64 = 20.0;

pub(crate) fn register(app: &mut App) {
    // In the feed phase, once the in-game UI exists: replies wait in the FIFO until then.
    app.add_systems(
        Update,
        tick_server_clock
            .in_set(crate::ui_script::UiFeed)
            .run_if(crate::ui_script::ingame_ui_up),
    );
}

/// The local `time()` the offset is taken against (`_time32`).
fn local_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// One frame of `0x5488f0`'s query leg, then each reply's offset, then the VM's copy. A new VM gets
/// the current offset. Vanilla VMs read no server clock.
fn tick_server_clock(
    script: Option<NonSendMut<UiScript>>,
    time: Res<Time>,
    mut packets: ResMut<NativeUiTimerPackets>,
    mut clock: ResMut<ServerClock>,
    commands: Option<Res<NetCommands>>,
    mut memo: Local<crate::ui_script::VmMemo<()>>,
) {
    let wrath = script
        .as_ref()
        .is_some_and(|s| s.dialect() == ScriptDialect::Wrath12340);
    let now = time.elapsed_secs_f64();
    if wrath && clock.due.is_some_and(|due| now >= due) {
        clock.due = Some(now + QUERY_PERIOD_SECS);
        if let Some(commands) = &commands {
            let _ = commands.0.send(ClientCommand::NativeUiTimerQuery);
        }
    }
    while let Some(packet) = packets.0.pop_front() {
        // The reply is a `__time32_t`; the difference wraps as the reference's dword does.
        clock.offset = Some((packet.time as i32).wrapping_sub(local_time() as i32));
        clock.dirty = true;
    }
    let Some(mut script) = script.filter(|_| wrath) else {
        return;
    };
    let fresh_vm = memo.get_reset(&script).1;
    if std::mem::take(&mut clock.dirty) || fresh_vm {
        script.set_wrath_server_time_offset(clock.offset);
    }
}

#[cfg(test)]
#[path = "wrath_ui_timer_capture_tests.rs"]
mod tests;
