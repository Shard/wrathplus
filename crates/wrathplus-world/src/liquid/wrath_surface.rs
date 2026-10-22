//! Native Wrath MH2O surfaces: one lane per LiquidType row, resolved once from the boot chain's
//! LiquidType/LiquidMaterial catalogs, and the per-tile spawn. MH2O never becomes an MCLQ
//! [`wrathplus_formats::LiquidMesh`] or a legacy nibble; it shares only the WGSL arms.
//!
//! The shipped `psLiquidWater` combiner is `rgb = primary·tex0 + tex1.rgb + tex1.a·(secondary +
//! 0.25)`, `alpha = primary.a·tex0.a` (stage 0 the depth swatch, stage 1 the animated sheet),
//! the same equation as the legacy `ocean0_s` arm `liquid.wgsl` already runs for MCLQ.
//! `psLiquidMagma` is `rgb = primary·tex0`, `alpha = 1`, the fullbright arm. Native setup
//! `0x8a2e20` builds `proceduralRiverDepthTex` (`0x8a2bf0` pair 1) and `proceduralOceanDepthTex`
//! (pair 0, last row darkened) as 2×64 swatches with the legacy fill, and LiquidType string
//! word 16 names which one a type binds; so the swatch follows that word, never the category.
//!
//! Material selector routing is derived from the installed joins (water/ocean types join
//! material 1, selector 0; magma/slime material 2, selector 1), not from a traced shader
//! dispatch. Other selectors warn once each and draw nothing.
//!
//! Unresolved, labelled: the frame clock is the shared 24 fps flip; water rows' word 42 (1250)
//! agrees with 30 frames at 24 fps but its native reader is not traced, and magma rows hold 0.
//! Placement keeps MCNK header XY, not native origin reconstruction `0x7c64b0`. The ambient
//! sound class is LiquidType word 3; the native loop reads the type's own SoundEntries word 4.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;

use super::query::{FoamPatch, LiquidSource, WaterChunkInfo};
use super::surface::{
    liquid_ext, load_frame_array, trimesh_collider, LiquidPath, LiquidSoundSource, LiquidSurface,
};
use super::wrath_inputs::{depth_lut, mesh_inputs, texture_filenames, WrathMeshInputs};
use crate::collision::liquid_layers;
use avian3d::prelude::RigidBody;
use wrathplus_assets::coords::wow_to_bevy;
use wrathplus_assets::materials::LiquidMaterial;
use wrathplus_assets::WorldAssets;
use wrathplus_formats::wrath_liquid::{
    LiquidCatalogs, LiquidMaterial as LiquidMaterialRow, LiquidType,
};
use wrathplus_formats::wrath_liquid_surface::WrathLiquidSurface;
use wrathplus_formats::LiquidKind;

/// LiquidType word 16's swatch name for the ocean pair; any other name takes the river pair.
const OCEAN_DEPTH_TEX: &str = "proceduralOceanDepthTex";

/// Which WGSL arm a LiquidType draws with, from its joined material's render selector.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum WrathRender {
    /// Selector 0, `psLiquidWater`: blended, depth swatch plus animated sheet.
    Water { ocean_swatch: bool },
    /// Selector 1, `psLiquidMagma`: the opaque sheet as body.
    Fullbright,
}

/// A type's resolved draw policy, independent of Bevy: render arm, sheet pattern, depth table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WrathLanePolicy {
    pub(crate) render: WrathRender,
    /// LiquidType word 15, expanded by [`texture_filenames`].
    pub(crate) texture: String,
    pub(crate) material_selector: u32,
    /// LiquidType word 41, the depth-table index of `0x79b870`.
    pub(crate) depth_table: u32,
    /// LiquidType word 3; `None` keeps the surface out of liquid queries rather than guessing.
    pub(crate) query_kind: Option<LiquidKind>,
}

/// Why a type has no lane.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NoLane {
    MissingMaterial,
    UnsupportedSelector(u32),
    Invalid(String),
}

