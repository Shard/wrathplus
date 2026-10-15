//! Test-only production specialization bridge and fixture buffer builders.
use super::*;

pub(super) fn load_module(
    device: &RenderDevice,
    source: ShaderCacheSource<'_>,
    _: &ValidateShader,
) -> Result<ShaderModule, PipelineCacheError> {
    let source = match source {
        ShaderCacheSource::Naga(module) => ShaderSource::Naga(Cow::Owned(module)),
        ShaderCacheSource::Wgsl(source) => ShaderSource::Wgsl(Cow::Owned(source)),
        ShaderCacheSource::SpirV(_) => panic!("WoW shadow smoke expects WGSL"),
    };
    Ok(
        device.create_and_validate_shader_module(ShaderModuleDescriptor {
            label: Some("wow_shadow_gpu_smoke"),
            source,
        }),
    )
}

// The same erased-to-typed bridge used by Bevy's material preparation. In particular this
// invokes ExtendedMaterial::specialize (StandardMaterial AND WowModelExt), not just the hook.
pub(super) fn specialize_material(
    pipeline: &MaterialPipeline,
    descriptor: &mut RenderPipelineDescriptor,
    layout: &MeshVertexBufferLayoutRef,
    key: ErasedMaterialPipelineKey,
) -> Result<(), SpecializedMeshPipelineError> {
    WowModelMaterial::specialize(
        pipeline,
        descriptor,
        layout,
        MaterialPipelineKey {
            mesh_key: key.mesh_key,
            bind_group_data: key.material_key.to_key(),
        },
    )
}

/// Only fixture buffer contents are manufactured here. Layouts and binding types always come
/// from the specialized production descriptor. Unexpected texture/array entries fail loudly.
pub(super) fn fixture_group(
    device: &RenderDevice,
    cache: &PipelineCache,
    descriptor: &BindGroupLayoutDescriptor,
    view: bool,
    mesh: bool,
) -> (BindGroup, Vec<u32>) {
    let mut offsets = Vec::new();
    let buffers: Vec<_> = descriptor
        .entries
        .iter()
        .map(|entry| {
            let BindingType::Buffer {
                ty,
                has_dynamic_offset,
                min_binding_size,
            } = entry.ty
            else {
                panic!("unexpected non-buffer fixture entry: {entry:?}");
            };
            assert!(entry.count.is_none(), "fixture does not support arrays");
            if has_dynamic_offset {
                offsets.push(0);
            }
            let size = min_binding_size.map_or(4096, |size| size.get().max(4096));
            let mut bytes = vec![0u8; size as usize];
            if view && entry.binding == 0 {
                // ViewUniform's seven leading matrices, identity orthographic light view.
                for matrix in 0..7 {
                    for diagonal in 0..4 {
                        let offset = matrix * 64 + diagonal * 20;
                        bytes[offset..offset + 4].copy_from_slice(&1f32.to_ne_bytes());
                    }
                }
            }
            if mesh && entry.binding == 0 {
                // MeshUniform's leading transposed affine world_from_local, instance zero.
                for offset in [0, 20, 40] {
                    bytes[offset..offset + 4].copy_from_slice(&1f32.to_ne_bytes());
                }
            }
            device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("shadow fixture buffer"),
                contents: &bytes,
                usage: match ty {
                    BufferBindingType::Uniform => BufferUsages::UNIFORM,
                    _ => BufferUsages::STORAGE,
                },
            })
        })
        .collect();
    let entries: Vec<_> = descriptor
        .entries
        .iter()
        .zip(&buffers)
        .map(|(entry, buffer)| BindGroupEntry {
            binding: entry.binding,
            resource: buffer.as_entire_binding(),
        })
        .collect();
    (
        device.create_bind_group(
            "shadow fixture group",
            &cache.get_bind_group_layout(descriptor),
            &entries,
        ),
        offsets,
    )
}
