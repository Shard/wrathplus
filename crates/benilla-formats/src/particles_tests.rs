use super::*;

#[test]
fn bit_one_is_the_unlit_flag_and_mod_blends_never_light() {
    // The Elwynn waterfall: bit 0x1 clear, blend 2 (alpha), so the scene lights it.
    assert!(
        lit_of(0x0002, 2),
        "an emitter that CLEARS 0x1 takes the light"
    );
    // The Orgrimmar bonfire's flame (0x29) and smoke (0x21) set 0x1, so both are unlit.
    assert!(!lit_of(0x0029, 4), "0x1 set: unlit, whatever the blend");
    assert!(!lit_of(0x0021, 2));
    for blend in 0u8..=4 {
        assert!(lit_of(0x0000, blend), "blend {blend} lights");
    }
    assert!(!lit_of(0x0000, 5), "Mod never lights");
    assert!(!lit_of(0x0000, 6), "Mod2x never lights");
    // A byte past the 7-entry table folds to `Opaque`, which lights.
    assert!(lit_of(0x0000, 7));
}

#[test]
fn the_waterfall_emitter_is_lit() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let bytes = chain
        .read_file("World\\Azeroth\\Elwynn\\PassiveDoodads\\Waterfall\\ElwynnTallWaterfall01.m2")
        .expect("read ElwynnTallWaterfall01.m2");
    let defs = parse_m2_particle_emitters(&bytes).expect("parse emitters");
    assert_eq!(defs.len(), 1);
    assert_eq!(
        defs[0].flags & 0xffff,
        0x0002,
        "authored flag word, less the 12340 0x20000 bit"
    );
    assert_eq!(defs[0].blend, ParticleBlend::Alpha);
    assert!(defs[0].lit, "the spray sheet is shaded by the world");
}

/// Straight segments of 1 and 3 yd (control points at thirds, so each cubic is linear) put
/// the boundary at t = 0.25, not 0.5.
#[test]
fn spline_chain_is_arc_length_parameterized() {
    let x = |v: f32| [v, 0.0, 0.0];
    let s = SplineData::new(vec![
        x(0.0),
        x(1.0 / 3.0),
        x(2.0 / 3.0),
        x(1.0), // segment 0: 1 yd
        x(2.0),
        x(3.0),
        x(4.0), // segment 1: 3 yd
    ])
    .expect("3K+1 chain");
    assert!((s.eval(0.25)[0] - 1.0).abs() < 1e-4, "boundary at arc 1/4");
    assert!((s.eval(0.625)[0] - 2.5).abs() < 1e-4, "mid of segment 1");
    assert_eq!(s.eval(-0.5), [0.0, 0.0, 0.0], "clamp to first point");
    assert_eq!(s.eval(1.5), [4.0, 0.0, 0.0], "clamp to last point");
    let tan = s.tangent(0.1);
    assert!(tan[0] > 0.0 && tan[1] == 0.0 && tan[2] == 0.0, "+X tangent");
    assert!(SplineData::new(vec![x(0.0)]).is_none(), "below one segment");
}

/// `BloodSpurt.m2`, the melee impact flash: emitters 1 to 3 are keyed bursts.
#[test]
fn real_blood_spurt_emitters_are_keyed_bursts() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let bytes = chain
        .read_file("Particles\\BloodSpurts\\BloodSpurt.m2")
        .expect("read BloodSpurt.m2");
    let defs = parse_m2_particle_emitters(&bytes).expect("parse emitters");
    assert_eq!(defs.len(), 4);
    // Emitter 0, the red spray: 100/s at t = 0.
    assert_eq!(defs[0].timing.rate(None, 0.0, 0.0), 100.0);
    // Emitter 3, the starflash: first key 0, 20/s within 67..100 ms, 0 again by 133 ms.
    let flash = &defs[3];
    assert!(flash
        .texture
        .as_deref()
        .is_some_and(|t| t.to_ascii_uppercase().contains("STARFLASH")));
    assert_eq!(flash.blend, ParticleBlend::Add);
    assert_eq!(flash.timing.rate(None, 0.0, 0.0), 0.0, "silent at t=0");
    assert_eq!(flash.timing.peak_rate(), 20.0);
    assert_eq!(flash.timing.rate(None, 0.080, 0.0), 20.0);
    assert_eq!(flash.timing.rate(None, 0.200, 0.0), 0.0);
    // Emitters 1 and 2, the glowball droplets: the same burst shape, peaking at 200.
    assert_eq!(defs[1].timing.peak_rate(), 200.0);
    assert_eq!(defs[2].timing.peak_rate(), 200.0);
    // The spray's enabled track cuts emission at 500 ms, where its rate track goes negative.
    assert!(defs[0].timing.emitting(None, 0.4, 0.0));
    assert!(!defs[0].timing.emitting(None, 0.6, 0.0));
}