/// Resolve one LiquidType row through its material join; no asset access.
pub(crate) fn lane_policy(
    ty: &LiquidType,
    material: Option<&LiquidMaterialRow>,
) -> Result<WrathLanePolicy, NoLane> {
    let material = material.ok_or(NoLane::MissingMaterial)?;
    let selector = material.raw_render_selector();
    let render = match selector {
        0 => WrathRender::Water {
            ocean_swatch: ty.strings[1].eq_ignore_ascii_case(OCEAN_DEPTH_TEX),
        },
        1 => WrathRender::Fullbright,
        other => return Err(NoLane::UnsupportedSelector(other)),
    };
    let depth_table = ty.raw_columns()[41];
    depth_lut(selector, depth_table).map_err(NoLane::Invalid)?;
    texture_filenames(&ty.strings[0]).map_err(NoLane::Invalid)?;
    Ok(WrathLanePolicy {
        render,
        texture: ty.strings[0].clone(),
        material_selector: selector,
        depth_table,
        query_kind: ty.kind(),
    })
}

struct WrathLane {
    material: Handle<LiquidMaterial>,
    policy: WrathLanePolicy,
}

/// The Wrath lanes by raw LiquidType ID; absent outside the Wrath profile.
pub(crate) struct WrathLiquidAssets {
    lanes: HashMap<u32, WrathLane>,
    /// IDs a tile named that have no lane, warned once each.
    warned: Mutex<HashSet<u32>>,
}

/// Build every catalog row's lane, one frame array and material per (pattern, arm).
pub(super) fn build_wrath_lanes(
    catalogs: &LiquidCatalogs,
    world_assets: &mut WorldAssets,
    images: &mut Assets<Image>,
    materials: &mut Assets<LiquidMaterial>,
) -> WrathLiquidAssets {
    let mut shared: HashMap<(String, WrathRender), Option<Handle<LiquidMaterial>>> = HashMap::new();
    let mut warned_selectors = HashSet::new();
    let mut lanes = HashMap::new();
    for ty in catalogs.types() {
        let policy = match lane_policy(ty, catalogs.material_for(ty)) {
            Ok(policy) => policy,
            Err(NoLane::UnsupportedSelector(s)) => {
                if warned_selectors.insert(s) {
                    warn!("liquid: Wrath material selector {s} unsupported; its types will not render");
                }
                continue;
            }
            Err(why) => {
                warn!(
                    "liquid: Wrath LiquidType {} ({}) has no lane: {why:?}",
                    ty.id(),
                    ty.name
                );
                continue;
            }
        };
        let key = (policy.texture.to_ascii_lowercase(), policy.render);
        let material = shared
            .entry(key)
            .or_insert_with(|| {
                let fullbright = policy.render == WrathRender::Fullbright;
                let paths = texture_filenames(&policy.texture).ok()?;
                let Some((frames, count)) =
                    load_frame_array(world_assets, images, &paths, !fullbright)
                else {
                    warn!(
                        "liquid: no frames for {} — its Wrath types will not render",
                        policy.texture
                    );
                    return None;
                };
                let ocean = matches!(policy.render, WrathRender::Water { ocean_swatch: true });
                Some(materials.add(ExtendedMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: if fullbright {
                            AlphaMode::Opaque
                        } else {
                            AlphaMode::Blend
                        },
                        cull_mode: None,
                        double_sided: true,
                        depth_bias: if fullbright {
                            0.0
                        } else {
                            crate::sky_order::WATER_BIAS
                        },
                        ..default()
                    },
                    // The ADT arm: no interior fog, no scroll (no MH2O scroll lane is established).
                    extension: liquid_ext(
                        frames,
                        count,
                        fullbright,
                        ocean,
                        LiquidPath::Adt,
                        false,
                        world_assets.shared_light.clone(),
                    ),
                }))
            })
            .clone();
        if let Some(material) = material {
            lanes.insert(ty.id(), WrathLane { material, policy });
        }
    }
    info!("liquid: {} Wrath liquid type lane(s)", lanes.len());
    WrathLiquidAssets {
        lanes,
        warned: Mutex::new(HashSet::new()),
    }
}

