//! The MH2O lanes and mesh inputs against the installed Wrath chain: every Northrend layer the
//! effective archives ship, and the Eastern Kingdoms cases the 1.12 MCLQ tests pinned (3.3.5 ships
//! those tiles as MH2O only), through the production policy and vertex builder. Skips without a
//! Wrath install (`WOW_CLIENT_PROFILE=wrath`); no assets are written or committed.

use std::collections::BTreeMap;

use super::wrath_inputs::{mesh_inputs, texture_filenames};
use super::wrath_surface::{lane_policy, WrathRender};
use benilla_formats::wrath_liquid::load_liquid_catalogs;
use benilla_formats::{adt_to_tile_mesh, Chain, DataProfile, LiquidKind};

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

/// One drawn MH2O layer of an installed tile, through its lane as `spawn_wrath_liquids` takes it.
struct EkLayer {
    surface: benilla_formats::wrath_liquid_surface::WrathLiquidSurface,
    policy: super::wrath_surface::WrathLanePolicy,
    inputs: super::wrath_inputs::WrathMeshInputs,
}

/// Every drawn MH2O layer on `Azeroth_{tx}_{ty}`; 3.3.5 ships no MCLQ there.
fn azeroth_layers(chain: &mut Chain, tx: u32, ty: u32) -> Vec<EkLayer> {
    let catalogs = load_liquid_catalogs(chain).expect("liquid catalogs");
    let bytes = chain
        .read_file(&format!("World\\Maps\\Azeroth\\Azeroth_{tx}_{ty}.adt"))
        .expect("ADT readable");
    let tile = adt_to_tile_mesh(&bytes).expect("ADT parses");
    assert!(
        tile.chunks.iter().all(|c| c.liquids.is_empty()),
        "Azeroth_{tx}_{ty} carries MCLQ on 3.3.5"
    );
    tile.wrath_liquids
        .into_iter()
        .filter_map(|surface| {
            let ty = catalogs.liquid_type(u32::from(surface.layer.liquid_type))?;
            let policy = lane_policy(ty, catalogs.material_for(ty)).ok()?;
            let inputs = mesh_inputs(
                &surface,
                policy.material_selector,
                policy.depth_table,
                &surface.positions,
            )
            .ok()
            .filter(|i| !i.indices.is_empty())?;
            Some(EkLayer {
                surface,
                policy,
                inputs,
            })
        })
        .collect()
}

fn drawn_z(layer: &EkLayer) -> Vec<f32> {
    layer
        .inputs
        .indices
        .iter()
        .map(|&i| layer.inputs.positions[i as usize][2])
        .collect()
}

/// Crystal Lake, Elwynn (`Azeroth_32_48`): the 42 chunks the 1.12 MCLQ carried are 42 MH2O layers
/// of still water, each a near-planar sheet at Elwynn's elevation.
#[test]
fn installed_elwynn_lake_tile_is_42_still_mh2o_layers() {
    let Some(mut chain) = wrath_chain() else {
        return;
    };
    let layers = azeroth_layers(&mut chain, 32, 48);
    assert_eq!(layers.len(), 42, "Azeroth_32_48 liquid layers");
    for l in &layers {
        assert_eq!(
            l.policy.query_kind,
            Some(LiquidKind::Still),
            "Crystal Lake is still water"
        );
        let z = drawn_z(l);
        assert!(
            z.iter().all(|z| (50.0..300.0).contains(z)),
            "Elwynn water elevation {z:?}"
        );
        let (lo, hi) = z
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &z| (a.min(z), b.max(z)));
        assert!(
            hi - lo < 30.0,
            "per-layer surface roughly flat ({lo}..{hi})"
        );
    }
}

/// Open-world lava is MH2O magma: the pin, `.go xyz -7845.99 -1065.50 123.60 0` in Burning Steppes
/// (`Azeroth_33_46`), stands on its 64 layers, drawn on the fullbright lane; no ADT slime ships.
#[test]
fn installed_burning_steppes_lava_is_mh2o_magma_at_the_reported_pin() {
    let Some(mut chain) = wrath_chain() else {
        return;
    };
    let layers = azeroth_layers(&mut chain, 33, 46);
    assert_eq!(layers.len(), 64, "Burning Steppes tile 33_46 magma layers");
    assert!(layers
        .iter()
        .all(|l| l.policy.query_kind == Some(LiquidKind::Magma)
            && l.policy.render == WrathRender::Fullbright));
    let (px, py, pz) = (-7845.99f32, -1065.50, 123.60);
    let hit = layers
        .iter()
        .find_map(|l| {
            super::wrath_surface::adt_water_info(&l.surface, LiquidKind::Magma).surface_z_at(px, py)
        })
        .expect("the reported pin stands on a magma layer");
    assert!(
        (hit - pz).abs() < 6.0,
        "lava surface {hit} at the reported eye height {pz}"
    );
}

