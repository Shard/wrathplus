use super::*;

#[test]
fn all_plus_switches_are_independent_and_booths_never_opt_in() {
    for bits in 0..8 {
        let policy = ModernLighting {
            shadows: true,
            fog: true,
            bloom: bits & 1 != 0,
            tone_mapping: bits & 2 != 0,
            colour_grading: bits & 4 != 0,
        };
        assert_eq!(postprocess_key(policy, false), [false; 3]);
        assert_eq!(
            postprocess_key(policy, true),
            [policy.bloom, policy.tone_mapping, policy.colour_grading]
        );
    }
}

// CPU mirror of the production Plus bloom extraction, not a claim about live colour range.
fn bloom(blur: Vec3, zone: f32) -> Vec3 {
    let brightness = blur.max_element();
    let knee = (brightness - 0.35 + 0.15).clamp(0.0, 0.30);
    let soft = knee * knee / 0.60;
    let contribution = (brightness - 0.35).max(soft) / brightness.max(0.0001);
    blur * contribution * (0.25 + 0.15 * zone.clamp(0.0, 1.0))
}

#[test]
fn plus_bloom_retains_highlight_hue_and_dark_surfaces_stay_dark() {
    assert_eq!(bloom(Vec3::ZERO, 1.0), Vec3::ZERO);
    assert_eq!(bloom(Vec3::splat(0.19), 1.0), Vec3::ZERO);
    let highlight = Vec3::new(0.1, 0.4, 0.8);
    let glow = bloom(highlight, 0.0);
    assert!(glow.z > 0.0);
    assert!((glow.x / glow.z - highlight.x / highlight.z).abs() < 1e-6);
    assert!((glow.y / glow.z - highlight.y / highlight.z).abs() < 1e-6);
    assert_eq!(bloom(highlight, -100.0), glow);
    assert_eq!(bloom(highlight, 100.0), bloom(highlight, 1.0));
}

#[test]
fn plus_bloom_soft_knee_is_continuous_monotonic_and_bounded() {
    let mut previous = 0.0;
    for i in 0..=1000 {
        let input = i as f32 / 1000.0;
        let output = bloom(Vec3::splat(input), 1.0).x;
        assert!(output.is_finite() && output >= previous && output <= input * 0.4 + 1e-6);
        assert!(output - previous < 0.001);
        previous = output;
    }
    let src = include_str!("../shaders/ffx_plus.wgsl");
    assert!(src.contains("let knee = clamp(brightness - 0.35 + 0.15, 0.0, 0.30);"));
    assert!(src.contains("colour += blur * contribution"));
}

#[test]
fn classic_postprocess_key_is_the_original_variant() {
    assert_eq!(postprocess_key(ModernLighting::default(), true), [false; 3]);
}