impl WrathLiquidAssets {
    fn lane(&self, id: u32) -> Option<&WrathLane> {
        let lane = self.lanes.get(&id);
        if lane.is_none() && self.warned.lock().is_ok_and(|mut w| w.insert(id)) {
            warn!("liquid: MH2O liquid type {id} has no Wrath lane; its layers will not render");
        }
        lane
    }
}

/// The render mesh: Bevy positions, an up normal, UV0 the sheet, UV1.x the swatch depth.
pub(crate) fn wrath_bevy_mesh(inputs: &WrathMeshInputs) -> Mesh {
    let positions: Vec<[f32; 3]> = inputs
        .positions
        .iter()
        .map(|p| wow_to_bevy(*p).to_array())
        .collect();
    let n = positions.len();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, inputs.uvs.clone());
    let uv1: Vec<[f32; 2]> = inputs.depths.iter().map(|&d| [d, 0.0]).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
    mesh.insert_indices(Indices::U32(inputs.indices.clone()));
    mesh
}

/// Spawn a tile's MH2O layers into `entities`, to despawn with the tile. Positions are already
/// absolute WoW, so the native batch transform's world positions feed the generated UVs directly.
pub(crate) fn spawn_wrath_liquids<'a>(
    commands: &mut Commands,
    surfaces: impl Iterator<Item = &'a WrathLiquidSurface>,
    wrath: &WrathLiquidAssets,
    meshes: &mut Assets<Mesh>,
    entities: &mut Vec<Entity>,
) {
    for surface in surfaces {
        let Some(lane) = wrath.lane(u32::from(surface.layer.liquid_type)) else {
            continue;
        };
        let policy = &lane.policy;
        let inputs = match mesh_inputs(
            surface,
            policy.material_selector,
            policy.depth_table,
            &surface.positions,
        ) {
            Ok(inputs) if !inputs.indices.is_empty() => inputs,
            Ok(_) => continue, // every cell dry
            Err(e) => {
                warn!("liquid: MH2O layer rejected: {e}");
                continue;
            }
        };
        let fullbright = policy.render == WrathRender::Fullbright;
        let mut entity = commands.spawn((
            Mesh3d(meshes.add(wrath_bevy_mesh(&inputs))),
            MeshMaterial3d(lane.material.clone()),
            Transform::IDENTITY,
            LiquidSurface,
            // Like MCLQ: per-tile exterior scene, the exterior cull its only `Visibility` writer.
            crate::exterior_cull::ExteriorScene,
        ));
        if let Some(kind) = policy.query_kind {
            entity.insert((
                adt_water_info(surface, kind),
                LiquidSoundSource {
                    nibble: sound_class(kind),
                },
            ));
            if !fullbright {
                entity.insert(FoamPatch);
            }
        }
        if let Some(collider) = trimesh_collider(&inputs.positions, &inputs.indices) {
            entity.insert((collider, RigidBody::Static, liquid_layers()));
        }
        entities.push(entity.id());
    }
}

/// An MH2O layer's query footprint: its vertex grid and per-cell `exists` mask, as `kind`.
pub(crate) fn adt_water_info(surface: &WrathLiquidSurface, kind: LiquidKind) -> WaterChunkInfo {
    let layer = &surface.layer;
    WaterChunkInfo::new(
        LiquidSource::AdtChunk,
        kind,
        [usize::from(layer.width) + 1, usize::from(layer.height) + 1],
        surface.positions.clone(),
        layer.exists.clone(),
    )
}

