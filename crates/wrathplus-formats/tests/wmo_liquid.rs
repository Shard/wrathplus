//! WMO MLIQ liquid surfaces on 3.3.5: an `xverts × yverts` grid with one tile fewer each way and a
//! per-tile flag whose low nibble `0xf` is a hole; the surface's category is the native resolver's
//! LiquidType (`wmo_group_wrath_liquid`), not a tile or `groupLiquid` nibble.

use wrathplus_formats::wrath_liquid::{load_liquid_catalogs, LiquidCatalogs};
use wrathplus_formats::{
    parse_wmo_root, wmo_group_header, wmo_group_liquid_mesh, wmo_group_liquid_mesh_of_kind,
    wmo_group_wrath_liquid, Chain, LiquidKind, LiquidMesh,
};

const STORMWIND: &str = "World\\wmo\\Azeroth\\Buildings\\Stormwind\\Stormwind.wmo";
const BLACKFATHOM: &str = "World\\wmo\\dungeon\\kl_blackfathom\\blackfathom_instance.wmo";

fn chain() -> Option<(Chain, LiquidCatalogs)> {
    let data = wrathplus_formats::wow_data_or_skip!(None);
    let mut chain = Chain::open(&data).expect("open the patch chain");
    let catalogs = load_liquid_catalogs(&mut chain).expect("liquid catalogs");
    Some((chain, catalogs))
}

/// A group's bytes and its surface as the Wrath loader builds it.
fn group(
    chain: &mut Chain,
    catalogs: &LiquidCatalogs,
    root_path: &str,
    gi: u32,
) -> (Vec<u8>, Option<LiquidMesh>) {
    let root = parse_wmo_root(&chain.read_file(root_path).expect("root")).expect("parse root");
    let stem = root_path.strip_suffix(".wmo").unwrap();
    let bytes = chain
        .read_file(&format!("{stem}_{gi:03}.wmo"))
        .expect("group readable");
    let state = wmo_group_wrath_liquid(&bytes, &root).expect("native liquid decode");
    let mesh = catalogs
        .liquid_type(state.resolved_id)
        .and_then(|t| t.kind())
        .and_then(|kind| wmo_group_liquid_mesh_of_kind(&bytes, root.flags().unwrap(), kind));
    (bytes, mesh)
}

/// Stormwind's canals are five MOHD-flag-4 groups whose `groupLiquid` is the resolver's input 5
/// (WMO Water), an empty slot in the 1.12 nibble table: the legacy builder drops all of them.
#[test]
fn stormwind_canals_build_still_water() {
    let Some((mut chain, catalogs)) = chain() else {
        return;
    };
    let root = parse_wmo_root(&chain.read_file(STORMWIND).unwrap()).unwrap();
    let wet: Vec<u32> = (0..root.group_count())
        .filter(|&gi| group(&mut chain, &catalogs, STORMWIND, gi).1.is_some())
        .collect();
    assert_eq!(
        wet,
        vec![224, 281, 282, 283, 284],
        "Stormwind's water groups"
    );
    for gi in wet {
        let (bytes, mesh) = group(&mut chain, &catalogs, STORMWIND, gi);
        let mesh = mesh.unwrap();
        assert_eq!(
            mesh.kind,
            LiquidKind::Still,
            "group {gi}: canal water is still"
        );
        assert!(
            wmo_group_liquid_mesh(&bytes).is_none(),
            "group {gi}: the 1.12 nibble reading has no kind for input 5"
        );
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.positions.len()));
    }

    // The main canal: 53×47 vertices, 52×46 tiles, 707 wet and the rest holes.
    let (_, canal) = group(&mut chain, &catalogs, STORMWIND, 281);
    let canal = canal.unwrap();
    assert_eq!(canal.grid, [53, 47]);
    assert_eq!(canal.positions.len(), 53 * 47);
    assert_eq!(canal.uvs.len(), 53 * 47);
    assert_eq!(canal.depths.len(), 53 * 47);
    assert_eq!(canal.indices.len(), 707 * 6, "two triangles per wet tile");

    let (_, masonry) = group(&mut chain, &catalogs, STORMWIND, 0);
    assert!(masonry.is_none(), "a masonry group carries no liquid");
}

/// MLIQ carries a height for every vertex, wet or not, and shipped files leave hole interiors at
/// `0.0`; the builder gives every undrawn vertex the lowest drawn height, so the bounds stay on
/// the sheet.
#[test]
fn hole_corner_heights_never_reach_the_mesh_bounds() {
    let Some((mut chain, catalogs)) = chain() else {
        return;
    };
    for (root_path, gi, surface_z) in [
        (BLACKFATHOM, 7, -58.283_04_f32),
        (STORMWIND, 281, -6.482_947_f32),
    ] {
        let (_, mesh) = group(&mut chain, &catalogs, root_path, gi);
        let mesh = mesh.unwrap_or_else(|| panic!("{root_path} {gi} carries water"));
        let (lo, hi) = mesh
            .positions
            .iter()
            .map(|p| p[2])
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), z| {
                (lo.min(z), hi.max(z))
            });
        assert!(
            (lo - surface_z).abs() < 1e-3 && (hi - surface_z).abs() < 1e-3,
            "{root_path} {gi}: mesh z spans [{lo}..{hi}], expected the flat sheet at {surface_z}"
        );
    }
}

/// `0x6b62e0`'s water splits on the group's `MOGP.flags & 0x48`: the exterior arm lights a vertex
/// normal, the interior arm takes its body colour from `MOMT[materialId].diffColor`. Each pool
/// carries a per-vertex opacity.
#[test]
fn the_two_water_arms_carry_their_own_inputs() {
    let Some((mut chain, catalogs)) = chain() else {
        return;
    };
    // (root, group, is the group interior?, the pool's MLIQ materialId)
    for (root_path, gi, interior, material_id) in [
        (BLACKFATHOM, 7, true, 27_u16),
        (STORMWIND, 281, false, 189_u16),
    ] {
        let (bytes, mesh) = group(&mut chain, &catalogs, root_path, gi);
        let header = wmo_group_header(&bytes).expect("a MOGP header");
        assert_eq!(
            header.flags & 0x48 == 0,
            interior,
            "{root_path} {gi}: MOGP flags {:#010x} put it on the wrong water arm",
            header.flags
        );
        let mesh = mesh.unwrap_or_else(|| panic!("{root_path} {gi} carries water"));
        assert_eq!(
            mesh.material_id,
            Some(material_id),
            "{root_path} {gi}: its MOMT slot"
        );
        let (lo, hi) = mesh
            .depths
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(
            (0.0..=1.0).contains(&lo) && (0.0..=1.0).contains(&hi) && hi > 0.0,
            "{root_path} {gi}: opacity V [{lo}..{hi}] out of range or unread"
        );
    }
}

/// WMO water repeats its texture once per 4.167-yd grid cell: `0x6b6630` writes `u = i, v = j`.
#[test]
fn wmo_water_repeats_once_per_grid_cell() {
    let Some((mut chain, catalogs)) = chain() else {
        return;
    };
    let (_, mesh) = group(&mut chain, &catalogs, STORMWIND, 281);
    let mesh = mesh.expect("the main canal carries water");
    let du = mesh.uvs[1][0] - mesh.uvs[0][0];
    assert!(
        (du - 1.0).abs() < 1e-4,
        "adjacent vertices differ by {du} in u"
    );
    let row = mesh.grid[0] as usize;
    let dv = mesh.uvs[row][1] - mesh.uvs[0][1];
    assert!((dv - 1.0).abs() < 1e-4, "adjacent rows differ by {dv} in v");
}
