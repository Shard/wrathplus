use super::gravity_step;

#[test]
fn gravity_telescopes_to_g_t_squared_at_any_frame_rate() {
    for &g in &[0.5_f32, 1.0, 1.5, 2.0, 5.0, -1.0] {
        for &dt in &[1.0 / 144.0_f32, 1.0 / 60.0, 1.0 / 30.0, 1.0 / 15.0] {
            let steps = (1.0 / dt).round() as usize; // exactly one second of frames
            let (mut z, mut age) = (0.0_f32, 0.0_f32);
            for _ in 0..steps {
                z += gravity_step(g, age, dt);
                age += dt;
            }
            let t = steps as f32 * dt;
            assert!(
                (z - g * t * t).abs() < 1e-4,
                "g {g} at dt {dt}: got {z}, want {}",
                g * t * t
            );
        }
    }
    // A positive gravity rises.
    assert!(gravity_step(2.0, 0.0, 0.016) > 0.0);
    assert!(gravity_step(-1.0, 0.0, 0.016) < 0.0);
}

/// The Frost Trap's four low streamers (authored `gravity`, `edgeLifetime`) rise `g·L²` over an
/// edge's life from a node at model z 0.129, clearing the crown's own geometry at z 0.637.
#[test]
fn frost_trap_low_rig_rises_into_a_tuft_over_the_crown() {
    for (g, life, want) in [
        (0.5_f32, 1.1_f32, 0.605_f32),
        (1.0, 1.0, 1.000),
        (1.5, 0.9, 1.215),
        (2.0, 0.8, 1.280),
    ] {
        let dt = 1.0 / 60.0;
        let steps = (life / dt).round() as usize;
        let (mut z, mut age) = (0.0_f32, 0.0_f32);
        for _ in 0..steps {
            z += gravity_step(g, age, dt);
            age += dt;
        }
        assert!((z - want).abs() < 0.01, "g {g} life {life}: {z} vs {want}");
        assert!(0.129 + z > 0.637, "clears the crown's own geometry");
    }
}
