//! Local repeat cancellation without a later server GO/failure to hide a leaked hold.
use super::*;
use crate::creature_anim::spell_visual::{FxClass, SpellKitFx, SpellKitSound};
use crate::creature_anim::{
    cancel_auto_repeat_local, AutoRepeatArmed, NockLatch, NockedAmmo, RangedHold,
};
use bevy::ecs::system::RunSystemOnce;
use wrathplus_world::rig_anim::RigPose;

const REPEAT: u32 = 75;

fn cancel(app: &mut App, unit: Entity) {
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut active: ResMut<crate::spell::AutoRepeatActive>,
                  net: Res<NetCommands>| {
                cancel_auto_repeat_local(Some(unit), &mut active, &mut commands, &net);
            },
        )
        .unwrap();
}

fn fixture(flags: u32, spell_id: u32) -> (App, Entity) {
    let (mut app, unit) = super::wrath_oneshot::fixture((0, 0));
    app.add_message::<SpellKitFx>()
        .add_message::<SpellKitSound>();
    app.insert_resource(crate::spell::AutoRepeatActive(Some(REPEAT)));
    // Reuse the real clock and production two-bone pose fixture: Load's terminal is Y*4,
    // Stand/Run is Y, and bind is zero. Both full-body and masked nodes are real graph clips.
    {
        let mut entity = app.world_mut().entity_mut(unit);
        entity
            .get_mut::<ModelAnimations>()
            .unwrap()
            .clips
            .iter_mut()
            .find(|c| c.anim_id == 18)
            .unwrap()
            .anim_id = 106;
        entity.remove::<Engaged>();
        entity.insert((
            CastHold {
                anim_id: 106,
                spell_id,
                ranged: true,
            },
            AutoRepeatArmed,
            RangedHold,
            NockedAmmo { display_id: 1 },
            NockLatch,
            Wielded {
                ranged: Some((2, 3)),
                ..Default::default()
            },
            MovementState {
                flags,
                speed: if flags == move_flags::FORWARD {
                    7.0
                } else {
                    0.0
                },
                ..Default::default()
            },
        ));
    }
    app.world_mut().write_message(SheathRequest {
        entity: unit,
        state: 2,
        ceremony: false,
    });
    app.update();
    advance(&mut app, 1200);
    app.update();
    (app, unit)
}

#[test]
fn repeat_cancel_releases_stationary_turning_and_moving_pose_without_go() {
    for flags in [0, move_flags::TURN_RIGHT, move_flags::FORWARD] {
        let (mut app, unit) = fixture(flags, REPEAT);
        assert_ne!(
            app.world().get::<RigPose>(unit).unwrap().locals[1].translation,
            Vec3::Y
        );
        // A long no-GO interval must not itself cancel Auto Shot.
        for _ in 0..14 {
            advance(&mut app, 1000);
        }
        assert_eq!(
            app.world().resource::<crate::spell::AutoRepeatActive>().0,
            Some(REPEAT)
        );
        cancel(&mut app, unit);
        assert!(
            app.world().get::<CastHold>(unit).is_none(),
            "matching hold must drop immediately"
        );
        assert!(app.world().get::<AutoRepeatArmed>(unit).is_none());
        assert!(app.world().get::<RangedHold>(unit).is_none());
        assert!(app.world().get::<NockedAmmo>(unit).is_none());
        assert!(app.world().get::<NockLatch>(unit).is_none());
        let sounds: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<SpellKitSound>>()
            .drain()
            .collect();
        assert!(
            matches!(sounds.as_slice(), [SpellKitSound::StopHold { entity }] if *entity == unit)
        );
        let fx: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<SpellKitFx>>()
            .drain()
            .collect();
        assert!(fx.iter().any(|e| matches!(e, SpellKitFx::Reap {entity, spell_id: REPEAT, class: FxClass::Hold} if *entity == unit)));
        app.update();
        advance(&mut app, 200);
        app.update();
        let driver = app.world().get::<AnimDriver>(unit).unwrap();
        assert!(driver.overlay.is_none() && driver.overlay_fade.is_none());
        assert_eq!(
            app.world().get::<RigPose>(unit).unwrap().locals[1].translation,
            Vec3::Y,
            "cancel restores the production base pose, not bind or Load terminal"
        );
    }
}

