//! Installed-geometry replay of the wedge-to-fall observation, not a synthetic floor fixture.

use super::super::arc::PlayerArc;
use super::*;
use wrathplus_assets::coords::bevy_to_wow;
use wrathplus_world::collision::WorldCollision;

/// Downstream pose from t=3518.777: already 0.322 yd buried, not a valid landing seed.
const ANCHOR: [f32; 3] = [-9607.03, -246.17, 56.936];
const FRAME_DT: f32 = 0.0113;
const FACING: f32 = 0.2477;

fn print_support(world: &WorldCollision, names: &Query<&Name>, capsule: &Collider, feet: Vec3) {
    let source = |entity| names.get(entity).map(|n| n.as_str()).unwrap_or("?");
    let origin = feet + Vec3::Y * 0.25;
    match world.ray_body(origin, Dir3::NEG_Y, 8.0) {
        Some(hit) => println!(
            "initial ray: source={} distance={} floor_wow_z={} normal_bevy={:?}",
            source(hit.entity),
            hit.distance,
            origin.y - hit.distance,
            hit.normal,
        ),
        None => println!("initial ray: miss within 8 yd from feet + 0.25"),
    }
    let center = feet + Vec3::Y * (CAPSULE_HEIGHT * 0.5);
    for (label, reach) in [
        ("ground classify", super::super::GROUND_PROBE),
        ("wedge retention / landing", super::super::LAND_PROBE),
        ("deep support diagnostic", 8.0),
    ] {
        match world.cast_mover(
            capsule,
            center,
            Vec3::NEG_Y * reach,
            super::super::SKIN_WIDTH,
        ) {
            Some(hit) => println!(
                "initial capsule {label}: source={} reach={reach} distance={} normal_bevy={:?}",
                source(hit.entity),
                hit.distance,
                hit.normal1,
            ),
            None => println!("initial capsule {label}: miss within {reach} yd"),
        }
    }
}

/// Hold the observed StrafeLeft for 200 frames on the installed Azeroth collision.
/// No spawn lift, floor seating, synthetic support or repair is applied to the seed.
#[test]
fn already_buried_wedge_has_no_recovery_evidence() {
    let Some(mut app) = scene("Azeroth", &[ANCHOR]) else {
        return;
    };
    app.world_mut()
        .run_system_once(
            |query: SpatialQuery, colliders: Query<(&Name, &ColliderAabb, &Position)>| {
                let feet = wow_to_bevy(ANCHOR);
                println!("fixture colliders={}", colliders.iter().count());
                for (name, aabb, pos) in &colliders {
                    if aabb.min.x <= feet.x
                        && aabb.max.x >= feet.x
                        && aabb.min.z <= feet.z
                        && aabb.max.z >= feet.z
                    {
                        println!("fixture column source={name} aabb={aabb:?} position={pos:?}");
                    }
                }
                for lift in [0.25, 2.0] {
                    let hit = query.cast_ray(
                        feet + Vec3::Y * lift,
                        Dir3::NEG_Y,
                        100.0,
                        true,
                        &SpatialQueryFilter::default(),
                    );
                    println!(
                        "fixture raw ray lift={lift}: {hit:?}; floor_z={:?}",
                        hit.map(|h| feet.y + lift - h.distance)
                    );
                }
            },
        )
        .unwrap();
    app.world_mut()
        .run_system_once(move |world: WorldCollision, names: Query<&Name>| {
            let capsule = Collider::capsule(CAPSULE_RADIUS, CAPSULE_HEIGHT - 2.0 * CAPSULE_RADIUS);
            // controller::control uses face_rot * X for right, negated for StrafeLeft.
            // movement_net sends face_yaw unchanged; there is no wire-to-Bevy yaw offset.
            let dir = -(Quat::from_rotation_y(FACING) * Vec3::X);
            let wow_dir = bevy_to_wow(dir);
            assert!((dir - Vec3::new(-0.9695, 0.0, 0.2452)).length() < 0.001);
            assert!(wow_dir[0] < 0.0 && wow_dir[1] > 0.0);
            let mut player = Player {
                active: true,
                pos: wow_to_bevy(ANCHOR),
                face_yaw: FACING,
                wedged: true,
                vel_y: 0.0,
                settling: false,
                airborne_since: None,
                airborne_prev: false,
                steep_support: false,
                ..Default::default()
            };
            println!("seed wow={ANCHOR:?} strafe_left bevy={dir:?} wow={wow_dir:?}");
            print_support(&world, &names, &capsule, player.pos);
            let mut time = Time::<()>::default();
            time.advance_by(std::time::Duration::from_secs_f32(3518.777));
            let mut lowest = ANCHOR[2];
            let mut first_fall = None;
            for frame in 1..=200 {
                let before = player.pos;
                time.advance_by(std::time::Duration::from_secs_f32(FRAME_DT));
                let outcome = mover::step(
                    &mut player,
                    &time,
                    &world,
                    &capsule,
                    true,
                    dir,
                    RUN,
                    false,
                    false,
                    None,
                    None,
                    AIR_NUDGE_SPEED,
                );
                // The controller's post-mover arc bookkeeping changes next frame's probe reach.
                player.advance_airborne_arc(
                    !outcome.held && (!outcome.grounded || outcome.jumped),
                    outcome.jumped,
                    time.elapsed_secs(),
                    before.y,
                );
                let here = bevy_to_wow(player.pos);
                lowest = lowest.min(here[2]);
                if here[2] < ANCHOR[2] - FALL_LIMIT && first_fall.is_none() {
                    first_fall = Some((frame, here));
                }
                println!(
                    "frame={frame} wow={here:?} down={} grounded={} wedged={} vel_y={} airborne_since={:?}",
                    before.y - player.pos.y,
                    outcome.grounded,
                    player.wedged,
                    player.vel_y,
                    player.airborne_since,
                );
                assert!(player.pos.is_finite(), "non-finite position at frame {frame}");
                assert!(player.pos.y <= before.y + 1.0e-4, "buried seed must not lift");
            }
            // This seed is downstream of the breach, outside both skin and frontface evidence.
            // It must not be teleported up; continued falling is the expected negative control.
            assert!(first_fall.is_some(), "buried seed unexpectedly recovered");
            assert!(lowest < ANCHOR[2] - FALL_LIMIT);
            assert!(player.pos.y < ANCHOR[2] - FALL_LIMIT);
        })
        .unwrap();
}

