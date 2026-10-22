//! The Lamppost's glow card rides a spherical billboard bone (flag 0x08); its post does not.

use wrathplus_formats::{load_m2_mesh, load_wrath_render_model, open_chain, BillboardKind};

#[test]
fn lamppost_glow_is_spherical_billboard() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = open_chain(&data).expect("open patch chain");
    let path = "World\\Azeroth\\Elwynn\\PassiveDoodads\\Lamppost\\Lamppost.m2";
    let subs = load_m2_mesh(&mut chain, path).expect("load Lamppost");
    let model = load_wrath_render_model(&mut chain, path).expect("render model");

    let glow = subs
        .iter()
        .find(|s| s.welded_billboard && s.additive)
        .expect("lamppost has an additive billboard glow card");
    assert_eq!(glow.positions.len(), 4, "the glow is one quad");
    let bone = glow.joints[0][0];
    assert_eq!(
        model.skeleton.bones[usize::from(bone)].billboard,
        Some(BillboardKind::Spherical),
        "the glow card rides a spherical billboard bone (flag 0x08)"
    );
    assert!(
        subs.iter().any(|s| !s.welded_billboard),
        "the lamppost post batch is not billboarded"
    );

    // The glow's pulse: its bone's scale track on a global sequence.
    let pulse = model
        .global_sequence_bones
        .iter()
        .find(|g| g.bone == bone)
        .and_then(|g| g.scale.as_ref())
        .expect("the glow's billboard bone has a global-sequence scale pulse");
    assert_eq!(pulse.period_ms, 1333, "global sequence loop length (ms)");
    let (lo, hi) = pulse
        .keys
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), (_, v)| {
            (lo.min(v[0]), hi.max(v[0]))
        });
    assert!(
        (0.8..0.95).contains(&lo) && (1.0..1.1).contains(&hi),
        "scale pulse breathes within ~0.86..1.04, got {lo}..{hi}"
    );
}
