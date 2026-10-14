//! The engine's WGSL, embedded in the binary; the game's shaders are embedded by `benilla-app`.
//! These calls must stay in a file directly under `src/`: `embedded_asset!` derives the served
//! path from `file!()` without normalizing it, so a call from a submodule would serve the shader
//! under that module's directory instead of `embedded://benilla_world/shaders/<name>.wgsl`.

use bevy::prelude::*;

/// Embed the engine's WGSL files under `embedded://benilla_world/shaders/`. The first member of
/// [`crate::world_plugins::WorldPlugins`]; it needs `DefaultPlugins`, which creates the asset
/// registry, added first.
pub(crate) fn plugin(app: &mut App) {
    bevy::asset::embedded_asset!(app, "shaders/sky_vertex.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/sky.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/star.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/cloud.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/celestial.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/ffx_glow.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/ffx_plus.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/wow_effect.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/static_gx.wgsl");
}

#[cfg(test)]
mod tests {
    use bevy::asset::io::AssetSourceId;
    use bevy::prelude::*;

    /// Reads each file on disk back through the embedded source a `ShaderRef` uses, so a shader
    /// that is unregistered, or registered from a submodule, fails here instead of drawing nothing.
    #[test]
    fn every_engine_shader_answers_at_its_embedded_path() {
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.add_plugins(super::plugin);
        let server = app.world().resource::<AssetServer>();
        let source = server.get_source(AssetSourceId::from("embedded")).unwrap();

        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/shaders");
        let mut found = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            if !name.ends_with(".wgsl") {
                continue;
            }
            found += 1;
            let path = format!("benilla_world/shaders/{name}");
            let read = bevy::tasks::block_on(async {
                source.reader().read(std::path::Path::new(&path)).await
            });
            assert!(
                read.is_ok(),
                "embedded://{path} does not resolve — {name} is on disk but not registered in \
                 `shaders::plugin`, or was registered from a file that is not directly under src/"
            );
        }
        // One per `embedded_asset!` line in `plugin`.
        assert_eq!(found, 9, "the engine's shader set changed size");
    }

    /// Use the same composer/import resolution as the renderer, but never create a GPU module.
    fn validate_module(
        _: &(),
        source: bevy::shader::ShaderCacheSource<'_>,
        _: &bevy::shader::ValidateShader,
    ) -> Result<(), bevy::shader::PipelineCacheError> {
        use bevy::shader::ShaderCacheSource;
        let module = match source {
            ShaderCacheSource::Naga(module) => module,
            ShaderCacheSource::Wgsl(source) => naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source))),
            ShaderCacheSource::SpirV(_) => panic!("expected a WGSL shader"),
        };
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("Naga shader validation failed: {error:?}"));
        Ok(())
    }

    /// DefaultPlugins embed Bevy's shader libraries even with rendering disabled. Read that
    /// virtual source, not a developer's Cargo registry: this also works in CI/offline installs.
    fn validation_cache() -> (bevy::shader::ShaderCache<(), ()>, Assets<Shader>) {
        use bevy::asset::io::Reader;
        use bevy::render::{
            render_resource::{DownlevelFlags, WgpuFeatures},
            settings::{RenderCreation, WgpuSettings},
            RenderPlugin,
        };
        use futures_lite::StreamExt;

        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<bevy::winit::WinitPlugin>()
                .set(WindowPlugin {
                    primary_window: None,
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(WgpuSettings {
                        backends: None,
                        ..default()
                    }),
                    ..default()
                }),
        );
        app.add_plugins(super::plugin);
        benilla_assets::materials::register_shaders(&mut app);
        app.finish();
        app.cleanup();

        let server = app.world().resource::<AssetServer>();
        let source = server.get_source(AssetSourceId::from("embedded")).unwrap();
        let mut cache = bevy::shader::ShaderCache::new(
            WgpuFeatures::empty(),
            DownlevelFlags::all(),
            validate_module,
        );
        let mut assets = Assets::<Shader>::default();
        bevy::tasks::block_on(async {
            let mut directories = vec![std::path::PathBuf::new()];
            let mut count = 0;
            while let Some(directory) = directories.pop() {
                let mut entries = source.reader().read_directory(&directory).await.unwrap();
                while let Some(path) = entries.next().await {
                    if source.reader().is_directory(&path).await.unwrap() {
                        directories.push(path);
                        continue;
                    }
                    if path.extension().is_none_or(|ext| ext != "wgsl") {
                        continue;
                    }
                    let mut bytes = Vec::new();
                    source
                        .reader()
                        .read(&path)
                        .await
                        .unwrap()
                        .read_to_end(&mut bytes)
                        .await
                        .unwrap();
                    let mut shader = Shader::from_wgsl(
                        String::from_utf8(bytes).unwrap(),
                        format!("embedded://{}", path.display()),
                    );
                    // These are normally supplied by MeshRenderPlugin's ShaderSettings.
                    // Reading the embedded bytes directly bypasses those loader settings.
                    if path
                        .file_name()
                        .is_some_and(|name| name == "mesh_view_types.wgsl")
                    {
                        shader.shader_defs = vec![
                            bevy::shader::ShaderDefVal::UInt(
                                "MAX_DIRECTIONAL_LIGHTS".into(),
                                bevy::pbr::MAX_DIRECTIONAL_LIGHTS as u32,
                            ),
                            bevy::shader::ShaderDefVal::UInt(
                                "MAX_CASCADES_PER_LIGHT".into(),
                                bevy::pbr::MAX_CASCADES_PER_LIGHT as u32,
                            ),
                        ];
                    }
                    let id = assets.add(shader.clone()).id();
                    cache.set_shader(id, shader);
                    count += 1;
                }
            }
            assert!(
                count > 13,
                "Bevy's embedded shader libraries were not registered"
            );
        });
        (cache, assets)
    }

    #[test]
    fn world_wgsl_validates_without_a_gpu() {
        use bevy::shader::ShaderDefVal;
        let (mut cache, mut assets) = validation_cache();
        let mut check = |name: &str, source: &'static str, defs: Vec<ShaderDefVal>| {
            let shader = Shader::from_wgsl(source, name);
            let id = assets.add(shader.clone()).id();
            cache.set_shader(id, shader);
            cache
                .get(&(), 0, id, &defs)
                .unwrap_or_else(|error| panic!("{name}, defs {defs:?}: {error:?}"));
        };
        // MeshPipeline's desktop forward-pass definitions, plus ExtendedMaterial's group.
        // Keep specialization branches in sync with benilla-assets/src/materials.rs.
        let mesh_defs = || {
            let mut defs: Vec<ShaderDefVal> = [
                "MESH_PIPELINE",
                "VERTEX_POSITIONS",
                "VERTEX_NORMALS",
                "VERTEX_UVS",
                "VERTEX_UVS_A",
                "VERTEX_UVS_B",
                "VERTEX_COLORS",
                "VERTEX_OUTPUT_INSTANCE_INDEX",
                "VIEW_PROJECTION_PERSPECTIVE",
            ]
            .into_iter()
            .map(Into::into)
            .collect();
            defs.push(ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3));
            defs.push(ShaderDefVal::UInt(
                "AVAILABLE_STORAGE_BUFFER_BINDINGS".into(),
                8,
            ));
            defs.push(ShaderDefVal::Int(
                "SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS".into(),
                4,
            ));
            defs
        };
        check(
            "terrain.wgsl",
            include_str!("../../benilla-assets/src/shaders/terrain.wgsl"),
            mesh_defs(),
        );
        check(
            "liquid.wgsl",
            include_str!("../../benilla-assets/src/shaders/liquid.wgsl"),
            mesh_defs(),
        );
        for specialization in [
            &[][..],
            &["WOW_RIG_SKIN"],
            &["WOW_MERGED_FADE"],
            &["WOW_MERGED_FADE", "WOW_MERGED_SLOT"],
            &["WOW_SKY_DEPTH"],
            &["WOW_WATER_CLIP"],
        ] {
            let mut defs = mesh_defs();
            defs.extend(specialization.iter().map(|name| ShaderDefVal::from(*name)));
            check(
                "wow_model.wgsl",
                include_str!("../../benilla-assets/src/shaders/wow_model.wgsl"),
                defs,
            );
        }
        for cutout in [false, true] {
            check(
                "static_gx.wgsl",
                include_str!("shaders/static_gx.wgsl"),
                if cutout {
                    vec!["GX_CUTOUT".into()]
                } else {
                    vec![]
                },
            );
        }
        for blend in [
            "BLEND_ALPHA",
            "BLEND_OPAQUE",
            "BLEND_ALPHAKEY",
            "BLEND_ADD",
            "BLEND_MULTIPLY",
            "BLEND_MOD2X",
        ] {
            for mask in 0..8 {
                let mut defs = vec![blend.into()];
                for (bit, name) in ["DECAL_WORLD_CLIP", "EFFECT_LIT", "WOW_PARTICLE_FLAT"]
                    .into_iter()
                    .enumerate()
                {
                    if mask & (1 << bit) != 0 {
                        defs.push(name.into());
                    }
                }
                check(
                    "wow_effect.wgsl",
                    include_str!("shaders/wow_effect.wgsl"),
                    defs,
                );
            }
        }
        // All eight Plus presets, including all-off, with both output encodings. Absence,
        // rather than Bool(false), matches production's #ifdef-based specialization.
        for mask in 0..8 {
            for gamma_out in [false, true] {
                let mut defs = vec![];
                if gamma_out {
                    defs.push("GAMMA_OUT".into());
                }
                for (bit, name) in ["PLUS_BLOOM", "PLUS_TONE_MAPPING", "PLUS_COLOUR_GRADING"]
                    .into_iter()
                    .enumerate()
                {
                    if mask & (1 << bit) != 0 {
                        defs.push(name.into());
                    }
                }
                check("ffx_plus.wgsl", include_str!("shaders/ffx_plus.wgsl"), defs);
            }
        }
    }
}
