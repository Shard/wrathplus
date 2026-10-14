//! Opt-in world shadow sun. It supplies shadow maps, never additional scene illumination.

use bevy::camera::visibility::RenderLayers;
use bevy::light::{CascadeShadowConfigBuilder, SimulationLightSystems};
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::lighting::WowLighting;
use crate::modern_lighting::ModernLighting;
use crate::view::WorldCamera;

/// App policy publishers must run before this set in PostUpdate.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShadowSyncSet;

/// Only the lifecycle's own sun is managed; other lights are left alone.
#[derive(Component)]
struct WorldShadowSun;

pub struct ModernShadowsPlugin;

impl Plugin for ModernShadowsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModernLighting>()
            .configure_sets(
                PostUpdate,
                ShadowSyncSet
                    .before(TransformSystems::Propagate)
                    .before(SimulationLightSystems::UpdateDirectionalLightCascades),
            )
            .add_systems(PostUpdate, sync_shadow_sun.in_set(ShadowSyncSet));
    }
}

fn sun_rotation(travel: Vec3) -> Option<Quat> {
    if !travel.is_finite() {
        return None;
    }
    let direction = travel.try_normalize()?;
    // DirectionalLight emits along local -Z. No look-at up vector means vertical
    // sunlight is safe too; sun_dir is travel, not the opposite to-sun vector.
    Some(Quat::from_rotation_arc(Vec3::NEG_Z, direction))
}