#[test]
fn repeat_cancel_preserves_unrelated_hold_and_sound_and_reaps_only_repeat_fx() {
    let (mut app, unit) = fixture(move_flags::FORWARD, 133);
    cancel(&mut app, unit);
    assert_eq!(app.world().get::<CastHold>(unit).unwrap().spell_id, 133);
    let fx: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<SpellKitFx>>()
        .drain()
        .collect();
    assert_eq!(fx.len(), 1);
    assert!(matches!(
        fx[0],
        SpellKitFx::Reap {
            spell_id: REPEAT,
            class: FxClass::Hold,
            ..
        }
    ));
    assert_eq!(
        app.world_mut()
            .resource_mut::<Messages<SpellKitSound>>()
            .drain()
            .count(),
        0
    );
    cancel(&mut app, unit); // inactive early-out, no extra effects
    assert_eq!(
        app.world_mut()
            .resource_mut::<Messages<SpellKitFx>>()
            .drain()
            .count(),
        0
    );
}

#[test]
fn repeat_cancel_reaps_animless_hold_and_rearm_survives_earlier_cancel() {
    let (mut app, unit) = fixture(0, REPEAT);
    app.world_mut().entity_mut(unit).remove::<CastHold>();
    cancel(&mut app, unit);
    assert_eq!(
        app.world_mut()
            .resource_mut::<Messages<SpellKitFx>>()
            .drain()
            .count(),
        1
    );
    assert_eq!(
        app.world_mut()
            .resource_mut::<Messages<SpellKitSound>>()
            .drain()
            .count(),
        1
    );
    app.world_mut().entity_mut(unit).insert(CastHold {
        anim_id: 106,
        spell_id: REPEAT,
        ranged: true,
    });
    app.world_mut()
        .resource_mut::<crate::spell::AutoRepeatActive>()
        .0 = Some(REPEAT);
    app.update();
    assert!(app.world().get::<CastHold>(unit).is_some());
}

#[test]
fn repeat_cancel_observes_queued_start_and_later_rearm_in_command_order() {
    let (mut app, unit) = fixture(0, REPEAT);
    app.world_mut().entity_mut(unit).remove::<CastHold>();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(NetCommands(tx));
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut active: ResMut<crate::spell::AutoRepeatActive>,
                  net: Res<NetCommands>| {
                commands.entity(unit).insert(CastHold {
                    anim_id: 106,
                    spell_id: REPEAT,
                    ranged: true,
                });
                cancel_auto_repeat_local(Some(unit), &mut active, &mut commands, &net);
                cancel_auto_repeat_local(Some(unit), &mut active, &mut commands, &net);
            },
        )
        .unwrap();
    assert!(app.world().get::<CastHold>(unit).is_none());
    assert!(matches!(rx.try_recv(), Ok(ClientCommand::CancelAutoRepeat)));
    assert!(
        rx.try_recv().is_err(),
        "inactive cancellation sends no duplicate wire command"
    );
    app.world_mut()
        .resource_mut::<crate::spell::AutoRepeatActive>()
        .0 = Some(REPEAT);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut active: ResMut<crate::spell::AutoRepeatActive>,
                  net: Res<NetCommands>| {
                cancel_auto_repeat_local(Some(unit), &mut active, &mut commands, &net);
                commands.entity(unit).insert(CastHold {
                    anim_id: 106,
                    spell_id: REPEAT,
                    ranged: true,
                });
            },
        )
        .unwrap();
    assert_eq!(
        app.world().get::<CastHold>(unit).unwrap().spell_id,
        REPEAT,
        "a later START must survive the earlier cancel, even for the same spell"
    );
}
