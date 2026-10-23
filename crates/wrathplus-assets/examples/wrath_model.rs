//! Serverless Wrath model-render gate. User-supplied archives stay read-only.
use wrathplus_assets::{ClientDataProfile, M2Model};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Resource)]
struct Gate {
    model: Handle<M2Model>,
    start: Instant,
    spawned: bool,
    shot: bool,
}
fn main() {
    let data = std::env::var("WOW_DATA").expect("WOW_DATA required");
    let profile = wrathplus_formats::DataProfile::Wrath {
        locale: "enUS".into(),
    };
    let mut app = App::new();
    wrathplus_assets::register_mpq_source_profile(&mut app, Path::new(&data), &profile)
        .expect("Wrath archives");
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Wrath model qualification (serverless)".into(),
            resolution: (800, 800).into(),
            ..default()
        }),
        ..default()
    }));
    wrathplus_assets::register_asset_loaders(&mut app);
    app.add_systems(Startup, setup)
        .add_systems(Update, qualify)
        .run();
}
fn setup(mut commands: Commands, server: Res<AssetServer>, profile: Res<ClientDataProfile>) {
    println!("WRATH_MODEL_GATE profile={:?} network=none", profile.0);
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(3.5, 2.0, 4.0).looking_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.7, -0.8, 0.0)),
    ));
    commands.insert_resource(Gate {
        model: server.load("mpq://character/human/male/humanmale.m2"),
        start: Instant::now(),
        spawned: false,
        shot: false,
    });
}
fn qualify(
    mut commands: Commands,
    mut gate: ResMut<Gate>,
    models: Res<Assets<M2Model>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !gate.spawned {
        if let Some(model) = models.get(&gate.model) {
            let mut triangles = 0usize;
            for sub in &model.submeshes {
                triangles += sub.geometry.indices.len() / 3;
                let mesh = wrathplus_assets::submesh_to_static_mesh(&sub.geometry);
                commands.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::srgb(0.65, 0.68, 0.72),
                        cull_mode: None,
                        ..default()
                    })),
                    Transform::default(),
                ));
            }
            println!(
                "WRATH_MODEL_GATE loaded batches={} triangles={} bones={} sequences={}",
                model.submeshes.len(),
                triangles,
                model.skeleton.joints.len(),
                model.sequences.len()
            );
            gate.spawned = true;
        }
    }
    if gate.spawned && !gate.shot && gate.start.elapsed() > Duration::from_secs(8) {
        let out = std::env::var("WRATH_MODEL_SHOT")
            .unwrap_or_else(|_| "/tmp/azerothbench-wrath-model.png".into());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(out));
        gate.shot = true;
    }
    if gate.start.elapsed() > Duration::from_secs(12) {
        println!(
            "WRATH_MODEL_GATE model_loaded={} screenshot_requested={}",
            gate.spawned, gate.shot
        );
        exit.write(if gate.spawned {
            AppExit::Success
        } else {
            AppExit::error()
        });
    }
}
