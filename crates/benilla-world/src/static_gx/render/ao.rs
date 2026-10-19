//! AO-only camera coverage for retained geometry. Classic keeps the stock forward path.
use super::*;
use bevy::core_pipeline::prepass::{ViewPrepassTextures, NORMAL_PREPASS_FORMAT};
use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionResources};

#[derive(Component)]
pub(super) struct GxAoBind(pub BindGroup);

#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
pub(super) fn prepare_bind_groups(
    mut commands: Commands,
    views: Query<
        (
            Entity,
            &Msaa,
            Option<&ScreenSpaceAmbientOcclusionResources>,
            Has<ScreenSpaceAmbientOcclusion>,
        ),
        With<StaticGxView>,
    >,
    pipes: Res<GxPipelines>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
) {
    for (entity, msaa, resources, enabled) in &views {
        let Some(resources) = resources.filter(|_| enabled && *msaa == Msaa::Off) else {
            commands.entity(entity).remove::<GxAoBind>();
            continue;
        };
        commands.entity(entity).insert(GxAoBind(
            device.create_bind_group(
                "static_gx_ao",
                &cache.get_bind_group_layout(&pipes.ao_layout),
                &BindGroupEntries::single(
                    &resources
                        .screen_space_ambient_occlusion_texture
                        .default_view,
                ),
            ),
        ));
    }
}

pub(super) fn prepare_pipelines(
    views: Query<(&Msaa, Has<ScreenSpaceAmbientOcclusion>), With<StaticGxView>>,
    mut pipes: ResMut<GxPipelines>,
    cache: Res<PipelineCache>,
) {
    if !views
        .iter()
        .any(|(msaa, enabled)| enabled && *msaa == Msaa::Off)
    {
        return;
    }
    // Clone the canonical forward descriptors, preserving vertex layout, culling and reverse-Z.
    // This map is intentionally rebuilt only when the forward specialization changes.
    for (key, id) in pipes.pipelines.clone() {
        // Fresh IDs are only in PipelineCache's pending queue. Descriptor lookup would
        // panic until process_queue has transferred them; a ready pipeline proves that.
        if cache.get_render_pipeline(id).is_none() {
            continue;
        }
        if pipes.normal_pipelines.contains_key(&key) {
            continue;
        }
        let descriptor = cache.get_render_pipeline_descriptor(id);
        let mut normal = descriptor.clone();
        normal.label = Some("static_gx_camera_normal".into());
        let fragment = normal.fragment.as_mut().expect("GX fragment");
        fragment.entry_point = Some("camera_normal".into());
        fragment.targets = vec![Some(ColorTargetState {
            format: NORMAL_PREPASS_FORMAT,
            blend: None,
            write_mask: ColorWrites::ALL,
        })];
        let mut forward = descriptor.clone();
        forward.label = Some("static_gx_ao".into());
        forward.layout.push(pipes.ao_layout.clone());
        forward
            .fragment
            .as_mut()
            .expect("GX fragment")
            .shader_defs
            .push("GX_AO".into());
        if !pipes.normal_pipelines.contains_key(&key) {
            pipes
                .normal_pipelines
                .insert(key, cache.queue_render_pipeline(normal));
            pipes
                .ao_pipelines
                .insert(key, cache.queue_render_pipeline(forward));
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(super) struct GxNormalLabel;
#[derive(Default)]
pub(super) struct GxNormalNode;
impl ViewNode for GxNormalNode {
    type ViewQuery = (
        &'static ViewDepthTexture,
        &'static ViewPrepassTextures,
        &'static ViewUniformOffset,
        &'static StaticGxView,
        &'static GxAoBind,
    );
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (depth, prepass, view_offset, _, _): QueryItem<'w, '_, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let gx = world.resource::<GxWorld>();
        if gx.visible.is_empty() && gx.visible_wmos.is_empty() {
            return Ok(());
        }
        let cache = world.resource::<GxGpuCache>();
        let pipes = world.resource::<GxPipelines>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let Some(view_bind) = world.get_resource::<GxViewBind>() else {
            return Ok(());
        };
        let meshes = world.resource::<RenderAssets<RenderMesh>>();
        let allocator = world.resource::<MeshAllocator>();
        let resolved = resolve_regions(gx, cache);
        if resolved.is_empty() {
            return Ok(());
        }
        // All four pipelines or none: a region drawing only its opaque half would flash its
        // cutout content off for a frame.
        let mut ready: HashMap<(bool, bool), &RenderPipeline> = HashMap::default();
        let selected_pipelines = &pipes.normal_pipelines;
        if selected_pipelines.len() != 4 {
            return Ok(());
        }
        for (k, id) in selected_pipelines {
            match pipeline_cache.get_render_pipeline(*id) {
                Some(p) => {
                    ready.insert(*k, p);
                }
                None => return Ok(()),
            }
        }
        // Never contribute invisible occluders while the matching AO colour pipelines compile.
        if pipes.ao_pipelines.len() != 4
            || pipes
                .ao_pipelines
                .values()
                .any(|id| pipeline_cache.get_render_pipeline(*id).is_none())
        {
            return Ok(());
        }
        let depth_attachment = depth.get_attachment(StoreOp::Store);
        let Some(normal) = prepass.normal.as_ref() else {
            return Ok(());
        };
        let Some(prepass_depth) = prepass.depth.as_ref() else {
            return Ok(());
        };
        let color_attachment = normal.get_attachment();
        // `render/static_gx/elapsed_gpu` on Vulkan and DX12: the perf journal's `gpu_static`.
        let diagnostics = render_context.diagnostic_recorder();
        let mut pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("static_gx_camera_normal"),
            color_attachments: &[Some(color_attachment)],
            depth_stencil_attachment: Some(depth_attachment),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let span = diagnostics.pass_span(&mut pass, "static_gx");
        pass.set_bind_group(0, &view_bind.0, &[view_offset.offset]);

        draw_regions(&mut pass, &resolved, &ready, meshes, allocator);
        span.end(&mut pass);
        drop(pass);
        // Bevy copied this before our pass; refresh the SSAO input after the GX draws.
        render_context.command_encoder().copy_texture_to_texture(
            depth.texture.as_image_copy(),
            prepass_depth.texture.texture.as_image_copy(),
            prepass.size,
        );
        Ok(())
    }
}
