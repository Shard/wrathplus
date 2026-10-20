//! Generated (environment-map) texcoords: `texCoordSet` (`+0x12`) indexes `texture_unit_lookup`
//! (`0x9c`), and the reference takes a value above 2 as generated (`0x70b8bd`). The Deeprun Tram's
//! glass authors `[-1]` and leaves all 330 vertices' UVs at (0, 0).

use benilla_formats::{load_m2_mesh, open_chain};

#[test]
fn tram_glass_batches_generate_their_texcoords() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = open_chain(&data).expect("open patch chain");
    let subs = load_m2_mesh(
        &mut chain,
        "World\\Generic\\Gnome\\Passive Doodads\\GnomeMachine\\GnomeSubwayGlass.m2",
    )
    .expect("load GnomeSubwayGlass");

    assert!(!subs.is_empty(), "the glass tube has render batches");
    assert!(
        subs.iter().all(|s| s.env_map),
        "every GnomeSubwayGlass batch authors texture_unit_lookup = -1 (generated texcoords)"
    );
    for s in &subs {
        assert!(
            s.uvs.iter().all(|uv| uv[0] == 0.0 && uv[1] == 0.0),
            "an env-mapped batch's authored UVs are degenerate — the runtime supplies them"
        );
    }
}

#[test]
fn weapon_rack_sheen_is_a_generated_second_stage() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = open_chain(&data).expect("open patch chain");
    let subs = load_m2_mesh(
        &mut chain,
        "World\\Generic\\Human\\Passive Doodads\\WeaponRacks\\GeneralWeaponrack01.m2",
    )
    .expect("load GeneralWeaponrack01");

    // `texture_unit_lookup = [0, -1]`: in build 12340 the ARMORREFLECT sheen is stage 2 of a
    // two-texture batch, reading lookup entry 1 (generated); every stage 0 reads UV channel 0.
    assert!(
        subs.iter().all(|s| !s.env_map),
        "every first stage reads its authored UVs"
    );
    let sheens: Vec<_> = subs
        .iter()
        .filter_map(|s| s.stage2.as_ref())
        .filter(|st| st.coord > 2)
        .collect();
    assert!(
        !sheens.is_empty(),
        "the rack carries a generated sheen stage"
    );
    assert!(
        sheens.iter().all(|st| st
            .texture
            .as_deref()
            .is_some_and(|t| t.to_ascii_lowercase().contains("armorreflect"))),
        "only the ARMORREFLECT sheen generates its texcoords"
    );
}
