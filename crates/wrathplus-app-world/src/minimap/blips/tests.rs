use super::*;

/// The call site gives only the sequence id; the installed 3.3.5 model says what it draws.
#[test]
fn installed_each_rim_arrow_sequence_lights_exactly_its_own_layer() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("the patch chain");
    let path = "Interface\\Minimap\\Rotating-MinimapArrow";
    let bytes = chain
        .read_file(&format!("{path}.m2"))
        .expect("the rim-arrow model");
    let skin = chain
        .read_file(&format!("{path}00.skin"))
        .expect("its skin");
    let model = wrathplus_formats::parse_wrath_render_model(&bytes, &skin, "", &[])
        .expect("the rim arrow parses");

    for kind in RimArrow::ALL {
        let seq = model
            .animations
            .iter()
            .find(|a| a.anim_id == kind.anim_id())
            .unwrap_or_else(|| panic!("{kind:?}: no sequence {:#x}", kind.anim_id()));
        // Lit if its alpha reaches past half anywhere in the band: some layers fade in.
        let shown: Vec<String> = model
            .submeshes
            .iter()
            .filter(|sub| {
                (0..=20).any(|step| {
                    let t = seq.duration * step as f32 / 20.0;
                    sub.alpha_anim
                        .as_ref()
                        .map_or(1.0, |a| a.sample(Some(seq.seq_index), t, 0.0))
                        > 0.5
                })
            })
            .filter_map(|sub| sub.texture.clone())
            .collect();
        assert_eq!(
            shown,
            vec![format!("{}.BLP", kind.texture().to_uppercase())],
            "{kind:?} (sequence {:#x}) must light its layer and ONLY its layer",
            kind.anim_id()
        );
    }
}

fn poi(importance: u32, flags: u32, x: f32, name: &str) -> AreaPoi {
    AreaPoi {
        importance,
        icon: 6,
        faction_id: 0,
        pos: [x, 0.0, 0.0],
        continent_id: 0,
        flags,
        area_id: 0,
        name: name.into(),
        description: String::new(),
        world_state_id: 0,
    }
}

/// Real 1.12 AreaPOI rows near Northshire Abbey at the default zoom's 133-yd radius.
#[test]
fn northshire_shape_under_the_byte_law() {
    let rows = [
        poi(3, 0x5, 46.0, "Northshire Abbey"),
        poi(0, 0x4, 237.0, "Echo Ridge Mine"),
        poi(3, 0x1d, 556.0, "Stormwind"),
        poi(3, 0x5, 601.0, "Goldshire"),
    ];
    let sel = select_landmarks(rows.iter(), None, 0, 0.0, 0.0, 133.0);
    assert!(sel.icons.is_empty(), "no Flags&2 row is in range");
    let names: Vec<&str> = sel.arrows.iter().map(|(_, p, _)| p.name.as_str()).collect();
    assert_eq!(names, ["Stormwind", "Goldshire"]);
}

/// The rank key is `Importance` (DBC column 1), not AreaID; distance only breaks ties.
#[test]
fn arrow_rank_is_importance_then_distance_capped_at_three() {
    let rows = [
        poi(3, 1, 200.0, "city-near"),
        poi(0, 1, 650.0, "minor-far"),
        poi(0, 1, 300.0, "minor-near"),
        poi(3, 1, 400.0, "city-mid"),
        poi(0, 1, 695.0, "beyond-rank"),
    ];
    let sel = select_landmarks(rows.iter(), None, 0, 0.0, 0.0, 100.0);
    let names: Vec<&str> = sel.arrows.iter().map(|(_, p, _)| p.name.as_str()).collect();
    assert_eq!(names, ["minor-near", "minor-far", "city-near"]);
}

/// The marker carries `Importance` 0, so it outranks the cities though farthest.
#[test]
fn the_guard_marker_competes_for_a_rim_slot_and_outranks_the_cities() {
    let cities = [
        poi(3, 0x1d, 200.0, "Stormwind"),
        poi(3, 0x5, 300.0, "Goldshire"),
        poi(3, 0x5, 400.0, "Northshire Abbey"),
    ];
    let marker = poi(0, 0x63, 500.0, "Stormwind Warrior Trainer");
    let sel = select_landmarks(cities.iter(), Some(&marker), 0, 0.0, 0.0, 133.0);
    let names: Vec<&str> = sel.arrows.iter().map(|(_, p, _)| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["Stormwind Warrior Trainer", "Stormwind", "Goldshire"],
        "Importance 0 ranks the marker first; the third city is crowded out"
    );
    let arts: Vec<RimArrow> = sel.arrows.iter().map(|&(_, _, a)| a).collect();
    assert_eq!(
        arts,
        [RimArrow::Guide, RimArrow::Landmark, RimArrow::Landmark],
        "the guard's directions draw the guide arrow; DBC rows draw the landmark arrow"
    );
}

/// Every 1.12 `points_of_interest` row carries flags 99 (0x63), so `Flags & 2` is set.
#[test]
fn the_guard_marker_draws_its_icon_in_range() {
    let marker = poi(0, 0x63, 50.0, "The Bank");
    let sel = select_landmarks(std::iter::empty(), Some(&marker), 0, 0.0, 0.0, 133.0);
    assert!(sel.arrows.is_empty(), "in range — no rim arrow");
    assert_eq!(sel.icons.len(), 1);
    assert_eq!(
        poi_icon_cell(sel.icons[0].icon),
        Some([0.75, 0.875, 0.0, 0.125]),
        "icon 6 = ICON_POI_REDFLAG, col 6 row 0 of the 8x8 atlas"
    );
}

/// The caller ([`crate::poi_marker::PoiMarker`]'s `on_map`) decides the marker's map.
#[test]
fn the_guard_marker_skips_the_candidacy_filter() {
    let mut marker = poi(0, 0, 300.0, "The Inn"); // Flags bit 0 clear: a DBC row would drop
    marker.continent_id = 571; // and a continent that isn't the displayed one
    let sel = select_landmarks(std::iter::empty(), Some(&marker), 0, 0.0, 0.0, 133.0);
    assert_eq!(sel.arrows.len(), 1, "appended unconditionally");
}

#[test]
fn in_range_icons_gate_on_flag_bit1_and_the_atlas_bound() {
    let mut tower = poi(0, 0x87, 50.0, "Crown Guard Tower");
    tower.icon = 9; // col 1, row 1
    let plain = poi(3, 0x5, 50.0, "Northshire Abbey");
    let rows = [tower, plain];
    let sel = select_landmarks(rows.iter(), None, 0, 0.0, 0.0, 133.0);
    assert_eq!(sel.arrows.len(), 0);
    assert_eq!(sel.icons.len(), 1, "only the Flags&2 tower draws in range");
    assert_eq!(
        poi_icon_cell(9),
        Some([0.125, 0.25, 0.125, 0.25]),
        "icon 9 = col 1 row 1 of the 8x8 atlas"
    );
    assert_eq!(poi_icon_cell(64), None, "the client gates Icon < 0x40");
}
