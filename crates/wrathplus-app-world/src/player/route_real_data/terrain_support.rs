//! Contact-tolerance landing on installed geometry near the first observed breach.
use super::super::arc::PlayerArc;

use super::*;
use wrathplus_world::collision::WorldCollision;

#[test]
fn shallow_contact_support_is_stabilized_on_installed_terrain() {
    const START: [f32; 3] = [-9606.32, -246.10, 57.255];
    let Some(mut app) = scene("Azeroth", &[START]) else {
        return;
    };
    app.world_mut()
        .run_system_once(|world: WorldCollision, names: Query<&Name>| {
            let capsule = Collider::capsule(CAPSULE_RADIUS, CAPSULE_HEIGHT - 2.0 * CAPSULE_RADIUS);
            let mut player = Player {
                pos: wow_to_bevy(START),
                active: true,
                ..Default::default()
            };
            let origin = player.pos + Vec3::Y * 0.02;
            let floor = world
                .ray_mover(origin, Dir3::NEG_Y, 0.04)
                .expect("installed floor");
            assert_eq!(names.get(floor.entity).unwrap().as_str(), "terrain 32_50");
            let depth = 0.02 - floor.distance;
            assert!(depth > 0.002 && depth < 0.004, "depth={depth}");
            let contact = world
                .cast_mover(
                    &capsule,
                    player.pos + Vec3::Y * (CAPSULE_HEIGHT * 0.5),
                    Vec3::NEG_Y * super::super::LAND_PROBE,
                    super::super::SKIN_WIDTH,
                )
                .expect("production landing contact tolerates shallow burial");
            assert_eq!(contact.entity, floor.entity);
            assert!(contact.normal1.y >= super::super::GROUND_COS);
            assert_eq!(contact.distance, 0.0);
            let mut time = Time::default();
            let dir = Quat::from_rotation_y(4.477) * Vec3::NEG_Z;
            for frame in 0..200 {
                time.advance_by(std::time::Duration::from_secs_f32(0.0113));
                let before = player.pos;
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
                let hit = world
                    .ray_mover(player.pos + Vec3::Y * 0.02, Dir3::NEG_Y, 0.1)
                    .expect("terrain support persists");
                assert!(
                    hit.distance >= 0.02 - 1.0e-4,
                    "frame={frame} clearance={}",
                    hit.distance - 0.02
                );
                assert!(out.grounded && !player.wedged, "frame={frame}");
            }
            assert!(player.pos.distance(wow_to_bevy(START)) > 10.0);
        })
        .unwrap();
}
