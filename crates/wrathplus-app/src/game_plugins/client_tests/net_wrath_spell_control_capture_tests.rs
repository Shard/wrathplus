//! Whole-client tests of `crate::net::wrath_spell_control_capture`, from `net/wrath_spell_control_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_spell_control_capture::*;
use bevy::prelude::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::wrath_spell_control::Packet;
use wrathplus_protocol::SessionEvent;
use wrathplus_protocol::SessionEventKind;

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
    // The capture, the world layer's cooldown delta and forced deselect, then the totem bar.
    let owners = &census[&SessionEventKind::WrathSpellControl];
    assert_eq!(owners.len(), 4, "{owners:?}");
    for (owner, name) in owners.iter().zip([
        "wrath_spell_control_capture::receive",
        "native_control::on_modify_cooldown",
        "native_control::on_clear_target",
        "ui_totems::on_totem_created",
    ]) {
        assert!(owner.contains(name), "{owners:?}");
    }
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
            end: wrathplus_protocol::SessionEnd::Lost,
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

/// `0x8006c0`: an impact plays its kit on a streamed target; `0x5d36a0`: the driven unit's
/// combo update becomes our combo display, anyone else's does not.
#[test]
fn an_impact_plays_its_kit_and_the_driven_units_combo_points_show() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let target = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<crate::net::GuidIndex>()
        .0
        .insert(0x77, target);
    app.world_mut()
        .resource_mut::<crate::net::wrath_vehicle::VehicleWorld>()
        .driven = Some(0xCA);
    let control = |packet| SessionEvent::WrathSpellControl { packet };
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![
            control(Packet::SpellImpact {
                target: 0x77,
                kit_id: 362,
            }),
            control(Packet::PetComboPoints {
                unit: 0xBB,
                target: 0x77,
                points: 2,
            }),
            control(Packet::PetComboPoints {
                unit: 0xCA,
                target: 0x77,
                points: 4,
            }),
        ],
    );
    let combo = app
        .world()
        .resource::<crate::net::wrath_combo_points::NativeComboPoints>();
    assert_eq!((combo.points, combo.target), (4, 0x77));
    let kits: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<crate::app_core::movement::KitPush>>()
        .drain()
        .map(|k| (k.entity, k.kit_id))
        .collect();
    assert_eq!(kits, vec![(target, 362)]);
}
