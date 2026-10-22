//! Narrow offline visual cache seam. No network entity, object store or material mutation.
use super::{Creatures, DisplayModel, ModelHandle};
use wrathplus_assets::materials::WowModelMaterial;
use bevy::asset::{LoadState, RecursiveDependencyLoadState, VisitAssetDependencies};
use bevy::prelude::*;

/// Visual-only installed CreatureDisplayInfo id, never a server creature entry or GUID.
#[derive(Clone, Debug, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShowcaseActor {
    pub(crate) display_id: u32,
    pub(crate) position: [f64; 3],
    /// WoW heading radians, bounded to one turn; the visual spawner owns coordinate conversion.
    #[serde(default)]
    pub(crate) yaw: f64,
    /// Multiplier on the display's installed model scale, not an absolute game unit size.
    #[serde(default = "default_actor_scale")]
    pub(crate) scale: f64,
}

fn default_actor_scale() -> f64 {
    1.0
}

/// Installed CreatureDisplayInfo ids wanted by the recorder, not creature entries or GUIDs.
#[derive(Resource, Default)]
pub(crate) struct RequestedDisplays(pub(crate) Vec<u32>);

pub(crate) struct ReadyDisplay<'a> {
    pub(crate) model: &'a DisplayModel,
    pub(crate) catalog: wrathplus_formats::CreatureModel,
}

impl ReadyDisplay<'_> {
    pub(crate) fn bindposes_resident(
        &self,
        assets: &Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>,
    ) -> bool {
        self.model
            .inverse_bindposes
            .as_ref()
            .is_some_and(|h| assets.contains(h))
    }
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
        if model.skeleton.joints.iter().any(|j| j.billboard.is_some())
            || !model.emitters.is_empty()
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

/// One recorder-owned world visual. No WorldUnit marker means no interaction/nameplate lane.
// Reached only from the dev-build recorder (`showcase_capture::driver`).
#[allow(dead_code)]
pub(crate) struct SpawnedActor {
    pub(crate) root: Entity,
    pub(crate) slot: u16,
    pub(crate) bones: usize,
    pub(crate) transform: Transform,
    pub(crate) manifest: String,
}

// Reached only from the dev-build recorder (`showcase_capture::driver`).
#[allow(dead_code)]
pub(crate) fn actor_transform(
    actor: &ShowcaseActor,
    installed_scale: f32,
) -> Result<Transform, String> {
    let scale = installed_scale * actor.scale as f32;
    if !scale.is_finite() || scale <= 0.0 {
        return Err("showcase effective actor scale is invalid".into());
    }
    Ok(Transform {
        translation: wrathplus_assets::coords::wow_to_bevy(actor.position.map(|v| v as f32)),
        // Same as wire_yaw: a WoW +Z heading becomes Bevy +Y, no doodad 180-degree offset.
        rotation: Quat::from_rotation_y(actor.yaw as f32),
        scale: Vec3::splat(scale),
    })
}

