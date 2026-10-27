//! The material cache against its images' lifecycle ([`super::BatchPools::materials`]).

use bevy::prelude::*;

use super::{MatKey, UiQuadMaterial};

/// Forget the materials of removed images, and re-prepare those of modified ones. Read before any
/// early return, since a missed read leaks: a cached material holds a strong image handle and,
/// prepared, its GPU texture. `Removed`, not `Unused`: the cache is itself a strong handle, so
/// `Unused` never fires for a cached image, and a producer retiring an image removes the asset
/// explicitly. A `Modified` render target (the glue scene's, resized to the window) is a new GPU
/// texture, and a material re-prepares only when it is itself modified: untouched, it keeps
/// sampling the old texture, the stale scene behind character select (10-27).
pub(super) fn sync_cached_images(
    cache: &mut std::collections::HashMap<MatKey, Handle<UiQuadMaterial>>,
    events: &mut MessageReader<AssetEvent<Image>>,
    materials: &mut Assets<UiQuadMaterial>,
) {
    let (mut retired, mut changed) = (Vec::new(), Vec::new());
    for event in events.read() {
        match event {
            AssetEvent::Removed { id } => retired.push(*id),
            AssetEvent::Modified { id } => changed.push(*id),
            _ => {}
        }
    }
    if !retired.is_empty() {
        cache.retain(|key, _| !retired.contains(&key.0));
    }
    if changed.is_empty() {
        return;
    }
    for (key, material) in cache.iter() {
        if changed.contains(&key.0) || key.7.is_some_and(|mask| changed.contains(&mask)) {
            // `get_mut` marks the material modified: bevy re-prepares its bind group.
            let _ = materials.get_mut(material);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    #[derive(Resource, Default)]
    struct Cache(std::collections::HashMap<MatKey, Handle<UiQuadMaterial>>);

    fn sync(
        mut cache: ResMut<Cache>,
        mut events: MessageReader<AssetEvent<Image>>,
        mut materials: ResMut<Assets<UiQuadMaterial>>,
    ) {
        sync_cached_images(&mut cache.0, &mut events, &mut materials);
    }

    fn key(image: AssetId<Image>) -> MatKey {
        (
            image, false, false, false, false, false, 0, None, [0; 4], [0; 4],
        )
    }

    fn modified(
        app: &mut App,
        cursor: &mut bevy::ecs::message::MessageCursor<AssetEvent<UiQuadMaterial>>,
    ) -> usize {
        let events = app
            .world()
            .resource::<Messages<AssetEvent<UiQuadMaterial>>>();
        cursor
            .read(events)
            .filter(|e| matches!(e, AssetEvent::Modified { .. }))
            .count()
    }

    /// The glue scene's target resized under a cached pane material: the material re-prepares,
    /// so the pane samples the new texture; an untouched image leaves it alone, and a removed one
    /// drops it from the cache.
    #[test]
    fn a_resized_target_re_prepares_the_materials_that_sample_it() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<UiQuadMaterial>()
            .init_resource::<Cache>()
            .add_systems(Update, sync);
        let size = Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        };
        let image = Image::new_uninit(
            size,
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        let material = app
            .world_mut()
            .resource_mut::<Assets<UiQuadMaterial>>()
            .add(UiQuadMaterial {
                additive: 0,
                texture: Some(target.clone()),
                circular: 0,
                desaturate: 0,
                premultiplied: 1,
                alpha_ref: 0.0,
                gamma_texel: 0,
                mask_rect: Vec4::ZERO,
                mask: None,
                uv_clamp: Vec4::ZERO,
            });
        let mut cache = app.world_mut().resource_mut::<Cache>();
        cache.0.insert(key(target.id()), material.clone());
        let mut cursor = bevy::ecs::message::MessageCursor::default();
        app.update();
        app.update();
        modified(&mut app, &mut cursor);

        app.update();
        assert_eq!(
            modified(&mut app, &mut cursor),
            0,
            "a settled frame re-prepares nothing"
        );

        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .get_mut(&target)
            .unwrap()
            .resize(Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            });
        app.update();
        app.update();
        assert_eq!(
            modified(&mut app, &mut cursor),
            1,
            "the resize re-prepares the material sampling the target"
        );

        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .remove(&target);
        app.update();
        app.update();
        assert!(
            app.world().resource::<Cache>().0.is_empty(),
            "a removed image drops out"
        );
    }
}
