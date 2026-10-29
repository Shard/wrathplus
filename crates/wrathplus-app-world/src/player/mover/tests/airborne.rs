use super::*;

/// An Elwynn hillside out of flat ground, a 55° face (`ny = +0.574`, read at WoW
/// `(-9236.8, -341.9, 101.6)`), too steep to stand on anywhere.
fn world_with_steep_hillside() -> App {
    const P: [(f32, f32); 3] = [(-3.0, 0.0), (0.0, 0.0), (2.5, 3.570)];
    world_from_profile(&P)
}

/// One airborne frame's readings: `(feet height, vertical velocity, fraction of the descent
/// gravity intended that the frame actually achieved)`.
type AirRow = (f32, f32, f32);

/// Jump from `start_feet`, flying the bare arc through [`airborne_step`] at a run along `dir`.
fn jump_into(mut world: App, start_feet: Vec3, dir: Vec3, frames: usize) -> Vec<AirRow> {
    world
        .world_mut()
        .run_system_once(move |world: wrathplus_world::collision::WorldCollision| {
            let capsule = player_capsule();
            let dt = std::time::Duration::from_secs_f32(TRAVEL_60FPS / 7.0);
            let secs = dt.as_secs_f32();
            let mut center = start_feet + Vec3::Y * (CAPSULE_HEIGHT * 0.5);
            let mut vel_y = JUMP_SPEED;
            (0..frames)
                .map(|_| {
                    vel_y = (vel_y - GRAVITY * secs).max(-TERMINAL_VELOCITY);
                    let before = center.y;
                    center =
                        airborne_step(&world, &capsule, center, dir * 7.0 + Vec3::Y * vel_y, dt);
                    let intent = vel_y * secs;
                    let achieved = center.y - before;
                    // Only a descent has a stall to measure; a rise reports 1.0.
                    let frac = if intent < 0.0 { achieved / intent } else { 1.0 };
                    (center.y - CAPSULE_HEIGHT * 0.5, vel_y, frac)
                })
                .collect::<Vec<_>>()
        })
        .unwrap()
}

#[test]
fn a_jump_into_a_steep_hillside_banks_no_height() {
    let arc = jump_into(
        world_with_steep_hillside(),
        Vec3::new(-0.6, 0.0, 0.0),
        Vec3::X,
        150,
    );
    let peak = arc.iter().map(|r| r.0).fold(f32::MIN, f32::max);
    let end = arc.last().unwrap().0;
    assert!(
        peak > 1.0,
        "the jump must actually leave the ground: {peak:.3}"
    );
    assert!(
        end < 0.05,
        "the arc must end back at the foot of the slope, not banked part-way up it: \
         ended at {end:.3} (peak {peak:.3})"
    );
    // Nor may the descent stall into a wedge rest, which would stand the body up part-way.
    let (mut run, mut worst_run) = (0u8, 0u8);
    for r in arc.iter().filter(|r| r.0 > 0.2) {
        run = if r.1 < -WEDGE_MIN_FALL && r.2 < WEDGE_STALL_RATIO {
            run + 1
        } else {
            0
        };
        worst_run = worst_run.max(run);
    }
    assert!(
        worst_run < WEDGE_STILL_FRAMES,
        "the descent must never stall long enough to read as a wedge rest: \
         {worst_run} consecutive stalled frames"
    );
}

#[test]
fn a_steep_face_never_cancels_a_fall() {
    // A plane clip turns a run into the hill into lift (`v'.y - v.y = -(v·n)·n.y`), which all
    // but cancels a fall against a 55° face.
    let arc = jump_into(
        world_with_steep_hillside(),
        Vec3::new(-0.6, 0.0, 0.0),
        Vec3::X,
        150,
    );
    // Only frames clear of the flat ground, which the bare arc has no ground probe to stand on.
    let worst = arc
        .iter()
        .filter(|r| r.1 < -WEDGE_MIN_FALL && r.0 > 0.2)
        .map(|r| r.2)
        .fold(f32::MAX, f32::min);
    assert!(
        worst > 0.98,
        "a falling frame against the face keeps ALL of its descent — the reference's response \
         is horizontal only: worst was {worst:.2} of intent"
    );
}

#[test]
fn walkable_and_overhanging_faces_are_untouched() {
    let push = Vec3::new(7.0, 0.0, 0.0);
    assert!(steep_contact_shear(face(40.0), push).is_none());
    // An overhang keeps the plain ceiling clip.
    assert!(steep_contact_shear(Vec3::new(-0.5, -0.7, 0.0).normalize(), push).is_none());
    assert!(steep_contact_shear(face(60.0), -push).is_none());
}

#[test]
fn a_vertical_wall_takes_the_whole_push_and_no_more() {
    let v = Vec3::new(7.0, -4.0, 0.0);
    let out = steep_contact_shear(face(90.0), v).expect("must strip");
    assert!(out.x.abs() < 1e-6 && (out.y - v.y).abs() < 1e-6);
}