/// World rig/material lane, not the booth's unfogged studio variants or mirrored palettes.
// Reached only from the dev-build recorder (`showcase_capture::driver`).
#[allow(dead_code)]
pub(crate) fn spawn_actor(
    commands: &mut Commands,
    palettes: &mut wrathplus_world::rig_palette::RigPalettes,
    actor: &ShowcaseActor,
    ready: ReadyDisplay<'_>,
) -> Result<SpawnedActor, String> {
    use wrathplus_world::rig_anim::RigPose;
    use wrathplus_world::rig_palette::{RigPart, RigSkin};
    let model = ready.model;
    let transform = actor_transform(actor, ready.catalog.scale)?;
    let ibp = model
        .inverse_bindposes
        .clone()
        .ok_or("showcase missing inverse bindposes")?;
    let anims = model
        .animations
        .as_ref()
        .ok_or("showcase missing animations")?;
    let clip = anims.find(0).ok_or("showcase missing Stand")?;
    let parts = model.parts.as_ref().ok_or("showcase missing parts")?;
    let rig = RigSkin::allocate_bones(palettes, model.skeleton.joints.len() as u32, ibp)
        .ok_or("showcase rig palette exhausted (no bind-pose fallback)")?;
    let slot = rig.slot;
    let root = commands.spawn((transform, Visibility::default())).id();
    let mut pose = RigPose::new(root, &model.skeleton);
    // Global bone tracks have an independent wall clock in normal play. Sample their zero
    // pose once; never attach GlobalSeqDrive to an offline frozen actor.
    for gb in &anims.global_bones {
        let Some(tf) = pose.locals.get_mut(gb.bone as usize) else {
            continue;
        };
        if let Some(c) = &gb.translation {
            tf.translation = c.sample(0.0);
        }
        if let Some(c) = &gb.rotation {
            tf.rotation = c.sample(0.0);
        }
        if let Some(c) = &gb.scale {
            tf.scale = c.sample(0.0);
        }
    }
    let mut player = AnimationPlayer::default();
    player.play(clip.node).pause();
    commands.entity(root).insert((
        pose,
        rig,
        player,
        AnimationGraphHandle(anims.graph.clone()),
        anims.clone(),
    ));
    for part in parts {
        // showcase_display has already proved skinned coverage for every part.
        let mesh = part
            .skinned_mesh
            .clone()
            .ok_or("showcase missing skinned mesh")?;
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(part.material.clone()),
            Transform::IDENTITY,
            ChildOf(root),
            RigPart(root),
            bevy::mesh::MeshTag(wrathplus_world::mesh_tag::spawn_tag(slot, 1.0)),
            // Bind-pose bounds cannot safely cull an animated Stand (same rule as the booth).
            bevy::camera::visibility::NoFrustumCulling,
        ));
    }
    let model_url = wrathplus_assets::m2_url(&ready.catalog.model_path);
    let normalized = model_url.trim_start_matches("mpq://");
    let dir = normalized.rsplit_once('/').map_or("", |(d, _)| d);
    let skins: Vec<String> = ready
        .catalog
        .textures
        .iter()
        .flatten()
        .map(|s| {
            wrathplus_assets::skin_url(dir, s)
                .trim_start_matches("mpq://")
                .to_owned()
        })
        .collect();
    let manifest = format!(
        "\n[[actors]]\ndisplay_id = {}\nrequested_model = {normalized:?}\nrequested_skins = {skins:?}\nposition = {:?}\nyaw = {}\nscale_multiplier = {}\ninstalled_scale = {}\neffective_scale = {}\npose = \"Stand\"\npose_seconds = 0.0\nprovenance = \"requested paths only; actual override consumption is in reader tracing\"\n",
        actor.display_id, actor.position, actor.yaw, actor.scale, ready.catalog.scale, transform.scale.x,
    );
    Ok(SpawnedActor {
        root,
        slot,
        bones: model.skeleton.joints.len(),
        transform,
        manifest,
    })
}

#[cfg(test)]
mod transform_tests {
    use super::*;
    #[test]
    fn actor_coordinates_yaw_and_installed_scale_match_world_units() {
        let actor = ShowcaseActor {
            display_id: 42,
            position: [10.0, 20.0, 30.0],
            yaw: std::f64::consts::FRAC_PI_2,
            scale: 0.5,
        };
        let transform = actor_transform(&actor, 2.0).unwrap();
        assert_eq!(transform.translation, Vec3::new(-20.0, 30.0, -10.0));
        assert_eq!(transform.scale, Vec3::ONE);
        assert!((transform.rotation * Vec3::NEG_Z).abs_diff_eq(Vec3::NEG_X, 1e-6));
        for invalid in [0.0, -1.0, f32::INFINITY, f32::NAN] {
            assert!(actor_transform(&actor, invalid).is_err());
        }
    }
}
