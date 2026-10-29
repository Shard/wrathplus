//! Final main-world specialization collection, after streaming and late material writers.
//!
//! Bevy's PostUpdate collector clears its list on each run. A mesh/material published after
//! that run can already reach shadow extraction, whose specialization requires its tick.
//! Append here instead of replacing Bevy's list (which also includes asset-change events).

use bevy::pbr::{EntitiesNeedingSpecialization, Material};
use bevy::prelude::*;

/// Last-schedule mesh/material producers must run before this extraction boundary.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FinalMaterialSpecialization;

#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
fn append_late<M: Material>(
    changed: Query<
        Entity,
        (
            With<MeshMaterial3d<M>>,
            Or<(Changed<MeshMaterial3d<M>>, Changed<Mesh3d>)>,
        ),
    >,
    pending: Option<ResMut<EntitiesNeedingSpecialization<M>>>,
) {
    let Some(mut pending) = pending else { return };
    // Independent change detection from Bevy's earlier collector intentionally allows
    // duplicates: extraction overwrites the same entity tick and does not queue extra draws.
    pending.extend(changed.iter());
}

pub(crate) fn plugin(app: &mut App) {
    use wrathplus_assets::materials::{
        LiquidMaterial, TerrainMaterial, WdlMaterial, WowModelMaterial,
    };
    app.add_systems(
        Last,
        (
            append_late::<WowModelMaterial>,
            append_late::<TerrainMaterial>,
            append_late::<LiquidMaterial>,
            append_late::<WdlMaterial>,
            append_late::<crate::sky::SkyMaterial>,
            append_late::<crate::clouds::CloudMaterial>,
            append_late::<crate::sun::CelestialMaterial>,
            append_late::<crate::sun::StarMaterial>,
            append_late::<StandardMaterial>,
        )
            .in_set(FinalMaterialSpecialization),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::pbr::check_entities_needing_specialization;
    use wrathplus_assets::materials::WowModelMaterial;

    #[derive(Resource, Default)]
    struct Published(Option<Entity>);

    fn spawn_late(mut commands: Commands, mut published: ResMut<Published>) {
        if published.0.is_none() {
            published.0 = Some(
                commands
                    .spawn((
                        Mesh3d(Handle::default()),
                        MeshMaterial3d::<WowModelMaterial>(Handle::default()),
                    ))
                    .id(),
            );
        }
    }

    #[test]
    fn last_spawn_is_collected_before_extraction_without_erasing_earlier_entries() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), plugin))
            .init_asset::<Mesh>()
            .init_asset::<WowModelMaterial>()
            .init_resource::<EntitiesNeedingSpecialization<WowModelMaterial>>()
            .init_resource::<Published>()
            .add_systems(
                PostUpdate,
                check_entities_needing_specialization::<WowModelMaterial>,
            )
            .add_systems(Last, spawn_late.before(FinalMaterialSpecialization));
        let early = app
            .world_mut()
            .spawn((
                Mesh3d(Handle::default()),
                MeshMaterial3d::<WowModelMaterial>(Handle::default()),
            ))
            .id();
        app.update();
        let late = app.world().resource::<Published>().0.unwrap();
        let pending = app
            .world()
            .resource::<EntitiesNeedingSpecialization<WowModelMaterial>>();
        assert!(pending.contains(&early));
        assert!(
            pending.contains(&late),
            "Last producer needs a tick in this extraction"
        );
        app.update();
        // Bevy's earlier collector has not seen the Last spawn yet: it legitimately
        // collects that addition on the following frame. Our append must not erase it.
        assert!(app
            .world()
            .resource::<EntitiesNeedingSpecialization<WowModelMaterial>>()
            .contains(&late));
        app.update();
        // Once both collectors have observed the addition, a quiet frame clears the list.
        assert!(app
            .world()
            .resource::<EntitiesNeedingSpecialization<WowModelMaterial>>()
            .is_empty());
    }
}
