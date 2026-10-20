//! One-shot native display receipts and revision-bound name completion. No roster cache.
use super::{native_membership::NativeMembership, native_roster::NativeChannelRosters};
use benilla_ui::script::{UiScript, WrathChannelRosterMember, WrathDisplayChannel};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(super) struct NativeRosterFeed {
    session: Option<u64>,
    pending: Vec<(u64, u64)>, // accepted UI revision, unresolved player GUID
}

pub(super) fn feed(
    script: Option<NonSendMut<UiScript>>,
    channels: Res<super::edit::ChannelState>,
    ledger: Res<NativeMembership>,
    mut receipts: ResMut<NativeChannelRosters>,
    mut state: ResMut<NativeRosterFeed>,
    names: Res<crate::names::NameCache>,
    commands: Res<crate::net::NetCommands>,
    mut deltas: ResMut<crate::net::wrath_social_lists_capture::NativeChannelMembers>,
) {
    let Some(mut script) = script else {
        state.session = None;
        state.pending.clear();
        receipts.0.clear();
        deltas.0.clear();
        return;
    };
    if state.session != Some(script.session()) {
        state.pending.clear();
        // Never deliver old callbacks into a replacement VM, whose revision may restart.
        if state.session.is_some() {
            receipts.0.clear();
        }
        state.session = Some(script.session());
    }
    script.set_wrath_display_channels(
        ledger
            .snapshot(|name| channels.number_of(name))
            .into_iter()
            .map(|row| WrathDisplayChannel {
                slot: row.slot,
                name: row.name,
                channel_id: row.channel_id,
                active: row.active,
                custom_state: row.custom_state,
            })
            .collect(),
    );
    while let Some(receipt) = receipts.0.pop_front() {
        let members: Vec<_> = receipt
            .members
            .into_iter()
            .map(|(guid, flags)| WrathChannelRosterMember {
                guid,
                flags,
                name: names.answered_player_name(guid).map(str::to_owned),
            })
            .collect();
        let unresolved: Vec<_> = members
            .iter()
            .filter(|row| row.name.is_none())
            .map(|row| row.guid)
            .collect();
        if let Some(revision) = script.receive_wrath_channel_roster(
            receipt.channel_type,
            &receipt.channel,
            receipt.flags,
            members,
        ) {
            // A later accepted receipt replaces the pending revision, never replays it.
            state.pending.clear();
            for guid in unresolved {
                names.resolve(guid, &commands);
                state.pending.push((revision, guid));
            }
        }
    }
    // A watched channel's deltas (`SMSG_USERLIST_*`) patch the selected roster after its full
    // list; the UI fires the roster events itself once names are complete. Member counts answer a
    // request no 12340 binding sends, so they are dropped.
    use benilla_protocol::wrath_channel_members::{Delta, Packet};
    use benilla_ui::script::WrathChannelRosterDelta as D;
    while let Some(packet) = deltas.0.pop_front() {
        let Packet::Roster {
            channel,
            delta,
            channel_flags,
            count,
        } = packet
        else {
            continue;
        };
        let (delta, added) = match delta {
            Delta::Add { guid, flags } => (D::Add { guid, flags }, Some(guid)),
            Delta::Update { guid, flags } => (D::Update { guid, flags }, None),
            Delta::Remove { guid } => (D::Remove { guid }, None),
        };
        if let (Some(revision), Some(guid)) = (
            script.apply_wrath_channel_roster_delta(&channel, delta, channel_flags, count),
            added,
        ) {
            names.resolve(guid, &commands);
            state.pending.push((revision, guid));
        }
    }
    state.pending.retain(|&(revision, guid)| {
        if let Some(name) = names.answered_player_name(guid) {
            script.resolve_wrath_channel_roster_name(revision, guid, name.to_owned());
            false
        } else {
            true
        }
    });
}

/// UI selection emits Watch before DisplayList; preserve that queue order exactly.
pub(super) fn drain_requests(
    script: Option<NonSendMut<UiScript>>,
    commands: Res<crate::net::NetCommands>,
) {
    use crate::net::ClientCommand;
    use benilla_ui::script::WrathChannelRosterRequest;
    let Some(mut script) = script else {
        return;
    };
    for request in script.take_wrath_channel_roster_requests() {
        let command = match request {
            WrathChannelRosterRequest::Watch { name } => ClientCommand::SetChannelWatch { name },
            WrathChannelRosterRequest::DisplayList { name } => {
                ClientCommand::ChannelDisplayList { name }
            }
        };
        let _ = commands.0.send(command);
    }
}