/// LiquidType word 3 as the sound class: water 0, ocean 1, magma 2, slime 3.
fn sound_class(kind: LiquidKind) -> u8 {
    match kind {
        LiquidKind::Ocean => 1,
        LiquidKind::Magma => 2,
        LiquidKind::Slime => 3,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::query::{liquid_at, LiquidClaim};
    use wrathplus_adt::{Mh2oLayer, Mh2oVertexFormat};
    use wrathplus_formats::wrath_liquid::parse_liquid_catalogs;
    use wrathplus_formats::wrath_liquid_surface::build_wrath_liquid_surface;

    fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
        let mut b = b"WDBC".to_vec();
        for w in [
            rows.len() as u32,
            N as u32,
            N as u32 * 4,
            strings.len() as u32,
        ] {
            b.extend(w.to_le_bytes());
        }
        for row in rows {
            b.extend(row.iter().flat_map(|w| w.to_le_bytes()));
        }
        b.extend(strings);
        b
    }

    /// String block: 1 "lake", 6 "lake_a.%d.blp", 20 ocean swatch, 44 "bad%s".
    const STRINGS: &[u8] = b"\0lake\0lake_a.%d.blp\0proceduralOceanDepthTex\0bad%s\0";

    fn row(
        id: u32,
        category: u32,
        material: u32,
        texture: u32,
        swatch: u32,
        word41: u32,
    ) -> [u32; 45] {
        let mut r = [0; 45];
        r[0] = id;
        r[1] = 1;
        r[3] = category;
        r[14] = material;
        r[15] = texture;
        r[16] = swatch;
        r[41] = word41;
        r
    }

    fn catalogs() -> LiquidCatalogs {
        let types = [
            row(1, 0, 1, 6, 0, 0),  // water, river swatch
            row(2, 1, 1, 6, 20, 1), // ocean, ocean swatch, linear table
            row(3, 2, 2, 6, 0, 0),  // magma
            row(4, 0, 3, 6, 0, 0),  // selector 2: unsupported
            row(5, 0, 9, 6, 0, 0),  // no material row
            row(6, 0, 1, 6, 0, 2),  // unknown depth table
            row(7, 0, 1, 44, 0, 0), // printf syntax in the texture
            row(8, 9, 1, 6, 0, 0),  // unknown category
        ];
        let materials = [[1, 0, 1], [2, 1, 0], [3, 2, 0]];
        parse_liquid_catalogs(&dbc(&types, STRINGS), &dbc(&materials, b"\0")).unwrap()
    }

    fn policy(c: &LiquidCatalogs, id: u32) -> Result<WrathLanePolicy, NoLane> {
        let ty = c.liquid_type(id).unwrap();
        lane_policy(ty, c.material_for(ty))
    }

    #[test]
    fn selector_routes_the_arm_and_word_16_the_swatch() {
        let c = catalogs();
        let water = policy(&c, 1).unwrap();
        assert_eq!(
            water.render,
            WrathRender::Water {
                ocean_swatch: false
            }
        );
        assert_eq!(water.texture, "lake_a.%d.blp");
        assert_eq!((water.material_selector, water.depth_table), (0, 0));
        assert_eq!(water.query_kind, Some(LiquidKind::Still));
        let ocean = policy(&c, 2).unwrap();
        assert_eq!(ocean.render, WrathRender::Water { ocean_swatch: true });
        assert_eq!(
            (ocean.depth_table, ocean.query_kind),
            (1, Some(LiquidKind::Ocean))
        );
        let magma = policy(&c, 3).unwrap();
        assert_eq!(magma.render, WrathRender::Fullbright);
        assert_eq!(magma.query_kind, Some(LiquidKind::Magma));
        // The category never picks the arm: an unknown one still draws, but answers no query.
        let unknown = policy(&c, 8).unwrap();
        assert_eq!(
            unknown.render,
            WrathRender::Water {
                ocean_swatch: false
            }
        );
        assert_eq!(unknown.query_kind, None);
    }

    #[test]
    fn unsupported_or_malformed_types_get_no_lane() {
        let c = catalogs();
        assert_eq!(policy(&c, 4), Err(NoLane::UnsupportedSelector(2)));
        assert_eq!(policy(&c, 5), Err(NoLane::MissingMaterial));
        assert!(matches!(policy(&c, 6), Err(NoLane::Invalid(_))));
        assert!(matches!(policy(&c, 7), Err(NoLane::Invalid(_))));
    }

    /// A 2×1 layer at `z`, the second cell dry, one depth byte per vertex.
    fn surface(liquid_type: u16, z: f32) -> WrathLiquidSurface {
        let layer = Mh2oLayer {
            liquid_type,
            vertex_format: Mh2oVertexFormat::HeightDepth,
            min_height: z,
            max_height: z,
            offset_x: 1,
            offset_y: 2,
            width: 2,
            height: 1,
            exists: vec![true, false],
            heights: vec![z; 6],
            depths: Some(vec![0, 21, 42, 0, 21, 42]),
            texture_coordinates: None,
        };
        build_wrath_liquid_surface(&layer, None, [100.0, 200.0], [-4.0, -4.0]).unwrap()
    }

    fn lanes(c: &LiquidCatalogs) -> WrathLiquidAssets {
        let lanes = [1, 3, 8]
            .into_iter()
            .map(|id| {
                let lane = WrathLane {
                    material: Handle::default(),
                    policy: policy(c, id).unwrap(),
                };
                (id, lane)
            })
            .collect();
        WrathLiquidAssets {
            lanes,
            warned: Mutex::new(HashSet::new()),
        }
    }

    fn spawn(surfaces: Vec<WrathLiquidSurface>) -> (App, Vec<Entity>) {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Mesh>();
        let wrath = lanes(&catalogs());
        let ents = app
            .world_mut()
            .run_system_once(
                move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>| {
                    let mut ents = Vec::new();
                    spawn_wrath_liquids(
                        &mut commands,
                        surfaces.iter(),
                        &wrath,
                        &mut meshes,
                        &mut ents,
                    );
                    ents
                },
            )
            .expect("the spawn system ran");
        (app, ents)
    }

    #[test]
    fn a_water_layer_spawns_an_exterior_queryable_surface() {
        let (app, ents) = spawn(vec![surface(1, 7.5)]);
        assert_eq!(ents.len(), 1);
        let e = app.world().entity(ents[0]);
        assert!(
            e.contains::<LiquidSurface>() && e.contains::<crate::exterior_cull::ExteriorScene>()
        );
        assert!(e.contains::<FoamPatch>() && e.contains::<avian3d::prelude::Collider>());
        assert_eq!(e.get::<LiquidSoundSource>().unwrap().nibble, 0);
        let info = e.get::<WaterChunkInfo>().unwrap();
        // Rows step X from the origin, columns Y: the wet cell spans x 92..88, y 196..192.
        let hit = liquid_at(
            std::iter::once(info),
            [90.0, 194.0, 0.0],
            LiquidClaim::Outdoors,
        );
        assert_eq!(
            hit.map(|h| (h.surface_z, h.kind)),
            Some((7.5, LiquidKind::Still))
        );
        assert!(liquid_at(
            std::iter::once(info),
            [90.0, 190.0, 0.0],
            LiquidClaim::Outdoors
        )
        .is_none());
        // The mesh carries the river ramp (byte/42) and the generated world-XY sheet.
        let mesh = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&e.get::<Mesh3d>().unwrap().0)
            .unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv1)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("depth lane");
        };
        assert_eq!(uv1[0][0], 0.0);
        assert!((uv1[1][0] - 0.5).abs() < 1e-6);
        assert_eq!(uv1[2][0], 1.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv0)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("sheet lane");
        };
        let scale = f32::from_bits(0x3d75_c290);
        assert_eq!(uv0[0], [92.0 * scale, 196.0 * scale]);
    }

    #[test]
    fn magma_is_fullbright_and_unknown_categories_are_draw_only() {
        let (app, ents) = spawn(vec![surface(3, 1.0), surface(8, 2.0)]);
        assert_eq!(ents.len(), 2);
        let magma = app.world().entity(ents[0]);
        assert!(!magma.contains::<FoamPatch>());
        assert_eq!(magma.get::<LiquidSoundSource>().unwrap().nibble, 2);
        let unknown = app.world().entity(ents[1]);
        assert!(!unknown.contains::<WaterChunkInfo>() && !unknown.contains::<FoamPatch>());
        assert!(unknown.contains::<LiquidSurface>());
    }

    #[test]
    fn laneless_and_dry_layers_spawn_nothing() {
        let mut dry = surface(1, 0.0);
        dry.layer.exists = vec![false, false];
        dry.indices.clear();
        let (_, ents) = spawn(vec![surface(4, 0.0), surface(999, 0.0), dry]);
        assert!(ents.is_empty());
    }
}
