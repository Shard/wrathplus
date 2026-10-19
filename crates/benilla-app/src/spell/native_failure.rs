//! Sole ordered receipt owner for native spell failures pending complete semantics.
//! 00809c70 and 00806ad0 share cleanup helpers, but the former also has caster-side
//! effects not yet identified. Do not replay the shared SpellFailedOther cleanup
//! here: AzerothCore SendInterrupted sends both packets for the same interruption.
use crate::net::NetHandlerApp;
use benilla_protocol::{
    wrath_spell_failure::{PetCastFailed, SpellFailure},
    SessionEvent, SessionEventKind,
};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Receipt {
    Spell(SpellFailure),
    Pet(PetCastFailed),
}
/// Lossless ordered receipts, including duplicates and fields not yet interpreted.
#[derive(Resource, Default)]
pub(crate) struct NativeFailures(pub(crate) VecDeque<Receipt>);
pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeFailures>()
        .net_handler(SessionEventKind::WrathSpellFailure, receive)
        .net_handler(SessionEventKind::WrathPetCastFailed, receive)
        .net_handler(SessionEventKind::Disconnected, clear)
        .net_handler(SessionEventKind::Connected, connected);
}
#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
fn receive(
    In(event): In<SessionEvent>,
    mut receipts: ResMut<NativeFailures>,
    // `Option`: a harness without the unit-cast feed still captures receipts.
    unit_casts: Option<ResMut<super::unit_cast::CastEdges>>,
    spells: Option<Res<crate::ui_action::Spells>>,
    mut errors: Option<ResMut<crate::ui_action::UiErrorKeys>>,
    // `SPELL_CAST_FAILED` for the build-12340 combat log (`0x751ad0`).
    native_log: Option<ResMut<crate::combat_log::native::NativeCombatLog>>,
    // Out of ammo stops our Auto Shot.
    auto_repeat: (
        Option<Res<crate::net::SelfGuid>>,
        Option<ResMut<super::AutoRepeatActive>>,
        Option<Res<crate::net::NetCommands>>,
        Query<Entity, With<crate::net::SelfPlayer>>,
        Commands,
    ),
) {
    if let Some(mut log) = native_log {
        log.record(&event);
    }
    match event {
        SessionEvent::WrathSpellFailure(value) => {
            // `0x809c70`: our own failure for want of ammo (`NEED_AMMO`, `_AMMO_POUCH`,
            // `_EXOTIC_AMMO`, 52-54) stops auto-repeat through `0x807560(1)`, which sends
            // `CMSG_CANCEL_AUTO_REPEAT_SPELL` and clears the running spell.
            let (me, active, net, body, mut commands) = auto_repeat;
            if me.is_some_and(|me| me.0 == Some(value.caster)) && stops_auto_repeat(value.reason) {
                if let (Some(mut active), Some(net)) = (active, net) {
                    crate::creature_anim::cancel_auto_repeat_local(
                        body.single().ok(),
                        &mut active,
                        &mut commands,
                        &net,
                    );
                }
            }
            // The result ladder and the local extras (`0x809c70`), in `super::unit_cast`.
            if let Some(mut unit_casts) = unit_casts {
                unit_casts.push(super::unit_cast::CastEdge::Result {
                    caster: value.caster,
                    spell_id: value.spell_id,
                    cast_count: value.cast_count,
                    reason: Some(value.reason),
                    failure: true,
                });
            }
            receipts.0.push_back(Receipt::Spell(value));
        }
        SessionEvent::WrathPetCastFailed(value) => {
            // 00806c30 requires a spell record before 00805610. Only the seven
            // independently proven no-format branches are presented; do not reuse
            // Vanilla reason tables or infer the unresolved cleanup tail.
            if spells
                .as_ref()
                .is_some_and(|s| s.catalog.get(value.spell_id).is_some())
            {
                if let (Some(key), Some(errors)) = (pet_message(value.reason), errors.as_mut()) {
                    errors.0.push(crate::ui_action::UiError::key(key));
                }
            }
            receipts.0.push_back(Receipt::Pet(value));
        }
        _ => {}
    }
}
/// The failures `0x809c70` ends auto-repeat on: `SPELL_FAILED_NEED_AMMO`, `_NEED_AMMO_POUCH` and
/// `_NEED_EXOTIC_AMMO`.
fn stops_auto_repeat(reason: u8) -> bool {
    matches!(reason, 52..=54)
}

