use super::*;

fn liquid_chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend((payload.len() as u32).to_le_bytes());
    out.extend(payload);
    out
}

#[test]
fn wrath_liquid_root_catalog_projection_retains_grid_without_render_mesh() {
    use wrathplus_formats::{wrath_liquid::parse_liquid_catalogs, LiquidKind};
    fn dbc(rows: &[Vec<u32>], words: u32) -> Vec<u8> {
        let mut b = b"WDBC".to_vec();
        for w in [rows.len() as u32, words, words * 4, 1] {
            b.extend(w.to_le_bytes());
        }
        for row in rows {
            for w in row {
                b.extend(w.to_le_bytes());
            }
        }
        b.push(0);
        b
    }
    let mut row = vec![0; 45];
    row[0] = 0x1239; // Low nibble deliberately disagrees with category.
    row[3] = 2;
    let catalogs = parse_liquid_catalogs(&dbc(&[row], 45), &dbc(&[], 3)).unwrap();
    let mut mohd = vec![0; 64];
    mohd[60..64].copy_from_slice(&5u32.to_le_bytes());
    let root = parse_wmo_root(&liquid_chunk(b"DHOM", &mohd)).unwrap();
    let group = |id: u32, grid: bool| {
        let mut h = vec![0; 68];
        h[52..56].copy_from_slice(&id.to_le_bytes());
        if grid {
            h[8..12].copy_from_slice(&0x1000u32.to_le_bytes());
            let mut m = vec![0; 30 + 4 * 8];
            for (i, w) in [2u32, 2, 1, 1].into_iter().enumerate() {
                m[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
            }
            m.push(15); // All holes: geometry may be absent, grid must remain bounded.
            h.extend(liquid_chunk(b"QILM", &m));
        }
        liquid_chunk(b"PGOM", &h)
    };
    let dry = WmoLiquidState::decode(&group(0, false), &root, &catalogs).unwrap();
    assert_eq!(dry.flood_kind(), None);
    assert_eq!(dry.grid_kind(), None);
    let flood = WmoLiquidState::decode(&group(0x1239, false), &root, &catalogs).unwrap();
    assert_eq!(flood.flood_kind(), Some(LiquidKind::Magma));
    assert_eq!(flood.grid_kind(), None);
    let grid = WmoLiquidState::decode(&group(0x1239, true), &root, &catalogs).unwrap();
    assert_eq!(grid.flood_kind(), None);
    assert_eq!(grid.grid_kind(), Some(LiquidKind::Magma));
    assert_eq!(grid.native.tile_dimensions, [1, 1]);
    let unknown = WmoLiquidState::decode(&group(999, false), &root, &catalogs).unwrap();
    assert_eq!(unknown.native.resolved_id, 999);
    assert_eq!(unknown.flood_kind(), None);
    assert!(WmoLiquidState::decode(&[], &root, &catalogs).is_err());
}

fn grp(interior: bool) -> WmoGroupInfo {
    WmoGroupInfo {
        interior,
        show_skybox: false,
        bbox_min: [-1.0; 3],
        bbox_max: [1.0; 3],
    }
}

fn prop(color: [u8; 4]) -> WmoDoodad {
    WmoDoodad {
        model: "candle.m2".into(),
        position: [0.0; 3],
        orientation: [0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        color,
    }
}

/// Northshire Abbey's candle stands MODD[18] and MODD[24], as the reference commits them
/// through `0x6a77e0` (cap scales 182 and 173).
#[test]
fn cap96_matches_the_decoded_abbey_stands_bit_exact() {
    let as_bytes = |c: [f32; 3]| c.map(|v| (v * 255.0).round() as u8);
    assert_eq!(as_bytes(cap96([78, 76, 134])), [56, 55, 96]);
    assert_eq!(as_bytes(cap96([90, 86, 141])), [61, 59, 96]);
    assert_eq!(as_bytes(cap96([96, 40, 20])), [96, 40, 20]);
    assert_eq!(as_bytes(floor112([78, 76, 134])), [78, 76, 134]);
    assert_eq!(as_bytes(floor112([90, 86, 141])), [90, 86, 141]);
    let raised = as_bytes(floor112([56, 28, 14]));
    assert_eq!(raised[0], 112);
    assert_eq!(raised, [112, 56, 28]);
    // Black stays black in the reference too: it scales the HSV value by `thresh/max`
    // (`0x6a78a5`) and forces max to 1 only to keep that divide finite (`0x6a780e`).
    assert_eq!(as_bytes(floor112([0, 0, 0])), [0, 0, 0]);
}

#[test]
fn floor168_matches_the_decoded_abbey_benches_bit_exact() {
    // The abbey inn benches as the reference commits them; 63·168/83 = 127.52 truncates to 127.
    let as_bytes = |c: [f32; 3]| c.map(|v| (v * 255.0).round() as u8);
    assert_eq!(as_bytes(cap96([59, 65, 92])), [59, 65, 92]);
    assert_eq!(as_bytes(cap96([69, 63, 83])), [69, 63, 83]);
    assert_eq!(as_bytes(floor168([59, 65, 92])), [107, 118, 168]);
    assert_eq!(as_bytes(floor168([69, 63, 83])), [139, 127, 168]);
    assert_eq!(as_bytes(floor168([200, 30, 10])), [200, 30, 10]);
}

#[test]
fn modr_ownership_picks_the_lane_and_the_light_refs() {
    let doodads = vec![
        prop([90, 86, 141, 255]), // owned by interior group 0
        prop([90, 86, 141, 255]), // owned by exterior group 1
        prop([90, 86, 141, 255]), // referenced by no group
    ];
    let groups = [grp(true), grp(false)];
    let modr = vec![vec![0u16], vec![1u16]];
    let molr = vec![vec![7u16, 9u16], vec![3u16]];
    let owner = modr_owners(doodads.len(), &modr);
    let refs = modr_refs(doodads.len(), &modr);
    assert_eq!(owner, vec![Some(0), Some(1), None]);
    let bases = resolve_doodad_bases(&doodads, &groups, &owner, &refs, &molr);
    match &bases[0] {
        DoodadBase::Interior(b) => {
            assert_eq!(b.light_refs, vec![7, 9]);
            assert_eq!(b.diffuse.map(|v| (v * 255.0).round() as u8), [90, 86, 141]);
            assert_eq!(b.ambient.map(|v| (v * 255.0).round() as u8), [61, 59, 96]);
        }
        other => panic!("interior-group doodad must take the MODD-colour base, got {other:?}"),
    }
    assert_eq!(bases[1], DoodadBase::Exterior);
    assert_eq!(bases[2], DoodadBase::Exterior);
}

#[test]
fn one_exterior_referrer_makes_the_whole_prop_exterior() {
    let doodads = vec![
        prop([0, 0, 0, 255]), // g0 (interior) names it first, g1 (exterior) also names it
        prop([0, 0, 0, 255]), // interior groups only
    ];
    let groups = [grp(true), grp(false), grp(true)];
    // g0 names both props, g1 (exterior) names only prop 0, g2 (interior) names only prop 1.
    let modr = vec![vec![0u16, 1u16], vec![0u16], vec![1u16]];
    let molr = vec![vec![7u16], Vec::new(), vec![4u16]];
    let owner = modr_owners(doodads.len(), &modr);
    let refs = modr_refs(doodads.len(), &modr);
    assert_eq!(
        owner,
        vec![Some(0), Some(0)],
        "g0 is the first referrer of both"
    );
    let bases = resolve_doodad_bases(&doodads, &groups, &owner, &refs, &molr);
    assert_eq!(
        bases[0],
        DoodadBase::Exterior,
        "the exterior referrer wins even though an interior group names it first"
    );
    match &bases[1] {
        DoodadBase::Interior(b) => {
            assert_eq!(b.ambient, [0.0; 3]);
            assert_eq!(b.diffuse, [0.0; 3]);
            assert_eq!(b.light_refs, vec![7]);
        }
        other => panic!("interior-only prop must keep the MODD-colour lane, got {other:?}"),
    }
}

/// Booty Bay's entrance arch, `BootyBay.wmo` MODD[3], named first by interior g22 and also by
/// exterior g42, with a black MODD colour.
#[test]
fn booty_bays_entrance_arch_is_sky_lit_not_a_black_silhouette() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let stem = "World\\wmo\\Azeroth\\Buildings\\Stranglethorn_BootyBay\\BootyBay";
    let chain = wrathplus_formats::Chain::open(&data).expect("open vanilla patch chain");
    let bytes = chain
        .read(&format!("{stem}.wmo"))
        .expect("read BootyBay.wmo");
    let root = parse_wmo_root(&bytes).expect("parse BootyBay.wmo");
    let refs: Vec<Vec<u16>> = (0..root.group_count())
        .map(|gi| {
            chain
                .read(&format!("{stem}_{gi:03}.wmo"))
                .map(|g| wmo_group_doodad_refs(&g))
                .unwrap_or_default()
        })
        .collect();

    let arch = 3usize;
    assert!(
        root.doodads()[arch]
            .model
            .to_ascii_lowercase()
            .contains("bootybayentrance_02"),
        "MODD[3] should be the entrance arch, got {}",
        root.doodads()[arch].model
    );
    assert_eq!(
        root.doodads()[arch].color,
        [0, 0, 0, 255],
        "the arch's baked MODD colour is unlit black — that is what made the lane load-bearing"
    );
    let owner = modr_owners(root.doodads().len(), &refs);
    let groups = modr_refs(root.doodads().len(), &refs);
    assert_eq!(
        owner[arch],
        Some(22),
        "an INTERIOR group still names the arch first"
    );
    assert!(
        groups[arch]
            .iter()
            .any(|&g| !root.group_infos()[g as usize].interior),
        "…and an EXTERIOR group also names it — the case exterior-wins decides"
    );
    let bases = resolve_doodad_bases(
        root.doodads(),
        root.group_infos(),
        &owner,
        &groups,
        &vec![Vec::new(); root.group_count() as usize],
    );
    assert_eq!(
        bases[arch],
        DoodadBase::Exterior,
        "the entrance arch must take the sky-lit lane, not the all-zero MODD-colour base"
    );
}

/// Northshire Abbey's group 3 has no MOLR, and its candle stand takes no point light in the
/// reference, its own flame included.
#[test]
fn no_molr_means_no_point_lights_at_all() {
    let bases = resolve_doodad_bases(
        &[prop([120, 110, 150, 255])],
        &[grp(true)],
        &[Some(0)],
        &[Arc::from(vec![0u16])],
        &[Vec::new()],
    );
    match &bases[0] {
        DoodadBase::Interior(b) => assert!(b.light_refs.is_empty()),
        other => panic!("expected interior base, got {other:?}"),
    }
}

#[test]
fn a_shared_modd_takes_its_first_referencing_groups_molr() {
    // g0 names doodad 2; g1 names 0 and 2; g2 names 1, plus a ref past the end of the list.
    let modr = vec![vec![2u16], vec![0u16, 2u16], vec![1u16, 99u16]];
    assert_eq!(
        modr_owners(3, &modr),
        vec![Some(1), Some(2), Some(0)],
        "doodad 2 is named by g0 and g1 — g0 wins"
    );
    assert_eq!(modr_owners(2, &[]), vec![None, None]);
}

#[test]
fn the_cull_key_is_every_referencing_group() {
    // The fixture above: g0 names 2; g1 names 0 and 2; g2 names 1 and an out-of-range ref.
    let modr = vec![vec![2u16], vec![0u16, 2u16], vec![1u16, 99u16]];
    let refs = modr_refs(3, &modr);
    assert_eq!(&*refs[0], &[1], "doodad 0 is named by g1 alone");
    assert_eq!(&*refs[1], &[2]);
    assert_eq!(
        &*refs[2],
        &[0, 1],
        "doodad 2 hangs in two rooms — either one draws it"
    );
    // A doodad named twice by one group counts once.
    assert_eq!(&*modr_refs(1, &[vec![0u16, 0u16]])[0], &[0]);
    assert!(modr_refs(2, &[])[0].is_empty());
}
