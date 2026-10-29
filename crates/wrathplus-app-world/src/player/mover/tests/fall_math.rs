//! Analytic fall integration, independent of collision fixtures.

use super::*;

#[test]
fn the_fall_step_is_frame_rate_independent() {
    let total = 0.5_f32;
    let drop = |steps: u32| {
        let dt = total / steps as f32;
        let (mut v, mut y) = (JUMP_SPEED, 0.0_f32);
        for _ in 0..steps {
            let (end, mean) = fall_step(v, dt, TERMINAL_VELOCITY);
            y += mean * dt;
            v = end;
        }
        y
    };
    let analytic = JUMP_SPEED * total - 0.5 * GRAVITY * total * total;
    for steps in [1_u32, 2, 8, 30, 120] {
        let got = drop(steps);
        assert!(
            (got - analytic).abs() < 1.0e-4,
            "{steps} steps must reproduce the closed form {analytic}, got {got}"
        );
    }
}

/// The jump apex is `v₀²/2g` = 1.640 yd at any frame rate.
#[test]
fn the_jump_apex_is_the_analytic_one_at_any_frame_rate() {
    let apex = JUMP_SPEED * JUMP_SPEED / (2.0 * GRAVITY);
    for fps in [30.0_f32, 60.0, 144.0] {
        let dt = 1.0 / fps;
        let (mut v, mut y, mut high) = (JUMP_SPEED, 0.0_f32, 0.0_f32);
        while v > -1.0 {
            let (end, mean) = fall_step(v, dt, TERMINAL_VELOCITY);
            y += mean * dt;
            high = high.max(y);
            v = end;
        }
        // The apex falls between frames: a step's worth of sampling, not drift.
        assert!(
            (high - apex).abs() < 0.5 * GRAVITY * dt * dt + 1.0e-4,
            "at {fps} fps the apex should be ~{apex}, got {high}"
        );
    }
}

/// A step reaching terminal partway is two regimes; below terminal the clamp changes nothing.
#[test]
fn the_terminal_clamp_is_exact_on_the_step_that_reaches_it() {
    let dt = 1.0 / 60.0;
    let (end, mean) = fall_step(0.0, dt, TERMINAL_VELOCITY);
    assert!((end - -GRAVITY * dt).abs() < 1.0e-6);
    assert!((mean - -0.5 * GRAVITY * dt).abs() < 1.0e-6);
    // Straddling terminal: reach it at `t_c`, coast the rest.
    let v0 = -TERMINAL_VELOCITY + 0.5 * GRAVITY * dt;
    let (end, mean) = fall_step(v0, dt, TERMINAL_VELOCITY);
    assert_eq!(end, -TERMINAL_VELOCITY, "the step ends at terminal");
    let t_c = (v0 + TERMINAL_VELOCITY) / GRAVITY;
    let want = (0.5 * (v0 - TERMINAL_VELOCITY) * t_c - TERMINAL_VELOCITY * (dt - t_c)) / dt;
    assert!(
        (mean - want).abs() < 1.0e-6,
        "piecewise, not a clamped mean"
    );
    let (end, mean) = fall_step(-TERMINAL_VELOCITY, dt, TERMINAL_VELOCITY);
    assert_eq!((end, mean), (-TERMINAL_VELOCITY, -TERMINAL_VELOCITY));
}
