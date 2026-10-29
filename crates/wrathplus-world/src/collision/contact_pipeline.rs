//! Contact-pipeline configuration checks.

use super::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

/// A static 10×10 floor and a kinematic 4×4 slab on it: a transport resting on terrain.
fn overlapping_world(broad_phase: bool) -> App {
    let mut app = App::new();
    let physics = PhysicsPlugins::new(bevy::app::PostUpdate);
    app.add_plugins((
        MinimalPlugins,
        bevy::transform::TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::scene::ScenePlugin,
    ));
    if broad_phase {
        app.add_plugins(physics);
    } else {
        app.add_plugins(physics.build().disable::<BvhBroadPhasePlugin>());
    }
    app.init_asset::<Mesh>();
    // `update()` never runs plugin `finish()`, where avian seats its diagnostics resources.
    app.finish();
    app.cleanup();

    let quad = |half: f32, y: f32| {
        (
            vec![
                Vec3::new(-half, y, -half),
                Vec3::new(half, y, -half),
                Vec3::new(half, y, half),
                Vec3::new(-half, y, half),
            ],
            vec![[0u32, 2, 1], [0, 3, 2]],
        )
    };
    let (fv, ft) = quad(5.0, 0.0);
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::trimesh(fv, ft),
        Transform::default(),
    ));
    // Coplanar and overlapping, so the AABBs intersect.
    let (sv, st) = quad(2.0, 0.0);
    app.world_mut().spawn((
        RigidBody::Kinematic,
        Collider::trimesh(sv, st),
        Transform::default(),
    ));
    // Two updates: the first seats `Position`/`Rotation` and the collider trees, and physics
    // first steps on the second.
    app.update();
    app.update();
    app
}

fn active_pairs(app: &App) -> usize {
    app.world().resource::<ContactGraph>().active_pairs().len()
}

#[test]
fn the_stock_plugin_set_generates_contact_pairs_we_never_read() {
    // If this reads 0, avian changed and the broad-phase disable may no longer save anything.
    assert!(
        active_pairs(&overlapping_world(true)) > 0,
        "expected the stock broad phase to pair the kinematic slab with the static floor"
    );
}

#[test]
fn dropping_the_broad_phase_removes_the_pairs_but_not_the_shape_casts() {
    let mut app = overlapping_world(false);
    assert_eq!(
        active_pairs(&app),
        0,
        "no broad phase means no contact pairs, so nothing to build manifolds for"
    );
    // The collider BVH is `ColliderTreePlugin`'s, not the broad phase's, so casts still work.
    let hit = app
        .world_mut()
        .run_system_once(|spatial: SpatialQuery| {
            spatial.cast_ray(
                Vec3::new(0.0, 5.0, 0.0),
                Dir3::NEG_Y,
                10.0,
                true,
                &SpatialQueryFilter::default(),
            )
        })
        .expect("run_system_once");
    assert!(
        hit.is_some(),
        "the shape-cast lane must survive the broad phase being gone"
    );
}
