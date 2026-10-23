//! On-demand GPU validation of the production Plus filters and combine descriptors.
use super::*;
use bevy::render::renderer::initialize_renderer;
use bevy::render::settings::{Backends, WgpuSettings};
use bevy::shader::{PipelineCacheError, ShaderCache, ShaderCacheSource, ValidateShader};
use std::borrow::Cow;

#[allow(clippy::result_large_err)] // the error type is the library's (a large enum), returned as-is
fn load_module(
    device: &RenderDevice,
    source: ShaderCacheSource<'_>,
    _: &ValidateShader,
) -> Result<ShaderModule, PipelineCacheError> {
    let source = match source {
        ShaderCacheSource::Naga(module) => ShaderSource::Naga(Cow::Owned(module)),
        ShaderCacheSource::Wgsl(source) => ShaderSource::Wgsl(Cow::Owned(source)),
        ShaderCacheSource::SpirV(_) => panic!("FFX smoke expects WGSL"),
    };
    Ok(
        device.create_and_validate_shader_module(ShaderModuleDescriptor {
            label: Some("ffx_plus_gpu_smoke"),
            source,
        }),
    )
}

#[test]
#[ignore = "on-demand real GPU tiny-pass validation; no game client"]
fn ffx_plus_gpu_smoke() {
    let resources = futures_lite::future::block_on(initialize_renderer(
        Backends::all(),
        None,
        &WgpuSettings::default(),
    ));
    let device = resources.0;
    let queue = resources.1;
    println!("FFX Plus GPU smoke adapter: {:?}", resources.2 .0);
    let (mut shaders, assets) = crate::shaders::tests::library_cache(ShaderCache::new(
        device.features(),
        DownlevelFlags::all(),
        load_module,
    ));
    // Embedding plugins only: never create a window, render app, world, or network.
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin {
                primary_window: None,
                ..default()
            })
            .set(bevy::render::RenderPlugin {
                render_creation: bevy::render::settings::RenderCreation::Automatic(WgpuSettings {
                    backends: None,
                    ..default()
                }),
                ..default()
            }),
    );
    app.add_plugins(crate::shaders::plugin);
    app.finish();
    app.cleanup();
    app.insert_resource(device.clone())
        .insert_resource(queue.clone())
        .insert_resource(PipelineCache::new(device.clone(), resources.3, true))
        .init_resource::<FullscreenShader>()
        .add_systems(Update, init_pipelines);
    app.update();
    // init_pipelines queues descriptors; publish the queue before inspecting them.
    // Shader compilation below uses the embedded-library cache, not async asset IO.
    app.world_mut()
        .resource_mut::<PipelineCache>()
        .process_queue();
    let pipes = app.world().resource::<FfxGlowPipelines>();
    let cache = app.world().resource::<PipelineCache>();
    let fullscreen = assets
        .iter()
        .find(|(_, shader)| shader.path.ends_with("fullscreen.wgsl"))
        .expect("embedded production fullscreen shader")
        .1
        .clone();
    shaders.set_shader(pipes.combine.fullscreen.shader().id(), fullscreen);
    for (handle, source) in [
        (
            &pipes.combine.plus_shader,
            include_str!("../shaders/ffx_plus.wgsl"),
        ),
        (
            &pipes.combine.shader,
            include_str!("../shaders/ffx_glow.wgsl"),
        ),
    ] {
        shaders.set_shader(handle.id(), Shader::from_wgsl(source, "ffx_smoke"));
    }
    let texture = |label, usage| {
        device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: ViewTarget::TEXTURE_FORMAT_HDR,
            usage,
            view_formats: &[],
        })
    };
    let input = texture(
        "ffx_smoke_input",
        TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
    );
    let input_view = input.create_view(&default());
    let output = texture("ffx_smoke_output", TextureUsages::RENDER_ATTACHMENT);
    let output_view = output.create_view(&default());
    let uniform = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("ffx_smoke_uniform"),
        contents: bytemuck::cast_slice(&[0.5f32, 0., 0.2, 0., 0.1, 0.2, 0., 0.]),
        usage: BufferUsages::UNIFORM,
    });
    let mut descriptors = Vec::new();
    for id in [
        pipes.plus_downsample,
        pipes.plus_gauss_h,
        pipes.plus_gauss_v,
    ] {
        descriptors.push((cache.get_render_pipeline_descriptor(id).clone(), false));
    }
    // Every feature subset, wet/dry and gamma encoding, through production specialization.
    for bits in 1..8 {
        for wave in [false, true] {
            for gamma_out in [false, true] {
                descriptors.push((
                    pipes.combine.specialize(FfxCombineKey {
                        format: ViewTarget::TEXTURE_FORMAT_HDR,
                        gamma_out,
                        inside_2d: None,
                        wave,
                        plus: [bits & 1 != 0, bits & 2 != 0, bits & 4 != 0],
                    }),
                    true,
                ));
            }
        }
    }
    for (index, (desc, combine)) in descriptors.iter().enumerate() {
        let layout_desc = &desc.layout[0];
        let layout =
            device.create_bind_group_layout(Some(layout_desc.label.as_ref()), &layout_desc.entries);
        let bind = if *combine {
            device.create_bind_group(
                Some("ffx_smoke_combine"),
                &layout,
                &BindGroupEntries::sequential((
                    &input_view,
                    &pipes.sampler,
                    &input_view,
                    uniform.as_entire_binding(),
                    &pipes.wave_view,
                    &pipes.wave_sampler,
                )),
            )
        } else {
            device.create_bind_group(
                Some("ffx_smoke_filter"),
                &layout,
                &BindGroupEntries::sequential((&input_view, &pipes.sampler)),
            )
        };
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ffx_smoke"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let fragment = desc.fragment.as_ref().unwrap();
        let vertex_module = shaders
            .get(
                &device,
                0,
                desc.vertex.shader.id(),
                &desc.vertex.shader_defs,
            )
            .expect("production fullscreen vertex module");
        let fragment_module = shaders
            .get(&device, 1, fragment.shader.id(), &fragment.shader_defs)
            .expect("production FFX Plus module");
        let pipeline = device.create_render_pipeline(&RawRenderPipelineDescriptor {
            label: desc.label.as_deref(),
            layout: Some(&pipeline_layout),
            vertex: RawVertexState {
                module: &vertex_module,
                entry_point: desc.vertex.entry_point.as_deref(),
                compilation_options: default(),
                buffers: &[],
            },
            fragment: Some(RawFragmentState {
                module: &fragment_module,
                entry_point: fragment.entry_point.as_deref(),
                compilation_options: default(),
                targets: &fragment.targets,
            }),
            primitive: desc.primitive,
            depth_stencil: desc.depth_stencil.clone(),
            multisample: desc.multisample,
            multiview: None,
            cache: None,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("ffx_smoke"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("ffx_smoke"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: Operations {
                        load: LoadOp::Clear(default()),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &*bind, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        device
            .poll(PollType::wait_indefinitely())
            .expect("FFX GPU submission completed");
        println!(
            "FFX Plus GPU smoke draw {index}: {:?} {:?}",
            fragment.entry_point, fragment.shader_defs
        );
    }
    assert_eq!(descriptors.len(), 31);
}