/// Last transmitted above-surface pose before t=3511.021's first unprobed descent.
/// The trace rounds network positions to 0.01 yd; this is an approach reconstruction, not a
/// bit-exact pre-breach state. Forward (flags 0x1), not the later StrafeLeft (0x4).
#[test]
fn last_supported_forward_approach_stays_above_installed_terrain() {
    const START: [f32; 3] = [-9606.32, -246.10, 57.27];
    const YAW: f32 = 4.477;
    let Some(mut app) = scene("Azeroth", &[START]) else {
        return;
    };
    app.world_mut()
        .run_system_once(|world: WorldCollision, names: Query<&Name>| {
            let capsule = Collider::capsule(CAPSULE_RADIUS, CAPSULE_HEIGHT - 2.0 * CAPSULE_RADIUS);
            let dir = Quat::from_rotation_y(YAW) * Vec3::NEG_Z;
            let mut player = Player {
                active: true,
                pos: wow_to_bevy(START),
                face_yaw: YAW,
                horiz_vel: dir * RUN,
                ..Default::default()
            };
            let floor = world
                .ray_body(player.pos + Vec3::Y * 0.25, Dir3::NEG_Y, 0.5)
                .expect("logged approach has installed support");
            assert_eq!(names.get(floor.entity).unwrap().as_str(), "terrain 32_50");
            assert!(floor.normal.y >= super::super::GROUND_COS);
            let clearance = floor.distance - 0.25;
            assert!(clearance > 0.0 && clearance < 0.02, "{clearance}");
            print_support(&world, &names, &capsule, player.pos);
            let mut time = Time::default();
            time.advance_by(std::time::Duration::from_secs_f32(3510.876));
            for frame in 1..=200 {
                let before = player.pos;
                time.advance_by(std::time::Duration::from_secs_f32(FRAME_DT));
                let out = mover::step(
                    &mut player,
                    &time,
                    &world,
                    &capsule,
                    true,
                    dir,
                    RUN,
                    false,
                    false,
                    None,
                    None,
                    AIR_NUDGE_SPEED,
                );
                player.advance_airborne_arc(
                    !out.held && (!out.grounded || out.jumped),
                    out.jumped,
                    time.elapsed_secs(),
                    before.y,
                );
                let origin = player.pos + Vec3::Y * 0.25;
                let hit = world
                    .ray_body(origin, Dir3::NEG_Y, 0.5)
                    .expect("approach must retain the terrain column");
                let clearance = hit.distance - 0.25;
                assert!(player.pos.is_finite());
                assert!(
                    clearance >= -1.0e-4,
                    "frame={frame} feet={:?} clearance={clearance}",
                    bevy_to_wow(player.pos)
                );
                assert!(out.grounded && !player.wedged, "frame={frame}");
            }
            assert!(
                player.pos.distance(wow_to_bevy(START)) > 10.0,
                "a stationary replay is not an approach reconstruction"
            );
            println!(
                "supported approach: 200 forward frames; final={:?}",
                bevy_to_wow(player.pos)
            );
        })
        .unwrap();
}
