//! Active C2 Plus shadow/depth-only support for `WowModelExt` (Bevy 0.18.1).
//!
//! Registered as a child of `materials` to read private `WowModelKey` fields. The material
//! embeds `shaders/wow_shadow.wgsl`, selects it through the prepass vertex/fragment hooks, and
//! dispatches `PREPASS_PIPELINE` here before forward blend/depth mutations, returning immediately.
//! The caller owns Plus view gating; Classic forward specialization is unchanged.
//!
//! Scope: depth-only shadow views, non-bindless materials, custom palette skinning, opaque and
//! mask coverage. This is not a deferred, motion-vector, morph or camera normal-prepass shader.
//! Bevy shares its prepass hook with camera prepasses: callers must restrict those lanes before
//! opting in. No new uniforms, storage buffers, bind groups or mesh attributes.
//!
//! Definitions supplied here: `WOW_SHADOW_FRAGMENT` (both stages), `WOW_RIG_SKIN` for joint meshes,
//! and `WOW_SHADOW_EXCLUDE` for unsupported keyed lanes and merged viewer-fade blobs. Extra mesh
//! fields need not be requested to retain the original interleaved buffer stride/offsets.
//!
//! `WOW_SHADOW_SOURCE_OPAQUE` provenance is NOT integrated into the caller or pipeline cache key.
//! It may be supplied only when a fade twin's authored source is known opaque, with that provenance
//! included in the key. `model_flags.y` alone is insufficient: opaque and authored translucent
//! twins both use Blend and lack the opaque-intent marker. Source-cutout twins already carry
//! uniform marker bit 10. Without explicit opaque provenance, unknown Blend twins conservatively
//! do not cast. Never infer provenance from camera fade/tag. zfill/far-side twins always exclude.

