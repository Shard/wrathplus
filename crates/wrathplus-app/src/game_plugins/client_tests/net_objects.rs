//! Whole-client tests of `crate::net::objects`, from `net/objects.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::motion::Spline;
use crate::motion::SplineStopped;
use crate::net::GuidIndex;
use crate::app_core::session::SelfGuid;
use wrathplus_assets::coords::bevy_to_wow;
use wrathplus_protocol::ObjectFields;
use wrathplus_protocol::SessionEvent;
use bevy::prelude::*;

/// `0x1EB` for an unheld item waits on the active player and replays at its create; `0x1EA`
/// for one is dropped; the destroy takes the cells.
#[test]
fn item_countdowns_live_on_the_item_and_a_queued_enchant_waits_for_it() {
    use crate::items::Countdowns;
    const ME: u64 = 0x0000_0000_0000_0007;
    const SWORD: u64 = 0x4000_0000_0000_0042;
    const STONE: u64 = 0x4000_0000_0000_0043;
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let world = app.world_mut();
    let create = |guid| SessionEvent::ItemCreate {
        guid,
        container: false,
        fields: ObjectFields::from_fields(&[(wrathplus_protocol::types::Field::ObjectEntry, 117)]),
    };
    let cells = |world: &World, guid: u64| -> Option<Countdowns> {
        let e = *world.resource::<GuidIndex>().0.get(&guid)?;
        world.get::<Countdowns>(e).cloned()
    };

    // No active player: the enchant update has nowhere to wait.
    crate::net::handlers::dispatch(
        world,
        vec![
            SessionEvent::ItemEnchantTime {
                item_guid: STONE,
                slot: 1,
                seconds: 60,
            },
            create(STONE),
        ],
    );
    assert_eq!(
        cells(world, STONE),
        Some(Countdowns::default()),
        "dropped, not queued"
    );

    // With the active player resolving, the enchant waits for the item; the lifetime does not.
    let me = world.spawn_empty().id();
    world.resource_mut::<GuidIndex>().0.insert(ME, me);
    world.resource_mut::<SelfGuid>().0 = Some(ME);
    crate::net::handlers::dispatch(
        world,
        vec![
            SessionEvent::ItemEnchantTime {
                item_guid: SWORD,
                slot: 1,
                seconds: 90,
            },
            SessionEvent::ItemTime {
                item_guid: SWORD,
                seconds: 600,
            },
        ],
    );
    assert_eq!(cells(world, SWORD), None, "nothing held yet");
    crate::net::handlers::dispatch(world, vec![create(SWORD)]);
    let c = cells(world, SWORD).expect("the create spawned the item with its cells");
    assert!(
        c.enchant_remaining_ms(1)
            .is_some_and(|ms| (89_000..=90_000).contains(&ms)),
        "the queued enchant replayed as of the create"
    );
    assert_eq!(
        c.lifetime_remaining_ms(),
        None,
        "`0x1EA` for an unheld item has no queue"
    );

    // Held now: both arms write the cells directly; a slot past the array writes nothing.
    crate::net::handlers::dispatch(
        world,
        vec![
            SessionEvent::ItemTime {
                item_guid: SWORD,
                seconds: 600,
            },
            SessionEvent::ItemEnchantTime {
                item_guid: SWORD,
                slot: 9,
                seconds: 30,
            },
        ],
    );
    let c = cells(world, SWORD).unwrap();
    assert!(c.lifetime_remaining_ms().is_some());
    assert!(c.enchant_remaining_ms(1).is_some());

    // The destroy takes the object and its cells with it.
    let e = world.resource::<GuidIndex>().0[&SWORD];
    crate::net::handlers::dispatch(world, vec![SessionEvent::ObjectDestroyed(SWORD)]);
    assert!(!world.resource::<GuidIndex>().0.contains_key(&SWORD));
    assert!(world.get_entity(e).is_err(), "the cells died with the item");
}

/// A real Wrath packet crosses the stateful adapter and the actual object handler before the
/// shared spline sampler supplies movement, facing, and a later stop.
#[test]
fn wrath_monster_move_adapter_drives_and_stops_the_ecs_spline() {
    use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter, SMSG_MONSTER_MOVE};
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    const MOB: u64 = 0x2a;
    fn packet(move_type: u8, spline_id: u32) -> Vec<u8> {
        let mut body = vec![0x01, MOB as u8, 0];
        for value in [1.0f32, 2.0, 3.0] {
            body.extend(value.to_le_bytes());
        }
        body.extend(spline_id.to_le_bytes());
        body.push(move_type);
        if move_type != 1 {
            body.extend(0u32.to_le_bytes());
            body.extend(1_000u32.to_le_bytes());
            body.extend(1u32.to_le_bytes());
            for value in [5.0f32, 2.0, 3.0] {
                body.extend(value.to_le_bytes());
            }
        }
        body
    }
    fn events(adapter: &mut WrathEventAdapter, body: &[u8]) -> Vec<SessionEvent> {
        let WrathDispatch::Handled { session_events, .. } = adapter
            .dispatch(SMSG_MONSTER_MOVE, body, 0)
            .expect("Wrath fixture decodes")
        else {
            panic!("monster move must be handled")
        };
        session_events
    }

    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let entity = app.world_mut().spawn(Transform::default()).id();
    app.world_mut()
        .resource_mut::<GuidIndex>()
        .0
        .insert(MOB, entity);
    let mut adapter = WrathEventAdapter::new();

    crate::net::handlers::dispatch(app.world_mut(), events(&mut adapter, &packet(0, 17)));
    let now = {
        let spline = app
            .world()
            .get::<Spline>(entity)
            .expect("handler installs spline");
        assert_eq!(spline.points, vec![[1.0, 2.0, 3.0], [5.0, 2.0, 3.0]]);
        spline.start + Duration::from_millis(500)
    };
    app.world_mut()
        .run_system_once(move |mut q: Query<(&Spline, &mut Transform)>| {
            let (spline, mut transform) = q.single_mut().unwrap();
            let (position, facing, _) = spline.sample(now);
            transform.translation = wrathplus_assets::coords::wow_to_bevy(position);
            transform.rotation = Quat::from_rotation_y(facing.unwrap());
        })
        .unwrap();
    let transform = app.world().get::<Transform>(entity).unwrap();
    assert!((bevy_to_wow(transform.translation)[0] - 3.0).abs() < 1e-5);
    assert!(transform.rotation.to_euler(EulerRot::YXZ).0.abs() < 1e-5);

    crate::net::handlers::dispatch(app.world_mut(), events(&mut adapter, &packet(1, 18)));
    assert!(app.world().get::<Spline>(entity).is_none());
    assert_eq!(app.world().get::<SplineStopped>(entity).unwrap().0, 18);
}
