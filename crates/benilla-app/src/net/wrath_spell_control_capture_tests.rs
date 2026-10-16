use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeSpellControlPackets>()
        .0
        .iter()
        .cloned()
        .collect()
}

#[test]
fn one_owner_keeps_ordered_duplicates_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathSpellControl];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_spell_control_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    // Slot 1, totem guid 9, 120 s, Searing Totem.
    let mut totem = vec![1];
    totem.extend(9u64.to_le_bytes());
    totem.extend(120_000u32.to_le_bytes());
    totem.extend(3599u32.to_le_bytes());
    let created = Packet::TotemCreated {
        slot: 1,
        totem: 9,
        duration_ms: 120_000,
        spell_id: 3599,
    };
    for event in [
        SessionEvent::Connected {
            self_guid: 1,
            name: "test".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        },
        SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, 0x0413, &totem);
        dispatch(&mut app, &mut adapter, 0x0413, &totem[..totem.len() - 1]);
        dispatch(&mut app, &mut adapter, 0x0413, &totem);
        assert_eq!(captured(&app), vec![created.clone(), created.clone()]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}

/// The family's shared replies keep their existing owners.
#[test]
fn shared_replies_reach_their_existing_owners() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    for kind in [
        SessionEventKind::CooldownEvent,
        SessionEventKind::ClearCooldown,
        SessionEventKind::ChannelStart,
        SessionEventKind::ChannelUpdate,
        SessionEventKind::ResurrectRequest,
        SessionEventKind::PetTameFailure,
        SessionEventKind::PetNameInvalid,
        SessionEventKind::PlaySpellVisual,
        SessionEventKind::SpellModifier,
        SessionEventKind::PetActionFeedback,
        SessionEventKind::PetActionSound,
        SessionEventKind::PetDismissSound,
    ] {
        assert!(
            census.get(&kind).is_some_and(|o| !o.is_empty()),
            "{kind:?} has no owner"
        );
    }
}