use super::WowModelKey;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::Mesh;
use bevy::render::render_resource::{
    FragmentState, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderDefVal;

fn defined(defs: &[ShaderDefVal], name: &str) -> bool {
    defs.iter()
        .any(|def| matches!(def, ShaderDefVal::Bool(n, true) if n == name))
}

fn define(defs: &mut Vec<ShaderDefVal>, name: &str) {
    if !defined(defs, name) {
        defs.push(name.into());
    }
}

/// Rebuild Bevy's PREPASS input layout, never the forward layout (normal/UV locations differ).
/// Caller must already select wow_shadow for the vertex and any existing fragment stage.
/// Preserve ALL prepass depth state, bias, unclipped-depth and attachment state verbatim.
pub(crate) fn specialize(
    descriptor: &mut RenderPipelineDescriptor,
    layout: &MeshVertexBufferLayoutRef,
    key: WowModelKey,
) -> Result<(), SpecializedMeshPipelineError> {
    // Defensive no-op: invoking this on a visible forward pipeline must not corrupt its layout.
    if !defined(&descriptor.vertex.shader_defs, "PREPASS_PIPELINE") {
        return Ok(());
    }
    // Native depth-only Bevy IO has no fragment outputs: PREPASS_FRAGMENT would
    // produce an invalid empty struct. Our shadow shader supplies its own depth output.
    if !defined(
        &descriptor.vertex.shader_defs,
        "UNCLIPPED_DEPTH_ORTHO_EMULATION",
    ) {
        descriptor.vertex.shader_defs.retain(
            |def| !matches!(def, ShaderDefVal::Bool(name, _) if name == "PREPASS_FRAGMENT"),
        );
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.retain(
                |def| !matches!(def, ShaderDefVal::Bool(name, _) if name == "PREPASS_FRAGMENT"),
            );
        }
    }
    let normal_output = defined(
        &descriptor.vertex.shader_defs,
        "NORMAL_PREPASS_OR_DEFERRED_PREPASS",
    );
    let rig = layout.0.contains(crate::ATTRIBUTE_WOW_JOINT_INDEX);
    let mut attrs = vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0)];
    for (attr, location, enabled) in [
        (Mesh::ATTRIBUTE_UV_0, 1, true),
        (Mesh::ATTRIBUTE_UV_1, 2, true),
        (
            Mesh::ATTRIBUTE_NORMAL,
            3,
            normal_output && defined(&descriptor.vertex.shader_defs, "VERTEX_NORMALS"),
        ),
        (
            Mesh::ATTRIBUTE_TANGENT,
            4,
            normal_output && defined(&descriptor.vertex.shader_defs, "VERTEX_TANGENTS"),
        ),
        (Mesh::ATTRIBUTE_COLOR, 7, true),
    ] {
        if enabled && layout.0.contains(attr) {
            attrs.push(attr.at_shader_location(location));
        }
    }
    if rig {
        attrs.push(crate::ATTRIBUTE_WOW_JOINT_INDEX.at_shader_location(10));
        // get_layout reports a missing weight attribute rather than silently producing bad skin.
        attrs.push(crate::ATTRIBUTE_WOW_JOINT_WEIGHT.at_shader_location(11));
    }
    let buffer = layout.0.get_layout(&attrs)?;
    descriptor.vertex.buffers = vec![buffer];

    let excluded = key.clutter
        || key.additive
        || key.no_depth_write
        || key.no_depth_test
        || key.modulate
        || key.modulate2x
        || key.zfill
        || key.far_side
        || key.sky_depth
        || layout.0.contains(crate::ATTRIBUTE_WOW_FADE_SPHERE)
        || layout.0.contains(crate::ATTRIBUTE_WOW_MERGED_SLOT);
    // key.fade deliberately does not exclude a twin: the shader checks source provenance.
    define(&mut descriptor.vertex.shader_defs, "WOW_SHADOW_FRAGMENT");
    if rig {
        define(&mut descriptor.vertex.shader_defs, "WOW_RIG_SKIN");
    }
    if excluded {
        define(&mut descriptor.vertex.shader_defs, "WOW_SHADOW_EXCLUDE");
    }
    // Opaque Bevy prepasses may omit their fragment stage. We still need discards for keyed and
    // per-material exclusions, and for opaque-source/cutout twins. Reuse the caller-selected
    // vertex shader handle (same WGSL), never add a new resource or touch depth state.
    if descriptor.fragment.is_none() {
        descriptor.fragment = Some(FragmentState {
            shader: descriptor.vertex.shader.clone(),
            shader_defs: descriptor.vertex.shader_defs.clone(),
            entry_point: Some("fragment".into()),
            targets: Vec::new(),
        });
    }
    let fragment = descriptor
        .fragment
        .as_mut()
        .expect("fragment inserted above");
    define(&mut fragment.shader_defs, "WOW_SHADOW_FRAGMENT");
    if rig {
        define(&mut fragment.shader_defs, "WOW_RIG_SKIN");
    }
    if excluded {
        define(&mut fragment.shader_defs, "WOW_SHADOW_EXCLUDE");
    }
    // Provenance can be supplied on the vertex defs by the caller; synchronize both stages.
    if defined(&descriptor.vertex.shader_defs, "WOW_SHADOW_SOURCE_OPAQUE") {
        define(&mut fragment.shader_defs, "WOW_SHADOW_SOURCE_OPAQUE");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{MeshVertexBufferLayouts, VertexAttributeValues};
    use bevy::render::render_resource::{
        ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState,
        PrimitiveTopology, StencilState, TextureFormat, VertexFormat,
    };

    fn opaque_key() -> WowModelKey {
        WowModelKey {
            fade: false,
            clutter: false,
            additive: false,
            no_depth_write: false,
            no_depth_test: false,
            modulate: false,
            modulate2x: false,
            zfill: false,
            far_side: false,
            sky_depth: false,
        }
    }

    fn mesh() -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0]]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[1.0, 1.0]]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0, 0.0, 0.0, 1.0]]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]]);
        mesh
    }

    fn layout(mesh: &Mesh) -> MeshVertexBufferLayoutRef {
        mesh.get_mesh_vertex_buffer_layout(&mut MeshVertexBufferLayouts::default())
    }

    fn descriptor() -> RenderPipelineDescriptor {
        let mut descriptor = RenderPipelineDescriptor::default();
        descriptor.vertex.entry_point = Some("vertex".into());
        descriptor.vertex.shader_defs = vec![
            "PREPASS_PIPELINE".into(),
            "VERTEX_NORMALS".into(),
            "VERTEX_TANGENTS".into(),
        ];
        descriptor.depth_stencil = Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState::default(),
            bias: DepthBiasState {
                constant: 7,
                slope_scale: 1.5,
                clamp: 0.25,
            },
        });
        descriptor.primitive.unclipped_depth = true;
        descriptor
    }

    fn assert_stage_def(descriptor: &RenderPipelineDescriptor, name: &str, expected: bool) {
        assert_eq!(
            defined(&descriptor.vertex.shader_defs, name),
            expected,
            "vertex {name}"
        );
        assert_eq!(
            defined(&descriptor.fragment.as_ref().unwrap().shader_defs, name),
            expected,
            "fragment {name}"
        );
    }

    #[test]
    fn forward_is_a_complete_no_op_even_with_malformed_rig() {
        let mut mesh = mesh();
        mesh.insert_attribute(
            crate::ATTRIBUTE_WOW_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(vec![[0; 4]]),
        );
        let mut descriptor = descriptor();
        descriptor.vertex.shader_defs = vec![ShaderDefVal::Bool("PREPASS_PIPELINE".into(), false)];
        descriptor.vertex.buffers = vec![layout(&mesh).0.layout().clone()];
        let before = descriptor.clone();
        let key = WowModelKey {
            no_depth_test: true,
            fade: true,
            ..opaque_key()
        };
        specialize(&mut descriptor, &layout(&mesh), key).unwrap();
        assert_eq!(descriptor, before);
        descriptor.vertex.shader_defs.clear();
        let before = descriptor.clone();
        specialize(&mut descriptor, &layout(&mesh), key).unwrap();
        assert_eq!(descriptor, before);
    }

    #[test]
    fn shadow_rig_keeps_uv_locations_and_interleaved_offsets() {
        let mut mesh = mesh();
        mesh.insert_attribute(
            crate::ATTRIBUTE_WOW_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(vec![[0; 4]]),
        );
        mesh.insert_attribute(
            crate::ATTRIBUTE_WOW_JOINT_WEIGHT,
            vec![[1.0, 0.0, 0.0, 0.0]],
        );
        let layout = layout(&mesh);
        let mut descriptor = descriptor();
        specialize(&mut descriptor, &layout, opaque_key()).unwrap();
        let expected = layout
            .0
            .get_layout(&[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
                Mesh::ATTRIBUTE_UV_1.at_shader_location(2),
                Mesh::ATTRIBUTE_COLOR.at_shader_location(7),
                crate::ATTRIBUTE_WOW_JOINT_INDEX.at_shader_location(10),
                crate::ATTRIBUTE_WOW_JOINT_WEIGHT.at_shader_location(11),
            ])
            .unwrap();
        assert_eq!(descriptor.vertex.buffers, vec![expected]);
        assert_eq!(
            descriptor.vertex.buffers[0].array_stride,
            layout.0.layout().array_stride
        );
        let attrs = &descriptor.vertex.buffers[0].attributes;
        assert_eq!(
            attrs.iter().map(|a| a.shader_location).collect::<Vec<_>>(),
            [0, 1, 2, 7, 10, 11]
        );
        assert_eq!(attrs[1].format, VertexFormat::Float32x2);
        assert_eq!(attrs[2].format, VertexFormat::Float32x2);
        assert_eq!(attrs[4].format, VertexFormat::Uint16x4);
        assert_eq!(attrs[5].format, VertexFormat::Float32x4);
        assert_stage_def(&descriptor, "WOW_RIG_SKIN", true);
        assert_stage_def(&descriptor, "WOW_SHADOW_FRAGMENT", true);
    }

    #[test]
    fn opaque_and_excluded_prepasses_insert_depth_only_fragment() {
        for key in [
            opaque_key(),
            WowModelKey {
                zfill: true,
                ..opaque_key()
            },
        ] {
            let mut descriptor = descriptor();
            specialize(&mut descriptor, &layout(&mesh()), key).unwrap();
            let fragment = descriptor.fragment.as_ref().unwrap();
            assert_eq!(fragment.shader, descriptor.vertex.shader);
            assert_eq!(fragment.entry_point.as_deref(), Some("fragment"));
            assert!(fragment.targets.is_empty());
            assert_eq!(fragment.shader_defs, descriptor.vertex.shader_defs);
            assert_stage_def(&descriptor, "WOW_SHADOW_FRAGMENT", true);
            assert_stage_def(&descriptor, "WOW_SHADOW_EXCLUDE", key.zfill);
            assert_stage_def(&descriptor, "WOW_RIG_SKIN", false);
        }
    }

    #[test]
    fn keyed_exclusions_preserve_prepass_state_not_forward_flags() {
        type KeyCase = (&'static str, fn(&mut WowModelKey));
        let cases: [KeyCase; 9] = [
            ("clutter", |k| k.clutter = true),
            ("additive", |k| k.additive = true),
            ("no_depth_write", |k| k.no_depth_write = true),
            ("no_depth_test", |k| k.no_depth_test = true),
            ("modulate", |k| k.modulate = true),
            ("modulate2x", |k| k.modulate2x = true),
            ("zfill", |k| k.zfill = true),
            ("far_side", |k| k.far_side = true),
            ("sky_depth", |k| k.sky_depth = true),
        ];
        for (name, set) in cases {
            let mut key = opaque_key();
            set(&mut key);
            let mut descriptor = descriptor();
            descriptor.fragment = Some(FragmentState {
                entry_point: Some("existing_fragment".into()),
                shader_defs: vec!["EXISTING_DEF".into()],
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::RED,
                })],
                ..Default::default()
            });
            let before = descriptor.clone();
            specialize(&mut descriptor, &layout(&mesh()), key).unwrap();
            assert_stage_def(&descriptor, "WOW_SHADOW_EXCLUDE", true);
            assert_stage_def(&descriptor, "WOW_SHADOW_FRAGMENT", true);
            assert!(defined(
                &descriptor.fragment.as_ref().unwrap().shader_defs,
                "EXISTING_DEF"
            ));
            // Only input layout and stage definitions may change an existing prepass.
            let mut expected = before;
            expected.vertex.buffers = descriptor.vertex.buffers.clone();
            expected.vertex.shader_defs = descriptor.vertex.shader_defs.clone();
            expected.fragment.as_mut().unwrap().shader_defs =
                descriptor.fragment.as_ref().unwrap().shader_defs.clone();
            assert_eq!(descriptor, expected, "{name}");
            let once = descriptor.clone();
            specialize(&mut descriptor, &layout(&mesh()), key).unwrap();
            assert_eq!(descriptor, once, "idempotent {name}");
        }
    }

    #[test]
    fn merged_attributes_exclude_without_requesting_extra_inputs() {
        for slot in [false, true] {
            let mut mesh = mesh();
            if slot {
                mesh.insert_attribute(crate::ATTRIBUTE_WOW_MERGED_SLOT, vec![0u32]);
            } else {
                mesh.insert_attribute(crate::ATTRIBUTE_WOW_FADE_SPHERE, vec![[0.0; 4]]);
            }
            let layout = layout(&mesh);
            let mut descriptor = descriptor();
            specialize(&mut descriptor, &layout, opaque_key()).unwrap();
            assert_stage_def(&descriptor, "WOW_SHADOW_EXCLUDE", true);
            assert_eq!(
                descriptor.vertex.buffers[0].array_stride,
                layout.0.layout().array_stride
            );
            assert_eq!(descriptor.vertex.buffers[0].attributes.len(), 4);
        }
    }

    #[test]
    fn fade_does_not_invent_opaque_source_provenance() {
        for known_opaque in [false, true] {
            let mut descriptor = descriptor();
            descriptor.fragment = Some(FragmentState::default());
            if known_opaque {
                descriptor
                    .vertex
                    .shader_defs
                    .push("WOW_SHADOW_SOURCE_OPAQUE".into());
            }
            specialize(
                &mut descriptor,
                &layout(&mesh()),
                WowModelKey {
                    fade: true,
                    ..opaque_key()
                },
            )
            .unwrap();
            assert_stage_def(&descriptor, "WOW_SHADOW_EXCLUDE", false);
            assert_stage_def(&descriptor, "WOW_SHADOW_SOURCE_OPAQUE", known_opaque);
        }
    }

    #[test]
    fn missing_rig_weights_returns_bevy_error_without_mutation() {
        let mut mesh = mesh();
        mesh.insert_attribute(
            crate::ATTRIBUTE_WOW_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(vec![[0; 4]]),
        );
        let mut descriptor = descriptor();
        let before = descriptor.clone();
        let error = specialize(&mut descriptor, &layout(&mesh), opaque_key()).unwrap_err();
        assert!(matches!(
            error,
            SpecializedMeshPipelineError::MissingVertexAttribute(_)
        ));
        assert!(error.to_string().contains("Wow_JointWeight"));
        assert_eq!(descriptor, before);
    }
}
