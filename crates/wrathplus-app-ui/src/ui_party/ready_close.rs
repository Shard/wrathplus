//! The initiator's close of a ready check: build 12340 arms a deadline when its own check starts
//! and, once it passes, prints the AFK roll call and sends `MSG_RAID_READY_CHECK_FINISHED`
//! (`0x573390`, run from `ProcessReadyCheckTimer`), which AzerothCore broadcasts to the group as
//! the check's end. Only the client whose guid started the check sends it. Labelled: the deadline
//! is the 30 s the `READY_CHECK` event announces; the stored timeout (`0xbeb61c`) is not read.

use crate::app_core::group::GroupState;
use crate::net::{ClientCommand, NetCommands};
use crate::app_core::session::SelfGuid;
use wrathplus_protocol::wrath_group::raid::Packet;
use bevy::prelude::*;

/// Seconds from our own check's start to its close.
const READY_CHECK_SECONDS: f64 = 30.0;

/// The ready-event journal's read cursor and our own check's deadline.
#[derive(Default)]
pub(super) struct Close {
    seen: usize,
    deadline: Option<f64>,
}

pub(super) fn close_own_ready_check(
    group: Res<GroupState>,
    me: Option<Res<SelfGuid>>,
    time: Res<Time>,
    commands: Option<Res<NetCommands>>,
    mut close: Local<Close>,
) {
    let now = time.elapsed_secs_f64();
    let events = &group.wrath_raid.ready_events;
    // `clear_session` releases the journal.
    if close.seen > events.len() {
        *close = Close::default();
    }
    for event in &events[close.seen..] {
        match event {
            Packet::ReadyStart { initiator } => {
                close.deadline = me
                    .as_ref()
                    .is_some_and(|me| me.0 == Some(*initiator))
                    .then_some(now + READY_CHECK_SECONDS);
            }
            Packet::ReadyFinished => close.deadline = None,
            _ => {}
        }
    }
    close.seen = events.len();
    if close.deadline.is_some_and(|d| now >= d) {
        close.deadline = None;
        if let Some(commands) = commands {
            let _ = commands.0.send(ClientCommand::NativeParty(
                wrathplus_protocol::wrath_party::Command::ReadyCheckFinished,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn only_our_own_check_closes_and_only_once_its_deadline_passes() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.init_resource::<GroupState>()
            .insert_resource(SelfGuid(Some(7)))
            .insert_resource(NetCommands(tx))
            .init_resource::<Time>()
            .add_systems(Update, close_own_ready_check);
        let at = |app: &mut App, secs: u64, event: Option<Packet>| {
            if let Some(e) = event {
                app.world_mut()
                    .resource_mut::<GroupState>()
                    .wrath_raid
                    .ready_events
                    .push(e);
            }
            app.world_mut()
                .resource_mut::<Time>()
                .advance_to(Duration::from_secs(secs));
            app.update();
            rx.try_iter().collect::<Vec<_>>()
        };
        // Someone else's check: never ours to close.
        assert!(at(&mut app, 1, Some(Packet::ReadyStart { initiator: 9 })).is_empty());
        assert!(at(&mut app, 40, None).is_empty());
        // Ours, answered early by a finish: nothing.
        assert!(at(&mut app, 41, Some(Packet::ReadyStart { initiator: 7 })).is_empty());
        assert!(at(&mut app, 42, Some(Packet::ReadyFinished)).is_empty());
        assert!(at(&mut app, 80, None).is_empty());
        // Ours, run out: one close at the deadline.
        assert!(at(&mut app, 100, Some(Packet::ReadyStart { initiator: 7 })).is_empty());
        assert!(at(&mut app, 129, None).is_empty());
        let sent = at(&mut app, 130, None);
        assert!(
            matches!(
                sent[..],
                [ClientCommand::NativeParty(
                    wrathplus_protocol::wrath_party::Command::ReadyCheckFinished
                )]
            ),
            "{sent:?}"
        );
        assert!(at(&mut app, 200, None).is_empty());
    }
}