pub(super) fn reset(
    script: Option<NonSendMut<UiScript>>,
    mut ledger: ResMut<NativeMembership>,
    mut state: ResMut<NativeRosterFeed>,
) {
    ledger.clear();
    *state = NativeRosterFeed::default();
    if let Some(mut script) = script {
        script.reset_wrath_channel_roster();
        script.set_wrath_display_channels(Vec::new());
    }
}

pub(super) fn disconnect(
    mut events: MessageReader<crate::net::DisconnectedMessage>,
    script: Option<NonSendMut<UiScript>>,
    ledger: ResMut<NativeMembership>,
    state: ResMut<NativeRosterFeed>,
) {
    if events.read().next().is_some() {
        reset(script, ledger, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::wrath_channel::ChannelList;
    use bevy::ecs::system::RunSystemOnce;

    fn world() -> World {
        let mut world = World::new();
        let script = UiScript::new().unwrap();
        let mut channels = super::super::edit::ChannelState::default();
        let slot = channels.claim_slot("World").unwrap();
        let mut ledger = NativeMembership::default();
        ledger.joined("World", 0, Some(slot));
        world.insert_resource(channels);
        world.insert_resource(ledger);
        world.insert_resource(NativeChannelRosters::default());
        world.insert_resource(crate::net::wrath_social_lists_capture::NativeChannelMembers::default());
        world.insert_resource(NativeRosterFeed::default());
        world.insert_resource(crate::names::NameCache::default());
        let (tx, _) = crossbeam_channel::unbounded();
        world.insert_resource(crate::net::NetCommands(tx));
        world.insert_non_send_resource(script);
        world.run_system_once(feed).unwrap();
        world
            .non_send_resource::<UiScript>()
            .run("SetSelectedDisplayChannel(4)")
            .unwrap();
        world
    }

    fn receipt(world: &mut World, channel_type: u8) {
        world
            .resource_mut::<NativeChannelRosters>()
            .0
            .push_back(ChannelList {
                channel_type,
                channel: "World".into(),
                flags: 0x91,
                members: vec![(0x11, 0xe9)],
            });
        world.run_system_once(feed).unwrap();
    }

    #[test]
    fn ordinary_receipt_cannot_replace_display_revision_or_query_names() {
        let mut world = world();
        receipt(&mut world, 1);
        let pending = world.resource::<NativeRosterFeed>().pending.clone();
        assert_eq!(pending.len(), 1);
        receipt(&mut world, 0);
        assert_eq!(world.resource::<NativeRosterFeed>().pending, pending);
        receipt(&mut world, 2);
        assert_ne!(world.resource::<NativeRosterFeed>().pending, pending);
        assert!(world.resource::<NativeChannelRosters>().0.is_empty());
    }

    #[test]
    fn replacement_vm_and_session_reset_discard_pending_callbacks() {
        let mut world = world();
        receipt(&mut world, 1);
        assert_eq!(world.resource::<NativeRosterFeed>().pending.len(), 1);
        world.insert_non_send_resource(UiScript::new().unwrap());
        receipt(&mut world, 1);
        assert!(world.resource::<NativeRosterFeed>().pending.is_empty());
        assert!(world.resource::<NativeChannelRosters>().0.is_empty());
        world.run_system_once(reset).unwrap();
        assert!(world
            .resource::<NativeMembership>()
            .snapshot(|_| Some(1))
            .is_empty());
        assert_eq!(world.resource::<NativeRosterFeed>().session, None);
    }
    #[test]
    fn selection_requests_send_watch_then_display_exactly_once() {
        let mut world = world();
        let (tx, sent) = crossbeam_channel::unbounded();
        world.insert_resource(crate::net::NetCommands(tx));
        world.run_system_once(drain_requests).unwrap();
        assert!(
            matches!(sent.try_recv(), Ok(crate::net::ClientCommand::SetChannelWatch { name }) if name == "World")
        );
        assert!(
            matches!(sent.try_recv(), Ok(crate::net::ClientCommand::ChannelDisplayList { name }) if name == "World")
        );
        world.run_system_once(drain_requests).unwrap();
        assert!(sent.try_recv().is_err());
        world.run_system_once(reset).unwrap();
        world.run_system_once(drain_requests).unwrap();
        assert!(sent.try_recv().is_err());
    }
}
