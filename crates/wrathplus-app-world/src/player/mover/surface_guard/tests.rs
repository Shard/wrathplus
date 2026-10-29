use super::*;
use crate::player::mover::tests::{player_capsule, world_from_profile};
use bevy::ecs::system::RunSystemOnce;
use wrathplus_world::collision::{MoverTraceExclusions, WorldCollision};

fn check(profile: &[(f32, f32)], before: Vec3, after: Vec3) -> Option<Recovery> {
    world_from_profile(profile)
        .world_mut()
        .run_system_once(move |world: WorldCollision| crossing(&world, before, after))
        .unwrap()
}

#[test]
fn missed_capsule_floor_crossing_clamps_on_the_actual_segment() {
    let before = Vec3::new(-0.2, 0.08, 0.0);
    let after = Vec3::new(0.2, -0.12, 0.0);
    let r = check(&[(-3.0, 0.0), (3.0, 0.0)], before, after).expect("crossed floor");
    assert!((r.feet.y - SKIN_WIDTH).abs() < 1.0e-5);
    let t = (r.feet.x - before.x) / (after.x - before.x);
    assert!(t > 0.0 && t < 1.0);
    assert!(r.feet.distance(before.lerp(after, t)) < 1.0e-5);
}

#[test]
fn thin_crossing_from_the_surface_rolls_back_without_an_upward_teleport() {
    let before = Vec3::ZERO;
    let r = check(&[(-3.0, 0.0), (3.0, 0.0)], before, Vec3::NEG_Y * 0.173)
        .expect("surface start crosses into solid");
    assert_eq!(r.feet, before);
}

#[test]
fn cliff_hole_and_prior_below_floor_are_not_recovery_evidence() {
    let before = Vec3::new(1.0, 0.1, 0.0);
    let after = Vec3::new(1.1, -0.3, 0.0);
    assert!(check(&[(-3.0, 0.0), (0.0, 0.0)], before, after).is_none());
    // A floor far below the segment is no evidence for preventing a fall into a hole.
    assert!(check(&[(-3.0, -20.0), (3.0, -20.0)], before, after).is_none());
    assert!(check(
        &[(-3.0, 0.0), (3.0, 0.0)],
        Vec3::NEG_Y * 0.01,
        Vec3::NEG_Y * 0.3,
    )
    .is_none());
}

#[test]
fn ceilings_backfaces_steep_sides_and_rising_motion_are_not_floors() {
    // Reversed winding points down: a descending ray must pass through its back.
    assert!(check(&[(3.0, 0.0), (-3.0, 0.0)], Vec3::Y * 0.1, Vec3::NEG_Y * 0.1,).is_none());
    assert!(check(&[(-3.0, 0.0), (3.0, 0.0)], Vec3::NEG_Y * 0.1, Vec3::Y * 0.1,).is_none());
    assert!(check(
        &[(0.0, -3.0), (0.0, 3.0)],
        Vec3::new(-0.1, 0.1, 0.0),
        Vec3::new(0.1, -0.1, 0.0),
    )
    .is_none());
}

#[test]
fn a_normal_landing_on_or_above_the_surface_is_not_a_recovery() {
    for end_y in [0.0, SKIN_WIDTH, 0.1] {
        assert!(check(&[(-3.0, 0.0), (3.0, 0.0)], Vec3::Y, Vec3::Y * end_y,).is_none());
    }
}

#[test]
fn mover_exclusions_and_camera_only_layers_do_not_supply_evidence() {
    for camera_only in [false, true] {
        let mut app = world_from_profile(&[(-3.0, 0.0), (3.0, 0.0)]);
        let entity = app
            .world_mut()
            .query_filtered::<Entity, With<Collider>>()
            .single(app.world())
            .unwrap();
        if camera_only {
            // CollisionLayer is crate-private; bit 2 is the camera audience, not default/walk.
            app.world_mut()
                .entity_mut(entity)
                .insert(CollisionLayers::new(LayerMask(1 << 2), LayerMask::ALL));
            app.update();
        } else {
            app.world_mut()
                .resource_mut::<MoverTraceExclusions>()
                .0
                .insert(entity);
        }
        let result = app
            .world_mut()
            .run_system_once(|world: WorldCollision| {
                assert!(support(&world, Vec3::NEG_Y * 0.003).is_none());
                crossing(&world, Vec3::Y * 0.1, Vec3::NEG_Y * 0.1)
            })
            .unwrap();
        assert!(result.is_none());
    }
}

