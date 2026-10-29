use super::*;

/// A tile-sized grid: 256 quads -> 512 triangles per chunk, `chunks` chunks.
fn grid(chunks: usize) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let (mut verts, mut tris) = (Vec::new(), Vec::new());
    for c in 0..chunks {
        let base = verts.len() as u32;
        for r in 0..17u32 {
            for q in 0..17u32 {
                verts.push(Vec3::new(
                    q as f32,
                    ((q + r + c as u32) % 5) as f32,
                    r as f32,
                ));
            }
        }
        for r in 0..16u32 {
            for q in 0..16u32 {
                let i = base + r * 17 + q;
                tris.push([i, i + 1, i + 17]);
                tris.push([i + 1, i + 18, i + 17]);
            }
        }
    }
    (verts, tris)
}

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        bevy::asset::AssetPlugin::default(),
    ));
    app.init_asset::<Mesh>();
    // Avian's collider hooks, so an attach costs what it does in the client; its schedules
    // never run.
    app.add_plugins(PhysicsPlugins::default());
    app.finish();
    app.cleanup();
    app
}

static TEARDOWN_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static DEFERRED_ERRORS: AtomicUsize = AtomicUsize::new(0);

fn count_deferred_error(
    _error: bevy::ecs::error::BevyError,
    _context: bevy::ecs::error::ErrorContext,
) {
    DEFERRED_ERRORS.fetch_add(1, Ordering::SeqCst);
}

fn teardown_test_app() -> App {
    DEFERRED_ERRORS.store(0, Ordering::SeqCst);
    let mut app = App::new();
    app.set_error_handler(count_deferred_error);
    app.add_plugins((
        TaskPoolPlugin::default(),
        bevy::asset::AssetPlugin::default(),
    ));
    app.init_asset::<Mesh>();
    app.add_plugins(PhysicsPlugins::default());
    app.finish();
    app.cleanup();
    app
}

fn ready_trimesh(static_body: bool) -> PendingCollider {
    let (verts, tris) = grid(1);
    PendingCollider::ready(Collider::trimesh(verts, tris), None, static_body)
}

/// Polled until done, not slept for: the timing is the pool's and is not asserted.
#[test]
fn a_finished_task_hands_its_shape_over_and_attaches() {
    let mut app = test_app();
    let (verts, tris) = grid(4);
    let task = build_collider_task(verts, tris);
    let e = app
        .world_mut()
        .spawn((Transform::default(), PendingCollider::new(task, None, true)))
        .id();
    for _ in 0..100_000 {
        app.world_mut().run_system_once(finish_colliders).unwrap();
        if app.world().entity(e).contains::<Collider>() {
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        app.world().entity(e).contains::<Collider>(),
        "the pool finished the build (or never did — a hung pool, not a budget)"
    );
    assert!(
        app.world().entity(e).contains::<RigidBody>(),
        "a static body rides the attach"
    );
    assert!(app.world().get::<PendingCollider>(e).is_none());
}

#[test]
fn diagnostic_source_survives_pending_attach() {
    let mut app = test_app();
    let e = app
        .world_mut()
        .spawn((
            Transform::default(),
            ready_trimesh(true).with_source(crate::collision::ColliderSource::Terrain),
        ))
        .id();
    app.world_mut().run_system_once(finish_colliders).unwrap();
    assert_eq!(
        app.world().get::<crate::collision::ColliderSource>(e),
        Some(&crate::collision::ColliderSource::Terrain)
    );
    assert!(app.world().get::<PendingCollider>(e).is_none());
}

/// Staged already built with a zero budget, so each call attaches exactly one.
#[test]
fn attach_budget_spreads_a_burst_without_losing_colliders() {
    const N: usize = 40;
    let mut app = test_app();
    app.insert_resource(AttachBudget(std::time::Duration::ZERO));
    let (verts, tris) = grid(16);
    let entities: Vec<Entity> = (0..N)
        .map(|_| {
            let collider = Collider::trimesh(verts.clone(), tris.clone());
            app.world_mut()
                .spawn((
                    Transform::default(),
                    PendingCollider::ready(collider, None, true),
                ))
                .id()
        })
        .collect();

    let attached = |app: &mut App| {
        entities
            .iter()
            .filter(|e| app.world().entity(**e).contains::<Collider>())
            .count()
    };

    app.world_mut().run_system_once(finish_colliders).unwrap();
    assert_eq!(
        attached(&mut app),
        1,
        "a zero budget is past after the first attach: exactly one lands per call"
    );

    // Drain: every remaining build must still land, none lost to the deferral.
    for _ in 0..N {
        if attached(&mut app) == N {
            break;
        }
        app.world_mut().run_system_once(finish_colliders).unwrap();
    }
    assert_eq!(attached(&mut app), N, "a finished collider was dropped");
    for e in &entities {
        assert!(app.world().entity(*e).contains::<RigidBody>());
        assert!(!app.world().entity(*e).contains::<PendingCollider>());
    }
}

/// Regression for Avian's same-entity linked relationship teardown: the finish path inserts the
/// static body and collider together, Avian defers `ColliderOf { body: self }`, and despawning
/// the body must leave no fallible relationship cleanup behind for the next flush.
#[test]
fn a_finished_static_collider_can_be_despawned_and_flushed_cleanly() {
    let _serial = TEARDOWN_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut app = teardown_test_app();
    let entity = app
        .world_mut()
        .spawn((Transform::default(), ready_trimesh(true)))
        .id();

    app.world_mut().run_system_once(finish_colliders).unwrap();
    app.world_mut().flush();

    let collider_of = app
        .world()
        .get::<ColliderOf>(entity)
        .expect("Avian attached the finished collider to its same-entity static body");
    assert_eq!(collider_of.body, entity);

    app.world_mut().entity_mut(entity).despawn();
    app.world_mut().flush();

    assert!(app.world().get_entity(entity).is_err());
    assert_eq!(
        DEFERRED_ERRORS.load(Ordering::SeqCst),
        0,
        "same-entity ColliderOf teardown reported a deferred command failure"
    );
}

/// The streamed bare-collider shape also occurs below an owning body. Both Bevy's `ChildOf` and
/// Avian's linked `RigidBodyColliders` then own the collider, so parent teardown exercises the
/// competing linked-spawn paths and their deferred relationship cleanup.
#[test]
fn a_finished_child_collider_goes_with_its_parent_body_without_deferred_errors() {
    let _serial = TEARDOWN_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut app = teardown_test_app();
    let body = app
        .world_mut()
        .spawn((RigidBody::Static, Transform::default()))
        .id();
    let collider = app
        .world_mut()
        .spawn((Transform::default(), ChildOf(body), ready_trimesh(false)))
        .id();

    app.world_mut().run_system_once(finish_colliders).unwrap();
    app.world_mut().flush();

    let collider_of = app
        .world()
        .get::<ColliderOf>(collider)
        .expect("Avian attached the finished child collider to its parent body");
    assert_eq!(collider_of.body, body);

    app.world_mut().entity_mut(body).despawn();
    app.world_mut().flush();

    assert!(app.world().get_entity(body).is_err());
    assert!(
        app.world().get_entity(collider).is_err(),
        "linked parent teardown left its collider alive"
    );
    assert_eq!(
        DEFERRED_ERRORS.load(Ordering::SeqCst),
        0,
        "parent/child linked-spawn teardown reported a deferred command failure"
    );
}
