//! Whole-client tests of `crate::net::wrath_spell_log_capture`, from `net/wrath_spell_log_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::wrath_spell_log_capture::*;
use benilla_protocol::messages::{DispelledAura, SpellDispelLog};
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use benilla_protocol::wrath_logs::Packet;
use benilla_protocol::SessionEvent;
use benilla_protocol::SessionEventKind;
use bevy::prelude::*;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeSpellLogPackets>()
        .0
        .iter()
        .cloned()
        .collect()
}

/// Packed victim 5, packed caster 6, Spellsteal, one stolen aura.
fn steal_body() -> Vec<u8> {
    let mut b = vec![0x01, 0x05, 0x01, 0x06];
    b.extend(30449u32.to_le_bytes());
    b.push(0);
    b.extend(1u32.to_le_bytes());
    b.extend(1459u32.to_le_bytes());
    b.push(0);
    b
}

#[test]
fn one_owner_keeps_ordered_duplicates_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathSpellLog];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_spell_log_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    let stolen = Packet::SpellSteal(SpellDispelLog {
        victim: 5,
        caster: 6,
        spell_id: 30449,
        flag: 0,
        auras: vec![DispelledAura {
            spell_id: 1459,
            flag: 0,
        }],
    });
    let body = steal_body();
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
        dispatch(&mut app, &mut adapter, 0x0333, &body);
        dispatch(&mut app, &mut adapter, 0x0333, &body[..body.len() - 1]);
        dispatch(
            &mut app,
            &mut adapter,
            0x0333,
            &[body.as_slice(), &[0]].concat(),
        );
        dispatch(&mut app, &mut adapter, 0x0333, &body);
        assert_eq!(captured(&app), vec![stolen.clone(), stolen.clone()]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}

/// The family's shared events each have exactly one owner, the combat log; none is captured.
#[test]
fn shared_logs_reach_the_combat_log_alone() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    for kind in [
        SessionEventKind::SpellHealLog,
        SessionEventKind::SpellEnergizeLog,
        SessionEventKind::SpellLogMiss,
        SessionEventKind::SpellLogExecute,
        SessionEventKind::PeriodicAuraLog,
        SessionEventKind::DamageShield,
        SessionEventKind::SpellDamageLog,
        SessionEventKind::ProcResist,
        SessionEventKind::DispelFailed,
        SessionEventKind::SpellOrDamageImmune,
        SessionEventKind::SpellDispelLog,
        SessionEventKind::SpellInstaKillLog,
    ] {
        let owners = census
            .get(&kind)
            .unwrap_or_else(|| panic!("{kind:?} has no owner"));
        assert_eq!(owners.len(), 1, "{kind:?}: {owners:?}");
        assert!(owners[0].contains("combat_log"), "{kind:?}: {owners:?}");
    }
}
