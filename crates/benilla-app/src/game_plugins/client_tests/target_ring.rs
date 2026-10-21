//! Whole-client tests of `crate::target::ring::tests`, from `target/ring.rs` (`teardown`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::{ClientCommand, Guid, GuidIndex, NetCommands, ObjectStore, SelfPlayer};
use crate::target::{AttackPick, Selection};
use benilla_protocol::types::ObjectType;
use benilla_protocol::{EntityKind, ObjectFields, SessionEvent};
use benilla_world::model_fade::DespawnFade;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use crossbeam_channel::Receiver;

const FIELD_UNIT_FLAGS: u16 = benilla_protocol::types::Field::UnitFlags.index();

const FIELD_UNIT_HEALTH: u16 = benilla_protocol::types::Field::UnitHealth.index();

const FIELD_UNIT_LEVEL: u16 = benilla_protocol::types::Field::UnitLevel.index();

const FIELD_UNIT_MAXHEALTH: u16 = benilla_protocol::types::Field::UnitMaxHealth.index();

const ME: u64 = 0x0000_0000_0000_0007;

const MOB: u64 = 0xF130_0000_1234_0001;

fn create(guid: u64) -> SessionEvent {
    SessionEvent::ObjectCreate {
        guid,
        kind: EntityKind::Unit,
        display_id: None,
        position: [3.0, 0.0, 0.0],
        orientation: 0.0,
        rotation: None,
        scale: 1.0,
        speeds: None,
        mover: None,
        transport_progress: None,
        transport: None,
        spline: None,
        // `OBJECT_FIELD_TYPE` (2) 0x9, object and unit, as a creature's create carries it:
        // `SetSelection`'s `IsSelectable` refuses any other type.
        fields: ObjectFields::from_pairs(&[
            (2, 0x9),
            (FIELD_UNIT_HEALTH, 100),
            (FIELD_UNIT_MAXHEALTH, 100),
            (FIELD_UNIT_LEVEL, 9),
        ])
        .into_created(ObjectType::Unit),
    }
}

/// The built client with our avatar and one selected live mob, the write channel captured.
fn client_with_selected_mob() -> (App, Entity, Receiver<ClientCommand>) {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(NetCommands(tx));
    let world = app.world_mut();
    // Inserted by a Startup system on a real boot; no schedule runs here.
    world.init_resource::<crate::target::ring::RingState>();
    let me = world
        .spawn((
            SelfPlayer,
            Guid(ME),
            Transform::default(),
            ObjectStore(
                // Player-controlled: a neutral mob is attackable with no faction catalog.
                ObjectFields::from_pairs(&[
                    (FIELD_UNIT_HEALTH, 100),
                    (FIELD_UNIT_MAXHEALTH, 100),
                    (FIELD_UNIT_FLAGS, 0x8),
                ])
                .into_created(ObjectType::Player),
            ),
        ))
        .id();
    world.resource_mut::<GuidIndex>().0.insert(ME, me);
    crate::net::handlers::dispatch(world, vec![create(MOB)]);
    let mob = world.resource::<GuidIndex>().0[&MOB];
    *world.resource_mut::<Selection>() = Selection {
        target: Some(mob),
        guid: Some(MOB),
        ..Default::default()
    };
    // Control: a live target keeps its selection through a ring pass.
    world
        .run_system_once(crate::target::ring::update_ring)
        .expect("the ring runs on the built client");
    assert_eq!(world.resource::<Selection>().guid, Some(MOB));
    // The create's name query is on the channel too; only a selection send matters.
    assert!(
        !rx.try_iter()
            .any(|c| matches!(c, ClientCommand::SetSelection { .. })),
        "a live target is not deselected"
    );
    (app, mob, rx)
}

fn selection_cleared_at(teardown: SessionEvent) {
    let (mut app, mob, rx) = client_with_selected_mob();
    let world = app.world_mut();
    crate::net::handlers::dispatch(world, vec![teardown]);
    assert!(
        world.get_entity(mob).is_ok(),
        "the model outlives the object"
    );
    assert!(world.get::<DespawnFade>(mob).is_some(), "…and fades");
    world
        .run_system_once(crate::target::ring::update_ring)
        .expect("the ring runs on the built client");
    let sel = world.resource::<Selection>();
    assert_eq!(
        (sel.target, sel.guid),
        (None, None),
        "the selection ends with the object, not 2 s later with its model"
    );
    let sent: Vec<_> = rx.try_iter().collect();
    assert!(
        sent.iter()
            .any(|c| matches!(c, ClientCommand::SetSelection { guid: 0 })),
        "the server is told at the teardown: {sent:?}"
    );
}

#[test]
fn a_destroyed_target_clears_the_selection_at_the_destroy() {
    selection_cleared_at(SessionEvent::ObjectDestroyed(MOB));
}

#[test]
fn a_streamed_out_target_clears_the_selection_at_the_stream_out() {
    selection_cleared_at(SessionEvent::ObjectsRemoved(vec![MOB]));
}

/// The nearest-enemy scan never picks a fading model.
#[test]
fn the_nearest_enemy_scan_never_picks_a_torn_down_object() {
    let (mut app, _mob, _rx) = client_with_selected_mob();
    let world = app.world_mut();
    let acquire = |world: &mut World| {
        *world.resource_mut::<Selection>() = Selection::default();
        world
            .run_system_once(
                |mut pick: AttackPick,
                 mut sel: ResMut<Selection>,
                 mut seam: crate::creature_anim::AttackSeam,
                 mut errors: ResMut<crate::ui_action::UiErrorKeys>| {
                    pick.target(None, &mut sel, &mut seam, &mut errors)
                },
            )
            .expect("the scan runs on the built client")
    };
    assert_eq!(
        acquire(world),
        Some(MOB),
        "control: the live mob is acquired"
    );
    crate::net::handlers::dispatch(world, vec![SessionEvent::ObjectsRemoved(vec![MOB])]);
    assert_eq!(acquire(world), None, "the fading model is not a candidate");
}

/// A respawn destroys and re-creates the guid in one tick: the create is a new object, and
/// the fading model does not answer to the guid.
#[test]
fn a_same_tick_recreate_is_a_fresh_object_and_the_old_model_is_nobody() {
    let (mut app, old, _rx) = client_with_selected_mob();
    let world = app.world_mut();
    crate::net::handlers::dispatch(world, vec![SessionEvent::ObjectDestroyed(MOB), create(MOB)]);
    let fresh = world.resource::<GuidIndex>().0[&MOB];
    assert_ne!(fresh, old, "a fresh entity for the fresh object");
    assert!(world.get::<DespawnFade>(fresh).is_none());
    let answering: Vec<Entity> = world
        .query::<(Entity, &Guid)>()
        .iter(world)
        .filter(|(_, g)| g.0 == MOB)
        .map(|(e, _)| e)
        .collect();
    assert_eq!(answering, [fresh], "one object answers to the guid");
}
