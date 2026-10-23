use super::*;

/// One sequence record with the given bones, the rest neutral.
fn sequence(duration: f32, bones: Vec<wrathplus_formats::BoneKeys>) -> ModelAnimation {
    ModelAnimation {
        anim_id: 0,
        seq_index: 0,
        start_ms: 0,
        end_ms: (duration * 1000.0) as u32,
        duration,
        looping: true,
        move_speed: 0.0,
        blend_time: 0.0,
        bounds_center: [0.0; 3],
        bounds_radius: 0.0,
        bounds_min: [0.0; 3],
        bounds_max: [0.0; 3],
        frequency: 0,
        min_replay: 0,
        max_replay: 0,
        bones,
        events: Vec::new(),
    }
}

/// A clip with no curves still carries its sequence's duration, or its clock never advances.
#[test]
fn a_boneless_sequence_still_yields_a_clock_clip() {
    let skeleton = ModelSkeleton {
        joints: Vec::new(),
        spine_bone: None,
        head_bone: None,
    };
    let (clip, _pose, poses_bones) = build_animation_clip(&sequence(1.333, Vec::new()), &skeleton);
    assert!(!poses_bones, "no bone track ⇒ nothing to skin to");
    assert!(
        (clip.duration() - 1.333).abs() < 1e-4,
        "the clock's period is the sequence's own band, not 0: {}",
        clip.duration()
    );
    // The flag gates the rig: a clock-only clip must never spawn one.
    let moving = wrathplus_formats::BoneKeys {
        bone: 0,
        translation: vec![(0.0, [0.0, 0.0, 0.0]), (0.5, [0.0, 0.0, 1.0])],
        rotation: Vec::new(),
        scale: Vec::new(),
    };
    let (_, _, poses_bones) = build_animation_clip(&sequence(1.0, vec![moving]), &skeleton);
    assert!(poses_bones, "a keyed track ⇒ a real pose");
}

/// The eye-blink shape from a real eyelid track: open, a fast ramp shut, open, repeating.
#[test]
fn global_seq_channel_samples_and_wraps() {
    let ch = GlobalSeqChannel {
        period: 6.633,
        keys: vec![
            (0.0, Vec3::ZERO),
            (0.033, Vec3::ONE),
            (0.100, Vec3::ONE),
            (0.133, Vec3::ZERO),
        ],
    };
    assert!(
        ch.sample(0.0).abs_diff_eq(Vec3::ZERO, 1e-4),
        "loop start = open"
    );
    assert!(
        ch.sample(0.06).abs_diff_eq(Vec3::ONE, 1e-4),
        "shut during the blink"
    );
    assert!(
        ch.sample(3.0).abs_diff_eq(Vec3::ZERO, 1e-4),
        "held open for the rest of the loop"
    );
    assert!(
        ch.sample(0.0165).abs_diff_eq(Vec3::splat(0.5), 1e-2),
        "linear ramp"
    );
    assert!(
        ch.sample(6.633 + 0.06).abs_diff_eq(Vec3::ONE, 1e-4),
        "wraps modulo period"
    );
}

#[test]
fn rotation_quat_matches_the_coordinate_map() {
    let r = wow_to_bevy_quat();
    for axis in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        let by_quat = r * Vec3::from_array(axis);
        let by_map = wow_to_bevy(axis);
        assert!(
            by_quat.abs_diff_eq(by_map, 1e-5),
            "r·{axis:?} = {by_quat:?} should equal wow_to_bevy = {by_map:?}"
        );
    }
}

/// Pins the conjugation and the `[x, y, z, w]` key order.
#[test]
fn wow_up_yaw_conjugates_to_bevy_up_yaw() {
    let r = wow_to_bevy_quat();
    let theta = std::f32::consts::FRAC_PI_3; // 60°, an asymmetric angle
    let (s, c) = (theta / 2.0).sin_cos();
    // A WoW quat for a yaw about +Z: (x,y,z,w) = (0,0,sin,cos).
    let q_wow = Quat::from_xyzw(0.0, 0.0, s, c);
    let q_bevy = r * q_wow * r.inverse();
    let expected = Quat::from_axis_angle(Vec3::Y, theta);
    // Quats equal up to sign: |dot| ≈ 1.
    assert!(
        q_bevy.dot(expected).abs() > 0.9999,
        "WoW +Z yaw should map to Bevy +Y yaw: got {q_bevy:?}, want {expected:?}"
    );
}

#[test]
fn identity_rotation_conjugates_to_identity() {
    let r = wow_to_bevy_quat();
    let id = r * Quat::IDENTITY * r.inverse();
    assert!(id.dot(Quat::IDENTITY).abs() > 0.9999, "got {id:?}");
}
