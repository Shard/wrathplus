//! On-demand device validation only: no window, game, archives, network or world plugins.
use super::*;
use bevy::render::renderer::initialize_renderer;
use bevy::render::settings::{Backends, WgpuSettings};
use bevy::shader::{PipelineCacheError, ShaderCache, ShaderCacheSource, ValidateShader};
use std::borrow::Cow;

fn load_module(
    device: &RenderDevice,
    source: ShaderCacheSource<'_>,
    _: &ValidateShader,
) -> Result<ShaderModule, PipelineCacheError> {
    let source = match source {
        ShaderCacheSource::Naga(module) => ShaderSource::Naga(Cow::Owned(module)),
        ShaderCacheSource::Wgsl(source) => ShaderSource::Wgsl(Cow::Owned(source)),
        ShaderCacheSource::SpirV(_) => panic!("GX smoke expects WGSL"),
    };
    Ok(
        device.create_and_validate_shader_module(ShaderModuleDescriptor {
            label: Some("gx_gpu_smoke"),
            source,
        }),
    )
}

/// Explicitly ignored so GPU-less CI and ordinary unit tests never initialize a device.
/// Run `cargo test -p benilla-world --lib retained_gx_gpu_smoke -- --ignored --nocapture`.
#[test]
#[ignore = "on-demand real GPU pipeline and tiny-draw validation; no game client"]
fn retained_gx_gpu_smoke() {
    let resources = futures_lite::future::block_on(initialize_renderer(
        Backends::all(),
        None,
        &WgpuSettings::default(),
    ));
    let device = resources.0;
    let queue = resources.1;
    println!("GX GPU smoke adapter: {:?}", resources.2 .0);
    // Construct the production layouts, not an auto-inferred substitute.
    let mut app = App::new();
    app.insert_resource(device.clone())
        .add_systems(Update, init_pipelines);
    app.update();
    let pipes = app.world().resource::<GxPipelines>();
    let layout = |desc: &BindGroupLayoutDescriptor| {
        device.create_bind_group_layout(Some(desc.label.as_ref()), &desc.entries)
    };
    let view_layout = layout(&pipes.view_layout);
    let cell_layout = layout(&pipes.cell_layout);
    let ao_layout = layout(&pipes.ao_layout);
    let (mut cache, mut assets) = crate::shaders::tests::library_cache(ShaderCache::new(
        device.features(),
        DownlevelFlags::all(),
        load_module,
    ));
    let shader = Shader::from_wgsl(include_str!("../../shaders/static_gx.wgsl"), "gx_gpu_smoke");
    let id = assets.add(shader.clone()).id();
    cache.set_shader(id, shader);

    let buffer = |label: &'static str, size, usage| {
        device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: false,
        })
    };
    // Zero-filled buffers are safe even on the killed/interior branches. Set the view's
    // clip/view matrices explicitly so the submitted triangle actually reaches rasterization.
    let mut view_bytes = vec![0u8; ViewUniform::min_size().get() as usize];
    for matrix in 0..7 {
        // the seven leading mat4s in ViewUniform
        for diagonal in 0..4 {
            let offset = matrix * 64 + (diagonal * 5) * 4;
            view_bytes[offset..offset + 4].copy_from_slice(&1f32.to_ne_bytes());
        }
    }
    let view = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gx_smoke_view"),
        contents: &view_bytes,
        usage: BufferUsages::UNIFORM,
    });
    let light = buffer(
        "gx_smoke_light",
        (22 + 512 + 57344) * 16,
        BufferUsages::STORAGE,
    );
    let cell = buffer("gx_smoke_cell", 16, BufferUsages::UNIFORM);
    let records = buffer("gx_smoke_records", 16, BufferUsages::STORAGE);
    let texture = |label: &'static str, format, usage| {
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
            format,
            usage,
            view_formats: &[],
        })
    };
    let albedo = texture(
        "gx_smoke_albedo",
        TextureFormat::Rgba8Unorm,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    queue.write_texture(
        albedo.as_image_copy(),
        &[255; 64],
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: Some(4),
        },
        Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
    );
    let albedo_view = albedo.create_view(&TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    let ao = texture(
        "gx_smoke_ao",
        TextureFormat::R16Float,
        TextureUsages::TEXTURE_BINDING,
    );
    let ao_view = ao.create_view(&default());
    let view_bind = device.create_bind_group(
        "gx_smoke_view",
        &view_layout,
        &BindGroupEntries::sequential((view.as_entire_binding(), light.as_entire_binding())),
    );
    let cell_bind = device.create_bind_group(
        "gx_smoke_cell",
        &cell_layout,
        &BindGroupEntries::sequential((
            cell.as_entire_binding(),
            records.as_entire_binding(),
            &albedo_view,
            &pipes.sampler,
            &pipes.sampler_clamp,
        )),
    );
    let ao_bind = device.create_bind_group(
        "gx_smoke_ao",
        &ao_layout,
        &BindGroupEntries::single(&ao_view),
    );
    let mut vertices = vec![0u8; 3 * 64];
    for (i, position) in [[-1f32, -1., 0.5], [3., -1., 0.5], [-1., 3., 0.5]]
        .iter()
        .enumerate()
    {
        vertices[i * 64..i * 64 + 12].copy_from_slice(bytemuck::cast_slice(position));
        vertices[i * 64 + 16..i * 64 + 20].copy_from_slice(&1f32.to_ne_bytes());
        // WORD_TEXTURED | WRAP_X | WRAP_Y exercises both sampler bindings and cutout.
        vertices[i * 64 + 48..i * 64 + 52]
            .copy_from_slice(&((1u32 << 21) | (1 << 16) | (1 << 17)).to_ne_bytes());
    }
    let vertex_buffer = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gx_smoke_triangle"),
        contents: &vertices,
        usage: BufferUsages::VERTEX,
    });
    for cutout in [false, true] {
        for two_sided in [false, true] {
            for normal in [false, true] {
                let mut defs = Vec::new();
                if cutout {
                    defs.push("GX_CUTOUT".into());
                }
                if !normal {
                    defs.push("GX_AO".into());
                }
                let module = cache
                    .get(&device, 0, id, &defs)
                    .expect("GX GPU shader composition");
                let groups = if normal {
                    vec![&*view_layout, &*cell_layout]
                } else {
                    vec![&*view_layout, &*cell_layout, &*ao_layout]
                };
                let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: Some("gx_smoke"),
                    bind_group_layouts: &groups,
                    push_constant_ranges: &[],
                });
                let vertex_layout = vertex_layout();
                let buffers = [RawVertexBufferLayout {
                    array_stride: vertex_layout.array_stride,
                    step_mode: vertex_layout.step_mode,
                    attributes: &vertex_layout.attributes,
                }];
                let format = if normal {
                    bevy::core_pipeline::prepass::NORMAL_PREPASS_FORMAT
                } else {
                    TextureFormat::Rgba8Unorm
                };
                let pipeline = device.create_render_pipeline(&RawRenderPipelineDescriptor {
                    label: Some("gx_smoke"),
                    layout: Some(&pipeline_layout),
                    vertex: RawVertexState {
                        module: &module,
                        entry_point: Some("vertex"),
                        compilation_options: default(),
                        buffers: &buffers,
                    },
                    fragment: Some(RawFragmentState {
                        module: &module,
                        entry_point: Some(if normal { "camera_normal" } else { "fragment" }),
                        compilation_options: default(),
                        targets: &[Some(ColorTargetState {
                            format,
                            blend: None,
                            write_mask: ColorWrites::ALL,
                        })],
                    }),
                    primitive: PrimitiveState {
                        cull_mode: (!two_sided).then_some(Face::Back),
                        ..default()
                    },
                    depth_stencil: Some(DepthStencilState {
                        format: CORE_3D_DEPTH_FORMAT,
                        depth_write_enabled: true,
                        depth_compare: CompareFunction::GreaterEqual,
                        stencil: default(),
                        bias: default(),
                    }),
                    multisample: default(),
                    multiview: None,
                    cache: None,
                });
                let color = texture("gx_smoke_output", format, TextureUsages::RENDER_ATTACHMENT);
                let color_view = color.create_view(&default());
                let depth = texture(
                    "gx_smoke_depth",
                    CORE_3D_DEPTH_FORMAT,
                    TextureUsages::RENDER_ATTACHMENT,
                );
                let depth_view = depth.create_view(&default());
                let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("gx_smoke"),
                });
                {
                    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("gx_smoke"),
                        color_attachments: &[Some(RenderPassColorAttachment {
                            view: &color_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: Operations {
                                load: LoadOp::Clear(default()),
                                store: StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                            view: &depth_view,
                            depth_ops: Some(Operations {
                                load: LoadOp::Clear(0.),
                                store: StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &*view_bind, &[0]);
                    pass.set_bind_group(1, &*cell_bind, &[]);
                    if !normal {
                        pass.set_bind_group(2, &*ao_bind, &[]);
                    }
                    pass.set_vertex_buffer(0, *vertex_buffer.slice(..));
                    pass.draw(0..3, 0..1);
                }
                queue.submit([encoder.finish()]);
                device
                    .poll(PollType::wait_indefinitely())
                    .expect("GPU submission completed");
                println!(
                    "GX GPU smoke draw: cutout={cutout} two_sided={two_sided} normal={normal}"
                );
            }
        }
    }
}
