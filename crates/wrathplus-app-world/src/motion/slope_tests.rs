//! A walker on a slope stays level: a grounded spline's sample takes yaw alone, however steeply
//! its path climbs or falls (the Valley of Trials boars, 2026-10-24). Pitch is a flying spline's
//! and a swimmer's.

use std::time::{Duration, Instant};

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use super::spline::{sample_splines, Spline};

#[test]
fn a_grounded_spline_down_a_slope_keeps_a_level_root() {
    let mut world = World::new();
    world.init_resource::<Time>();
    // Down 3.4 yd over 16 (the logged boar path), a quarter of the way along.
    let unit = world
        .spawn((
            Transform::default(),
            Spline {
                deck: None,
                points: vec![[-516.6, -4279.1, 41.5], [-521.5, -4295.3, 38.1]],
                start: Instant::now() - Duration::from_millis(1720),
                duration: Duration::from_millis(6880),
                id: 7,
                grounded: true,
                interpolation: wrathplus_protocol::SplineInterpolation::Linear,
                run_mode: Some(false),
            },
        ))
        .id();
    world
        .run_system_once(sample_splines)
        .expect("the sampler runs");
    let (_, pitch, roll) = world
        .get::<Transform>(unit)
        .unwrap()
        .rotation
        .to_euler(EulerRot::YXZ);
    assert!(
        pitch.abs() < 1e-6 && roll.abs() < 1e-6,
        "pitch {pitch}, roll {roll}"
    );
}