fn pet_message(reason: u8) -> Option<&'static str> {
    Some(match reason {
        0x01 => "ERR_PET_SPELL_AFFECTING_COMBAT",
        0x17 => "ERR_PET_SPELL_DEAD",
        0x38 => "ERR_PET_SPELL_NOPATH",
        0x39 => "ERR_PET_SPELL_NOT_BEHIND",
        0x61 => "ERR_PET_SPELL_OUT_OF_RANGE",
        0x67 => "ERR_PET_SPELL_ROOTED",
        0x6d => "ERR_PET_SPELL_TARGETS_DEAD",
        _ => return None,
    })
}
fn clear(In(event): In<SessionEvent>, mut receipts: ResMut<NativeFailures>) {
    if matches!(event, SessionEvent::Disconnected { .. }) {
        receipts.0.clear();
    }
}
fn connected(
    In(event): In<SessionEvent>,
    dialect: Option<Res<crate::net::SessionDialect>>,
    mut receipts: ResMut<NativeFailures>,
) {
    if matches!(event, SessionEvent::Connected { .. })
        && dialect.as_deref() == Some(&crate::net::SessionDialect::Wrath12340)
    {
        receipts.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
    #[test]
    fn only_the_ammo_failures_stop_auto_repeat() {
        for reason in 0..=u8::MAX {
            assert_eq!(
                stops_auto_repeat(reason),
                (52..=54).contains(&reason),
                "{reason}"
            );
        }
    }

    #[test]
    fn native_receipts_preserve_order_duplicates_and_raw_fields_without_shared_projection() {
        let mut app = App::new();
        register(&mut app);
        app.insert_resource(crate::net::SessionDialect::Wrath12340);
        let mut adapter = WrathEventAdapter::new();
        let mut expected = VecDeque::new();
        for opcode in [0x133, 0x138, 0x133, 0x138] {
            let mut body = if opcode == 0x133 { vec![1, 42] } else { vec![] };
            body.push(255);
            body.extend(123u32.to_le_bytes());
            body.push(0x1d);
            if opcode == 0x138 {
                for arg in [u32::MAX, 0x80000000] {
                    body.extend(arg.to_le_bytes());
                }
            }
            let WrathDispatch::Handled { session_events, .. } =
                adapter.dispatch(opcode, &body, 0).unwrap()
            else {
                panic!()
            };
            assert_eq!(session_events.len(), 1);
            expected.push_back(match &session_events[0] {
                SessionEvent::WrathSpellFailure(v) => Receipt::Spell(v.clone()),
                SessionEvent::WrathPetCastFailed(v) => Receipt::Pet(v.clone()),
                _ => panic!("shared projection"),
            });
            crate::net::handlers::dispatch(app.world_mut(), session_events);
        }
        assert_eq!(app.world().resource::<NativeFailures>().0, expected);
        app.insert_resource(crate::net::SessionDialect::Vanilla5875);
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Connected {
                self_guid: 42,
                name: "Synthetic".into(),
                billing_time_rested: 0,
                tutorial_flags: None,
                addon_info: None,
            }],
        );
        assert_eq!(app.world().resource::<NativeFailures>().0, expected);
        app.insert_resource(crate::net::SessionDialect::Wrath12340);
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Connected {
                self_guid: 42,
                name: "Synthetic".into(),
                billing_time_rested: 0,
                tutorial_flags: None,
                addon_info: None,
            }],
        );
        assert!(app.world().resource::<NativeFailures>().0.is_empty());
    }
    #[test]
    fn registered_capture_never_duplicates_shared_cleanup_or_clears_colliding_player_cast() {
        use crate::creature_anim::{CastEvent, Casting};
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        app.insert_resource(crate::net::SessionDialect::Wrath12340);
        app.world_mut().resource_mut::<crate::net::SelfGuid>().0 = Some(42);
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
        for kind in [
            SessionEventKind::WrathSpellFailure,
            SessionEventKind::WrathPetCastFailed,
        ] {
            assert_eq!(census[&kind].len(), 1);
            assert!(census[&kind][0].contains("native_failure::receive"));
        }
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
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(0x2a6, &spell, 0).unwrap()
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
                end: benilla_protocol::SessionEnd::Lost,
            }],
        );
        assert!(app.world().resource::<NativeFailures>().0.is_empty());
    }
    #[test]
    fn native_pet_known_spell_has_one_proven_key_per_receipt_and_keeps_all_arguments() {
        use std::collections::HashMap;
        let mut app = App::new();
        register(&mut app);
        app.init_resource::<crate::ui_action::UiErrorKeys>();
        app.insert_resource(crate::ui_action::Spells {
            catalog: benilla_formats::SpellCatalog::from_displays(HashMap::from([(
                123,
                benilla_formats::SpellDisplay {
                    name: "Synthetic".into(),
                    ..Default::default()
                },
            )])),
            ..crate::ui_action::Spells::empty_for_tests()
        });
        for reason in 0..=255 {
            let value = PetCastFailed {
                cast_count: 9,
                spell_id: 123,
                reason,
                arguments: vec![u32::MAX, 77],
            };
            crate::net::handlers::dispatch(
                app.world_mut(),
                vec![SessionEvent::WrathPetCastFailed(value.clone())],
            );
            let errors = &app.world().resource::<crate::ui_action::UiErrorKeys>().0;
            let expected = [
                (0x01, "ERR_PET_SPELL_AFFECTING_COMBAT"),
                (0x17, "ERR_PET_SPELL_DEAD"),
                (0x38, "ERR_PET_SPELL_NOPATH"),
                (0x39, "ERR_PET_SPELL_NOT_BEHIND"),
                (0x61, "ERR_PET_SPELL_OUT_OF_RANGE"),
                (0x67, "ERR_PET_SPELL_ROOTED"),
                (0x6d, "ERR_PET_SPELL_TARGETS_DEAD"),
            ];
            if let Some((_, key)) = expected.iter().find(|(r, _)| *r == reason) {
                assert_eq!(errors.len(), 1);
                assert_eq!(errors[0].key, *key);
                assert!(errors[0].args.is_empty());
            } else {
                assert!(errors.is_empty());
            }
            assert_eq!(
                app.world().resource::<NativeFailures>().0.back(),
                Some(&Receipt::Pet(value))
            );
            app.world_mut()
                .resource_mut::<crate::ui_action::UiErrorKeys>()
                .0
                .clear();
        }
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::WrathPetCastFailed(PetCastFailed {
                cast_count: 1,
                spell_id: 999,
                reason: 0x61,
                arguments: vec![],
            })],
        );
        assert!(app
            .world()
            .resource::<crate::ui_action::UiErrorKeys>()
            .0
            .is_empty());
        assert_eq!(app.world().resource::<NativeFailures>().0.len(), 257);
    }
}
