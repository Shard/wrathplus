//! Whole-client tests of `crate::spell::native_failure`, from `spell/native_failure.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::spell::native_failure::*;
use bevy::prelude::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::SessionEvent;
use wrathplus_protocol::SessionEventKind;

#[test]
fn registered_capture_never_duplicates_shared_cleanup_or_clears_colliding_player_cast() {
    use crate::creature_anim::{CastEvent, Casting};
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    app.world_mut()
        .resource_mut::<crate::app_core::session::SelfGuid>()
        .0 = Some(42);
    let entity = app
        .world_mut()
        .spawn(Casting {
            spell_id: 123,
            until: None,
        })
        .id();
    app.world_mut()
        .resource_mut::<crate::net::GuidIndex>()
        .0
        .insert(42, entity);
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    // The receipt owner, and for a spell failure the combat log's `SPELL_CAST_FAILED` beside it.
    for (kind, owners) in [
        (SessionEventKind::WrathSpellFailure, 2),
        (SessionEventKind::WrathPetCastFailed, 1),
    ] {
        assert_eq!(census[&kind].len(), owners, "{kind:?}: {:?}", census[&kind]);
        assert!(census[&kind]
            .iter()
            .any(|o| o.contains("native_failure::receive")));
    }
    assert!(census[&SessionEventKind::WrathSpellFailure]
        .iter()
        .any(|o| o.contains("combat_log::on_native_only")));
    let mut cursor = bevy::ecs::message::MessageCursor::<CastEvent>::default();
    let mut adapter = WrathEventAdapter::new();
    let mut spell = vec![1, 42, 7];
    spell.extend(123u32.to_le_bytes());
    spell.push(22);
    let mut pet = vec![9];
    pet.extend(123u32.to_le_bytes());
    pet.push(22);
    for opcode in [0x133, 0x138] {
        let body = if opcode == 0x133 { &spell } else { &pet };
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, body, 0).unwrap()
        else {
            panic!()
        };
        crate::net::handlers::dispatch(app.world_mut(), session_events);
        assert!(app.world().get::<Casting>(entity).is_some());
        assert_eq!(
            cursor
                .read(app.world().resource::<Messages<CastEvent>>())
                .count(),
            0
        );
    }
    let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x2a6, &spell, 0).unwrap()
    else {
        panic!()
    };
    crate::net::handlers::dispatch(app.world_mut(), session_events);
    assert!(app.world().get::<Casting>(entity).is_none());
    assert_eq!(
        cursor
            .read(app.world().resource::<Messages<CastEvent>>())
            .count(),
        1
    );
    assert_eq!(app.world().resource::<NativeFailures>().0.len(), 2);
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "Synthetic".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<NativeFailures>().0.is_empty());
}