/// The lava's UVs are authored per vertex and world-continuous: a vertex two layers share carries
/// one UV, and the step is not the generated ¼-per-cell field.
#[test]
fn installed_burning_steppes_magma_uvs_are_authored_and_continuous() {
    let Some(mut chain) = wrath_chain() else {
        return;
    };
    let layers = azeroth_layers(&mut chain, 33, 46);
    assert!(layers
        .iter()
        .all(|l| l.surface.layer.texture_coordinates.is_some()));
    assert!(
        layers.iter().any(|l| {
            let du = (l.inputs.uvs[1][0] - l.inputs.uvs[0][0]).abs();
            du > 0.001 && (du - 0.25).abs() > 0.05
        }),
        "magma UVs are authored, not generated"
    );
    let mut shared = 0;
    for (ai, a) in layers.iter().enumerate() {
        for b in &layers[ai + 1..] {
            for (pa, ua) in a.inputs.positions.iter().zip(&a.inputs.uvs) {
                for (pb, ub) in b.inputs.positions.iter().zip(&b.inputs.uvs) {
                    if (pa[0] - pb[0]).abs() < 0.01 && (pa[1] - pb[1]).abs() < 0.01 {
                        assert!(
                            (ua[0] - ub[0]).abs() < 1e-4 && (ua[1] - ub[1]).abs() < 1e-4,
                            "shared lava vertex carries one UV: {ua:?} vs {ub:?}"
                        );
                        shared += 1;
                    }
                }
            }
        }
    }
    assert!(shared > 0, "no shared lava layer edge found to check");
}

/// A river mouth (Hillsbrad, `Azeroth_33_33`) stacks two MH2O layers in one chunk: the stream
/// above the sea.
#[test]
fn installed_river_mouth_chunk_stacks_a_river_over_the_sea() {
    let Some(mut chain) = wrath_chain() else {
        return;
    };
    let layers = azeroth_layers(&mut chain, 33, 33);
    let mut by_chunk: BTreeMap<[u32; 2], Vec<&EkLayer>> = BTreeMap::new();
    for l in &layers {
        by_chunk
            .entry(
                l.surface
                    .parent
                    .as_ref()
                    .expect("terrain layer has its MCNK")
                    .index,
            )
            .or_default()
            .push(l);
    }
    let mut mouths = 0;
    for stack in by_chunk.values().filter(|s| s.len() > 1) {
        let mean = |k: LiquidKind| {
            stack
                .iter()
                .find(|l| l.policy.query_kind == Some(k))
                .map(|l| {
                    let z = drawn_z(l);
                    z.iter().sum::<f32>() / z.len() as f32
                })
        };
        if let (Some(river), Some(sea)) = (mean(LiquidKind::Still), mean(LiquidKind::Ocean)) {
            assert!(
                river > sea + 1.0,
                "the stream ({river}) sits above sea level ({sea})"
            );
            mouths += 1;
        }
    }
    assert!(
        mouths > 0,
        "Azeroth_33_33 carries river-mouth chunks with a river and a sea layer"
    );
}

/// The depth channel through each lane's table: Baradin Bay's shelf ocean (`Azeroth_41_41`) ramps
/// through the middle of its range rather than pinning, and Elwynn's river channels saturate.
#[test]
fn installed_shore_ocean_ramps_and_river_channels_saturate() {
    let Some(mut chain) = wrath_chain() else {
        return;
    };
    let ocean: Vec<f32> = azeroth_layers(&mut chain, 41, 41)
        .iter()
        .filter(|l| l.policy.query_kind == Some(LiquidKind::Ocean))
        .flat_map(|l| l.inputs.depths.clone())
        .collect();
    let mid = ocean.iter().filter(|d| (0.15..0.85).contains(*d)).count();
    eprintln!(
        "Azeroth_41_41 ocean: {} depths, {mid} mid-band",
        ocean.len()
    );
    assert!(
        mid > 100,
        "the shelf carries a real ramp, saw {mid} mid-band depths"
    );
    let river: Vec<f32> = azeroth_layers(&mut chain, 32, 48)
        .iter()
        .flat_map(|l| l.inputs.depths.clone())
        .collect();
    assert!(
        river.iter().any(|&d| d >= 0.999),
        "a river channel middle saturates to the deep row"
    );
}
