//! Narrow offline visual cache seam. No network entity, object store or material mutation.
use super::{Creatures, DisplayModel, ModelHandle};
use benilla_assets::materials::WowModelMaterial;
use bevy::asset::{LoadState, RecursiveDependencyLoadState, VisitAssetDependencies};
use bevy::prelude::*;

/// Installed CreatureDisplayInfo ids wanted by the recorder, not creature entries or GUIDs.
#[derive(Resource, Default)]
pub(crate) struct RequestedDisplays(pub(crate) Vec<u32>);

pub(crate) struct ReadyDisplay<'a> {
    pub(crate) model: &'a DisplayModel,
    pub(crate) catalog: benilla_formats::CreatureModel,
}

impl Creatures {
    /// Pending is `Ok(None)`; invalid/unsupported data fails rather than earning a debug cube.
    pub(crate) fn showcase_display(
        &self,
        id: u32,
        server: &AssetServer,
        materials: &Assets<WowModelMaterial>,
        images: &Assets<Image>,
    ) -> Result<Option<ReadyDisplay<'_>>, String> {
        let catalog = self
            .catalog
            .model(id)
            .ok_or_else(|| format!("display {id}: missing catalog row"))?;
        if !catalog.scale.is_finite()
            || catalog.scale <= 0.0
            || catalog.model_path.is_empty()
            || catalog.npc_appearance.is_some()
            || catalog
                .model_path
                .to_ascii_lowercase()
                .starts_with("character\\")
        {
            return Err(format!("display {id}: requires a valid plain creature"));
        }
        let Some(model) = self.models.get(&id) else {
            return Ok(None);
        };
        let ModelHandle::M2(handle) = &model.handle else {
            return Err(format!("display {id}: no M2 model"));
        };
        if !external_ready(server, handle.id().untyped())? {
            return Ok(None);
        }
        let Some(parts) = &model.parts else {
            return Ok(None);
        };
        if parts.is_empty() {
            return Err(format!("display {id}: empty geometry"));
        }
        // Version one refuses unsupported frozen batch coverage. Never silently drop cards,
        // animated material channels or authored effects from an apparently complete actor.
        if !model.emitters.is_empty()
            || !model.ribbons.is_empty()
            || !model.lights.is_empty()
            || parts.iter().any(|p| {
                p.billboard.is_some()
                    || p.welded_billboard
                    || p.alpha_anim.is_some()
                    || p.rgb_anim.is_some()
                    || p.rgb_seq.is_some()
                    || p.uv_anim.is_some()
                    || p.uv_seq.is_some()
                    || p.uv_rot_seq.is_some()
                    || p.uv_scale_seq.is_some()
                    || p.ground_quad.is_some()
            })
        {
            return Err(format!(
                "display {id}: unsupported frozen batch/effect coverage"
            ));
        }
        if model.skeleton.joints.is_empty()
            || model.inverse_bindposes.is_none()
            || model
                .animations
                .as_ref()
                .is_none_or(|a| a.find(0).is_none())
            || parts.iter().any(|p| p.skinned_mesh.is_none())
        {
            return Err(format!(
                "display {id}: deterministic skinned Stand unavailable"
            ));
        }
        for part in parts {
            let Some(material) = materials.get(&part.material) else {
                return Ok(None);
            };
            if !material_images_ready(material, server, images)? {
                return Ok(None);
            }
        }
        Ok(Some(ReadyDisplay { model, catalog }))
    }
}

fn external_ready(server: &AssetServer, id: bevy::asset::UntypedAssetId) -> Result<bool, String> {
    let root = server.get_load_state(id);
    let dependencies = server.get_recursive_dependency_load_state(id);
    if matches!(root, Some(LoadState::Failed(_)))
        || matches!(dependencies, Some(RecursiveDependencyLoadState::Failed(_)))
    {
        return Err(format!("showcase asset/dependency failed: {id:?}"));
    }
    Ok(matches!(root, Some(LoadState::Loaded))
        && matches!(dependencies, Some(RecursiveDependencyLoadState::Loaded)))
}

/// VisitAssetDependencies enumerates all current and future effective image slots, including
/// extension textures. Generated images have no server state but must be resident in Assets.
fn material_images_ready(
    material: &impl VisitAssetDependencies,
    server: &AssetServer,
    images: &Assets<Image>,
) -> Result<bool, String> {
    let mut result = Ok(true);
    material.visit_dependencies(&mut |id| {
        let Ok(image) = id.try_typed::<Image>() else {
            return;
        };
        let ready = if server.get_load_state(id).is_some() {
            external_ready(server, id).map(|loaded| loaded && images.contains(image))
        } else {
            Ok(images.contains(image))
        };
        match ready {
            Err(error) => result = Err(error),
            Ok(false) if result.is_ok() => result = Ok(false),
            _ => (),
        }
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_material_images_must_all_be_resident_without_server_load_state() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Image>();
        let mut images = Assets::<Image>::default();
        let base = images.add(Image::default());
        let normal = images.add(Image::default());
        let material = StandardMaterial {
            base_color_texture: Some(base.clone()),
            normal_map_texture: Some(normal.clone()),
            ..default()
        };
        let server = app.world().resource::<AssetServer>();
        assert!(material_images_ready(&material, server, &images).unwrap());
        images.remove(normal.id());
        assert!(!material_images_ready(&material, server, &images).unwrap());
        images.remove(base.id());
        assert!(!material_images_ready(&material, server, &images).unwrap());
    }
}

#[cfg(test)]
mod request_tests {
    use super::*;
    #[test]
    fn default_requests_are_empty_and_do_not_name_network_entities() {
        assert!(RequestedDisplays::default().0.is_empty());
        let requests = RequestedDisplays(vec![42, 42]);
        assert_eq!(requests.0, [42, 42]);
    }
}
