//! The MH2O lanes and mesh inputs against the installed Wrath chain: every Northrend layer the
//! effective archives ship, through the production policy and vertex builder. Skips without a
//! Wrath install (`WOW_CLIENT_PROFILE=wrath`); no assets are written or committed.

use std::collections::BTreeMap;

use super::wrath_inputs::{mesh_inputs, texture_filenames};
use super::wrath_surface::{lane_policy, WrathRender};
use benilla_formats::wrath_liquid::load_liquid_catalogs;
use benilla_formats::{adt_to_tile_mesh, Chain, DataProfile};

fn wrath_chain() -> Option<Chain> {
    let data = benilla_formats::wow_data_or_skip!(None);
    let profile = DataProfile::from_env().expect("valid WOW_CLIENT_PROFILE/WOW_LOCALE");
    if !matches!(profile, DataProfile::Wrath { .. }) {
        eprintln!("skipped: WOW_CLIENT_PROFILE is not wrath");
        return None;
    }
    Some(Chain::open_profile(&data, &profile).expect("open the Wrath chain"))
}

/// Every Northrend MH2O layer resolves to a lane and builds valid vertex inputs; the five shipped
/// IDs take the arms and swatches their catalog rows name, and every lane's first frame exists.
#[test]
fn every_northrend_mh2o_layer_builds_through_its_lane() {
    let Some(mut chain) = wrath_chain() else {
        return;
    };
    let catalogs = load_liquid_catalogs(&mut chain).expect("liquid catalogs");
    let mut entries = chain
        .list_filtered(|n| {
            let n = n.replace('/', "\\").to_ascii_lowercase();
            n.starts_with("world\\maps\\northrend\\") && n.ends_with(".adt")
        })
        .expect("list Northrend");
    entries.sort_by_key(|e| e.name.to_ascii_lowercase());
    assert_eq!(entries.len(), 1131, "effective Northrend ADT count moved");

    let mut layers: BTreeMap<u32, u64> = BTreeMap::new();
    let (mut drawn, mut dry) = (0u64, 0u64);
    for entry in &entries {
        let bytes = chain.read_file(&entry.name).expect("ADT readable");
        let tile = adt_to_tile_mesh(&bytes).unwrap_or_else(|e| panic!("{}: {e:#}", entry.name));
        for surface in &tile.wrath_liquids {
            let id = u32::from(surface.layer.liquid_type);
            let ty = catalogs.liquid_type(id).expect("type row");
            let policy = lane_policy(ty, catalogs.material_for(ty))
                .unwrap_or_else(|e| panic!("type {id}: {e:?}"));
            let inputs = mesh_inputs(
                surface,
                policy.material_selector,
                policy.depth_table,
                &surface.positions,
            )
            .unwrap_or_else(|e| panic!("{} type {id}: {e}", entry.name));
            assert!(inputs.depths.iter().all(|d| (0.0..=1.0).contains(d)));
            if inputs.indices.is_empty() {
                dry += 1;
            } else {
                drawn += 1;
            }
            *layers.entry(id).or_default() += 1;
        }
    }
    eprintln!("Northrend MH2O: {layers:?}, {drawn} drawn, {dry} all-dry");
    assert_eq!(layers.values().sum::<u64>(), 226_825, "layer census moved");
    assert_eq!(
        layers.keys().copied().collect::<Vec<_>>(),
        [1, 2, 5, 81, 181]
    );
    assert!(drawn > 0);

    let expect = [
        (
            1,
            WrathRender::Water {
                ocean_swatch: false,
            },
        ),
        (2, WrathRender::Water { ocean_swatch: true }),
        (
            5,
            WrathRender::Water {
                ocean_swatch: false,
            },
        ),
        (
            81,
            WrathRender::Water {
                ocean_swatch: false,
            },
        ),
        (
            181,
            WrathRender::Water {
                ocean_swatch: false,
            },
        ),
    ];
    for (id, render) in expect {
        let ty = catalogs.liquid_type(id).unwrap();
        let policy = lane_policy(ty, catalogs.material_for(ty)).unwrap();
        assert_eq!(policy.render, render, "type {id}");
        let frames = texture_filenames(&policy.texture).unwrap();
        let present = frames
            .iter()
            .take_while(|p| chain.read_file(p).is_ok())
            .count();
        eprintln!("type {id} {}: {present} frame(s)", policy.texture);
        assert!(present > 0, "type {id}: no frames for {}", policy.texture);
    }
}