#[test]
fn settling_and_rooted_movers_do_not_relocate_even_below_a_floor() {
    for rooted in [false, true] {
        let mut app = world_from_profile(&[(-3.0, 0.0), (3.0, 0.0)]);
        app.world_mut()
            .run_system_once(move |world: WorldCollision| {
                let mut player = Player {
                    pos: Vec3::NEG_Y * 0.069,
                    settling: !rooted,
                    ..Default::default()
                };
                player.modes.rooted = rooted;
                player.vel_y = -12.0;
                let before = player.pos;
                let mut time = Time::default();
                time.advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
                step(
                    &mut player,
                    &time,
                    &world,
                    &player_capsule(),
                    true,
                    Vec3::X,
                    7.0,
                    false,
                    false,
                    None,
                    None,
                    2.5,
                );
                assert!(player.pos.distance(before) < 1.0e-6);
            })
            .unwrap();
    }
}

#[test]
fn ordinary_fall_lands_on_the_capsule_and_a_cliff_keeps_falling() {
    for floor in [false, true] {
        let profile = if floor {
            &[(-3.0, 0.0), (3.0, 0.0)][..]
        } else {
            &[(-3.0, -20.0), (3.0, -20.0)][..]
        };
        let mut app = world_from_profile(profile);
        app.world_mut()
            .run_system_once(move |world: WorldCollision| {
                let mut player = Player {
                    pos: Vec3::Y * 0.3,
                    vel_y: -8.0,
                    ..Default::default()
                };
                let mut time = Time::default();
                time.advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
                for _ in 0..12 {
                    step(
                        &mut player,
                        &time,
                        &world,
                        &player_capsule(),
                        false,
                        Vec3::ZERO,
                        7.0,
                        false,
                        false,
                        None,
                        None,
                        2.5,
                    );
                }
                if floor {
                    assert!(
                        player.pos.y >= 0.0 && player.pos.y < 0.05,
                        "{:?}",
                        player.pos
                    );
                } else {
                    assert!(player.pos.y < -1.0, "{:?}", player.pos);
                }
            })
            .unwrap();
    }
}

#[test]
fn legal_downstairs_checks_slide_and_lowering_not_a_diagonal_chord() {
    let mut app = world_from_profile(&[(-3.0, 0.0), (0.0, 0.0)]);
    app.world_mut()
        .run_system_once(|world: WorldCollision| {
            let before = Vec3::new(-0.1, 0.02, 0.0);
            let over_edge = Vec3::new(0.1, 0.02, 0.0);
            let lowered = Vec3::new(0.1, -0.15, 0.0);
            // The chord cuts the old tread; neither actual leg crosses solid.
            assert!(crossing(&world, before, lowered).is_some());
            assert!(crossing(&world, before, over_edge).is_none());
            assert!(crossing(&world, over_edge, lowered).is_none());
        })
        .unwrap();
}

#[test]
fn ordinary_support_identity_is_independent_of_asset_provenance() {
    use wrathplus_world::collision::ColliderSource;

    for source in [
        ColliderSource::Terrain,
        ColliderSource::Wmo,
        ColliderSource::M2,
        ColliderSource::Runtime,
    ] {
        let mut app = world_from_profile(&[(-3.0, 0.0), (3.0, 0.0)]);
        let floor = app
            .world_mut()
            .query_filtered::<Entity, With<Collider>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(floor).insert(source);
        // An unrelated entity must never be reported as support (and hence a ride candidate).
        let unrelated = app.world_mut().spawn_empty().id();
        app.world_mut()
            .run_system_once(move |world: WorldCollision| {
                let mut player = Player {
                    pos: Vec3::Y * SKIN_WIDTH,
                    ..Default::default()
                };
                let mut time = Time::default();
                time.advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
                let out = step(
                    &mut player,
                    &time,
                    &world,
                    &player_capsule(),
                    false,
                    Vec3::ZERO,
                    7.0,
                    false,
                    false,
                    None,
                    None,
                    2.5,
                );
                assert!(out.grounded);
                assert_eq!(out.ground, Some(floor), "source={source:?}");
                assert_ne!(out.ground, Some(unrelated));
                assert!(!player.wedged);
            })
            .unwrap();
    }
}

#[test]
fn support_stabilization_requires_a_frontface_inside_the_skin_band() {
    for (profile, depth, expected) in [
        (&[(-3.0, 0.0), (3.0, 0.0)][..], 0.003, true),
        (&[(-3.0, 0.0), (3.0, 0.0)][..], 0.322, false),
        (&[(-3.0, 0.0), (3.0, 0.0)][..], -0.003, false),
        (&[(3.0, 0.0), (-3.0, 0.0)][..], 0.003, false),
        (&[(-3.0, -20.0), (3.0, -20.0)][..], 0.003, false),
        (&[(-3.0, 0.0), (-1.0, 0.0)][..], 0.003, false),
    ] {
        world_from_profile(profile)
            .world_mut()
            .run_system_once(move |world: WorldCollision| {
                let result = support(&world, Vec3::NEG_Y * depth);
                assert_eq!(result.is_some(), expected, "depth={depth}");
                if let Some(result) = result {
                    assert!(result.feet.y.abs() < 1.0e-5);
                }
            })
            .unwrap();
    }
}