/// `Feint_Impact_Chest.m2`: the plume (e0) and crescents (e3) set flag 0x8000, one puff at
/// their 67 ms key; `Eviscerate_Cast_Hands.m2` has the same 1.6 s shape with the flag clear.
#[test]
fn real_feint_impact_authors_burst_emitters() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let impact = parse_m2_particle_emitters(
        &chain
            .read_file("Spells\\Feint_Impact_Chest.m2")
            .expect("read Feint_Impact_Chest.m2"),
    )
    .expect("parse emitters");
    assert_eq!(impact.len(), 4);
    assert!(impact[0].burst(), "plume is a one-shot burst");
    assert!(!impact[1].burst(), "lava pours (enabled window 0–333 ms)");
    assert!(!impact[2].burst(), "dust pours (enabled window 67–200 ms)");
    assert!(impact[3].burst(), "crescents are a one-shot burst");
    // A step rate: 0 before the 67 ms key, its full value from it on.
    assert_eq!(impact[0].timing.rate(None, 0.050, 0.0), 0.0);
    assert_eq!(impact[0].timing.rate(None, 0.067, 0.0), 30.0);
    assert_eq!(impact[0].timing.rate(None, 1.500, 0.0), 30.0);
    let cast = parse_m2_particle_emitters(
        &chain
            .read_file("Spells\\Eviscerate_Cast_Hands.m2")
            .expect("read Eviscerate_Cast_Hands.m2"),
    )
    .expect("parse emitters");
    assert!(
        cast.iter().all(|e| !e.burst()),
        "the cast-hands flame is continuous — same asset shape, opposite flag"
    );
}

/// `FlameStrike_Area.m2`'s four fire columns: type-3 chains of 16 and 19 points, no spin.
#[test]
fn real_flamestrike_authors_spline_chains() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let defs = parse_m2_particle_emitters(
        &chain
            .read_file("Spells\\FlameStrike_Area.m2")
            .expect("read FlameStrike_Area.m2"),
    )
    .expect("parse emitters");
    let splines: Vec<_> = defs
        .iter()
        .filter(|d| d.shape == ParticleShape::Spline)
        .collect();
    assert_eq!(splines.len(), 4, "the four fire-column emitters");
    for d in &splines {
        let s = d.spline.as_ref().expect("chain parses");
        assert_eq!(s.points.len() % 3, 1, "3K+1 control points");
        assert!(s.points.len() >= 16);
        let now = d.params.sample(None, 0.0, 0.0);
        assert_eq!(now.area_length, 0.0, "tMin");
        assert_eq!(now.area_width, 1.0, "tMax");
        assert_eq!(now.vertical_range, 0.0, "no tangent spin");
        assert!(d.burst(), "one puff of standing flames");
        assert!(
            (5.0..15.0).contains(&s.points[0][2]),
            "z {}",
            s.points[0][2]
        );
        assert_eq!(s.eval(0.0), s.points[0], "t=0 is the first point");
    }
}

/// Fireball (spell 133): the cast flash is on for its clip's first 200 ms and the impact plume
/// ramps 0 → 60 over 133 ms of clip time, though seq 0 spans [1000, 2600] of the file.
#[test]
fn real_fireball_effects_rebase_to_clip_time() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cast = parse_m2_particle_emitters(
        &chain
            .read_file("Spells\\Fire_Cast_Hand.m2")
            .expect("read Fire_Cast_Hand.m2"),
    )
    .expect("parse emitters");
    assert_eq!(cast.len(), 3);
    for em in &cast {
        assert!(
            em.timing.emitting(None, 0.1, 0.0),
            "the flash is ON at its start"
        );
        assert!(
            !em.timing.emitting(None, 0.25, 0.0),
            "and OFF from 200 ms — the 1.0 s clip does not burn through"
        );
    }
    let impact = parse_m2_particle_emitters(
        &chain
            .read_file("Spells\\MoltenBlast_Impact_Chest.m2")
            .expect("read MoltenBlast_Impact_Chest.m2"),
    )
    .expect("parse emitters");
    assert_eq!(impact.len(), 6);
    // Build 12340 keys the plume (0 ms, 0) → (100 ms, 100) → (267 ms, 0), linear.
    assert_eq!(impact[0].timing.rate(None, 0.0, 0.0), 0.0);
    assert_eq!(
        impact[0].timing.rate(None, 0.1, 0.0),
        100.0,
        "the plume peaks at impact"
    );
    assert!(
        (impact[0].timing.rate(None, 0.133, 0.0) - 80.24).abs() < 0.01,
        "and falls linearly to its 267 ms key"
    );
    assert!(impact[1].timing.emitting(None, 0.1, 0.0));
    assert!(
        !impact[1].timing.emitting(None, 0.4, 0.0),
        "shockwave window is 300 ms"
    );
    assert!(
        !impact[4].timing.emitting(None, 0.05, 0.0),
        "smoke starts staggered…"
    );
    assert!(impact[4].timing.emitting(None, 0.2, 0.0));
    assert!(
        !impact[4].timing.emitting(None, 0.7, 0.0),
        "…and ends by 567 ms"
    );
}
