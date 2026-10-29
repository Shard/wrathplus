//! AO owns a boot-only AA choice; runtime profile changes never switch MSAA.
use crate::experience::{Experience, Feature};
use bevy::anti_alias::smaa::{Smaa, SmaaPreset};
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::ecs::system::EntityCommands;
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::prelude::*;
use bevy::render::{renderer::RenderDevice, view::Msaa};
use wrathplus_world::view::WorldCamera;

#[derive(Resource, Clone, Copy, Default)]
pub struct AoBootPolicy(bool);

impl AoBootPolicy {
    pub fn from_experience(experience: &Experience) -> Self {
        Self(experience.enabled(Feature::AmbientOcclusion))
    }

    pub(crate) fn camera_msaa(self, configured: Msaa) -> Msaa {
        if self.0 {
            Msaa::Off
        } else {
            configured
        }
    }

    pub(crate) fn insert_aa(self, camera: &mut EntityCommands) {
        if self.0 {
            camera.insert(Smaa {
                preset: SmaaPreset::High,
            });
        }
    }
}

pub fn sync_camera(
    mut commands: Commands,
    experience: Res<Experience>,
    device: Option<Res<RenderDevice>>,
    cameras: Query<(Entity, &Msaa, Has<ScreenSpaceAmbientOcclusion>), With<WorldCamera>>,
    mut warned: Local<bool>,
) {
    let requested = experience.enabled(Feature::AmbientOcclusion);
    let supported = device
        .as_ref()
        .is_some_and(|device| device.limits().max_storage_textures_per_shader_stage >= 5);
    for (entity, msaa, active) in &cameras {
        let enable = requested && *msaa == Msaa::Off && supported;
        if requested && !enable && !*warned {
            warn!("Plus AO requires an MSAA-Off world camera and five storage textures; restart with AO configured. AO is inactive.");
            *warned = true;
        }
        if enable && !active {
            commands
                .entity(entity)
                .insert(ScreenSpaceAmbientOcclusion::default());
        } else if !enable && active {
            commands
                .entity(entity)
                .remove::<(ScreenSpaceAmbientOcclusion, DepthPrepass, NormalPrepass)>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::Profile;

    #[test]
    fn runtime_request_under_msaa_does_not_install_ao_or_change_aa() {
        let mut app = App::new();
        let mut experience = Experience::default();
        experience.set_profile(Profile::Plus);
        experience.set_feature(Feature::AmbientOcclusion, true);
        app.insert_resource(experience)
            .add_systems(Update, sync_camera);
        let camera = app.world_mut().spawn((WorldCamera, Msaa::Sample4)).id();
        app.update();
        assert_eq!(app.world().get::<Msaa>(camera), Some(&Msaa::Sample4));
        assert!(app
            .world()
            .get::<ScreenSpaceAmbientOcclusion>(camera)
            .is_none());
        assert!(app.world().get::<NormalPrepass>(camera).is_none());
        assert!(app.world().get::<Smaa>(camera).is_none());
    }

    #[test]
    fn classic_removes_active_ao_without_switching_boot_aa() {
        let mut app = App::new();
        app.insert_resource(Experience::default())
            .add_systems(Update, sync_camera);
        let camera = app
            .world_mut()
            .spawn((
                WorldCamera,
                Msaa::Off,
                ScreenSpaceAmbientOcclusion::default(),
                Smaa {
                    preset: SmaaPreset::High,
                },
            ))
            .id();
        assert!(app.world().get::<DepthPrepass>(camera).is_some());
        assert!(app.world().get::<NormalPrepass>(camera).is_some());
        app.update();
        assert!(app
            .world()
            .get::<ScreenSpaceAmbientOcclusion>(camera)
            .is_none());
        assert!(app.world().get::<DepthPrepass>(camera).is_none());
        assert!(app.world().get::<NormalPrepass>(camera).is_none());
        assert_eq!(app.world().get::<Msaa>(camera), Some(&Msaa::Off));
        assert!(app.world().get::<Smaa>(camera).unwrap().preset == SmaaPreset::High);
    }

    #[test]
    fn headless_device_does_not_enable_ao_even_with_msaa_off() {
        let mut app = App::new();
        let mut experience = Experience::default();
        experience.set_profile(Profile::Plus);
        experience.set_feature(Feature::AmbientOcclusion, true);
        app.insert_resource(experience)
            .add_systems(Update, sync_camera);
        let camera = app.world_mut().spawn((WorldCamera, Msaa::Off)).id();
        app.update();
        assert!(app
            .world()
            .get::<ScreenSpaceAmbientOcclusion>(camera)
            .is_none());
    }

    #[test]
    fn only_explicit_plus_ao_changes_boot_aa() {
        let mut experience = Experience::default();
        experience.set_feature(Feature::AmbientOcclusion, true);
        assert_eq!(
            AoBootPolicy::from_experience(&experience).camera_msaa(Msaa::Sample4),
            Msaa::Sample4
        );
        experience.set_profile(Profile::Plus);
        let boot = AoBootPolicy::from_experience(&experience);
        assert_eq!(boot.camera_msaa(Msaa::Sample4), Msaa::Off);
        experience.set_profile(Profile::Classic);
        assert_eq!(
            boot.camera_msaa(Msaa::Sample4),
            Msaa::Off,
            "boot policy is latched"
        );
        experience.set_profile(Profile::Plus);
        experience.set_feature(Feature::AmbientOcclusion, false);
        assert_eq!(
            AoBootPolicy::from_experience(&experience).camera_msaa(Msaa::Sample2),
            Msaa::Sample2
        );
    }
}