fn sync_shadow_sun(
    mut commands: Commands,
    policy: Res<ModernLighting>,
    cameras: Query<(), With<WorldCamera>>,
    lighting: Option<Res<WowLighting>>,
    suns: Query<Entity, With<WorldShadowSun>>,
) {
    let enabled = policy.shadows && !cameras.is_empty();
    let rotation = lighting.as_deref().and_then(|l| sun_rotation(l.sun_dir));
    let mut retained = false;
    for entity in &suns {
        if !enabled || retained {
            commands.entity(entity).despawn();
        } else {
            retained = true;
            // Keep the last valid orientation during an invalid lighting sample.
            if let Some(rotation) = rotation {
                // Only our owned sun is touched. ApplyDeferred precedes Propagate via
                // ShadowSyncSet, so there is no broad mutable Transform query that
                // conflicts with unrelated rig/conform animation writers.
                commands
                    .entity(entity)
                    .insert(Transform::from_rotation(rotation));
            }
        }
    }
    if enabled && !retained {
        commands.spawn((
            WorldShadowSun,
            DirectionalLight {
                illuminance: 0.0,
                shadows_enabled: true,
                ..default()
            },
            RenderLayers::layer(0),
            CascadeShadowConfigBuilder {
                num_cascades: 2,
                first_cascade_far_bound: 30.0,
                maximum_distance: 120.0,
                ..default()
            }
            .build(),
            Transform::from_rotation(
                rotation.unwrap_or_else(|| sun_rotation(Vec3::NEG_Y).unwrap()),
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::light::CascadeShadowConfig;

    // ECS-only apps: no render plugins, windows, assets or GPU device.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(ModernShadowsPlugin);
        app.insert_resource(WowLighting {
            sun_dir: Vec3::new(0.3, -0.8, 0.5),
            ..default()
        });
        app
    }

    fn suns(app: &mut App) -> Vec<Entity> {
        app.world_mut()
            .query_filtered::<Entity, With<WorldShadowSun>>()
            .iter(app.world())
            .collect()
    }

    #[test]
    fn lifecycle_requires_policy_and_world_camera_and_restores_classic() {
        let mut app = app();
        app.update();
        assert!(suns(&mut app).is_empty());
        app.world_mut().resource_mut::<ModernLighting>().shadows = true;
        // A booth camera is not a world camera.
        app.world_mut().spawn(Camera::default());
        app.update();
        assert!(suns(&mut app).is_empty());
        let camera = app.world_mut().spawn(WorldCamera).id();
        app.update();
        let sun = suns(&mut app);
        assert_eq!(sun.len(), 1);
        let light = app.world().get::<DirectionalLight>(sun[0]).unwrap();
        assert_eq!(light.illuminance, 0.0);
        assert!(light.shadows_enabled);
        assert_eq!(
            app.world().get::<RenderLayers>(sun[0]).unwrap(),
            &RenderLayers::layer(0)
        );
        let cascades = app.world().get::<CascadeShadowConfig>(sun[0]).unwrap();
        assert_eq!(cascades.bounds, vec![30.0, 120.0]);
        app.update();
        assert_eq!(suns(&mut app), sun);
        *app.world_mut().resource_mut::<ModernLighting>() = ModernLighting::default();
        app.update();
        assert!(suns(&mut app).is_empty());
        app.world_mut().resource_mut::<ModernLighting>().shadows = true;
        app.update();
        assert_eq!(suns(&mut app).len(), 1);
        app.world_mut().despawn(camera);
        app.update();
        assert!(suns(&mut app).is_empty());
    }

    #[test]
    fn deduplicates_owned_suns_without_touching_other_lights() {
        let mut app = app();
        app.world_mut().resource_mut::<ModernLighting>().shadows = true;
        app.world_mut().spawn(WorldCamera);
        let unrelated = app.world_mut().spawn(DirectionalLight::default()).id();
        app.update();
        app.world_mut()
            .spawn((WorldShadowSun, Transform::default()));
        app.update();
        assert_eq!(suns(&mut app).len(), 1);
        assert!(app.world().get::<DirectionalLight>(unrelated).is_some());
        app.world_mut().resource_mut::<ModernLighting>().shadows = false;
        app.update();
        assert!(suns(&mut app).is_empty());
        assert!(app.world().get::<DirectionalLight>(unrelated).is_some());
    }

    #[test]
    fn rotation_follows_travel_and_invalid_samples_keep_last_valid_pose() {
        let mut app = app();
        app.world_mut().resource_mut::<ModernLighting>().shadows = true;
        app.world_mut().spawn(WorldCamera);
        for travel in [Vec3::new(0.3, -0.8, 0.5), Vec3::Y, Vec3::NEG_Y, Vec3::Z] {
            app.world_mut().resource_mut::<WowLighting>().sun_dir = travel;
            app.update();
            let sun = suns(&mut app)[0];
            let rotation = app.world().get::<Transform>(sun).unwrap().rotation;
            assert!((rotation * Vec3::NEG_Z - travel.normalize()).length() < 1e-5);
        }
        let sun = suns(&mut app)[0];
        let before = *app.world().get::<Transform>(sun).unwrap();
        for invalid in [
            Vec3::ZERO,
            Vec3::splat(f32::NAN),
            Vec3::splat(f32::INFINITY),
        ] {
            assert!(sun_rotation(invalid).is_none());
            app.world_mut().resource_mut::<WowLighting>().sun_dir = invalid;
            app.update();
            assert_eq!(*app.world().get::<Transform>(sun).unwrap(), before);
        }
    }

    #[test]
    fn invalid_initial_direction_has_finite_fallback() {
        let mut app = app();
        app.world_mut().resource_mut::<ModernLighting>().shadows = true;
        app.world_mut().resource_mut::<WowLighting>().sun_dir = Vec3::ZERO;
        app.world_mut().spawn(WorldCamera);
        app.update();
        let sun = suns(&mut app)[0];
        let rotation = app.world().get::<Transform>(sun).unwrap().rotation;
        assert!(rotation.is_finite());
        assert!((rotation * Vec3::NEG_Z - Vec3::NEG_Y).length() < 1e-5);
    }

    #[test]
    fn policy_published_before_sync_applies_in_the_same_frame() {
        let mut app = app();
        app.world_mut().spawn(WorldCamera);
        app.add_systems(
            PostUpdate,
            (|mut policy: ResMut<ModernLighting>| policy.shadows = true).before(ShadowSyncSet),
        );
        app.update();
        assert_eq!(suns(&mut app).len(), 1);
    }
}
