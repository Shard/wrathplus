//! Opt-in replay witnesses. These do not participate in movement resolution.
use super::WorldCollision;
use avian3d::character_controller::move_and_slide::MoveHitData;
use avian3d::parry::query::PointQuery;
use avian3d::prelude::*;
use bevy::prelude::*;

/// Asset provenance, independent of collision audience. Unmarked runtime colliders stay unknown;
/// in particular, a walk layer alone cannot distinguish a WMO from an ADT impassable fence.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColliderSource {
    Wmo,
    M2,
    Terrain,
    #[default]
    Runtime,
}

impl ColliderSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wmo => "WMO",
            Self::M2 => "M2",
            Self::Terrain => "terrain",
            Self::Runtime => "runtime/unknown",
        }
    }
}

/// Nearest triangle to the replay's collider-side point, NOT an exact sweep feature ID.
/// At an edge or a tie another triangle may have produced the cast contact normal.
#[derive(Debug)]
pub struct WitnessFace {
    pub index: u32,
    pub normal: Vec3,
    pub point_gap: f32,
}

pub struct HitWitness {
    pub hit: MoveHitData,
    pub source: ColliderSource,
    /// None for convex shapes or a mesh with no nondegenerate triangle.
    pub nearest_face: Option<WitnessFace>,
}

impl WorldCollision<'_, '_> {
    /// Replays the intended horizontal sweep with the mover's live exclusions and one-sided law.
    /// A replay hit is evidence of a blocker, not a claim that it caused the final resolved delta
    /// (step-up and slide may take other paths). No cast or triangle search with `hit` filtered out.
    pub fn trace_hit_witness(
        &self,
        shape: &Collider,
        from: Vec3,
        intended: Vec3,
        skin_width: f32,
    ) -> Option<HitWitness> {
        if !benilla_assets::trace::enabled_for("hit") {
            return None;
        }
        let hit = self.cast_mover(shape, from, intended, skin_width)?;
        let source = self.sources.get(hit.entity).copied().unwrap_or_default();
        let nearest_face =
            self.ms
                .colliders
                .get(hit.entity)
                .ok()
                .and_then(|(collider, position, rotation, _)| {
                    nearest_face(collider, position.0, rotation.0, hit.point1)
                });
        Some(HitWitness {
            hit,
            source,
            nearest_face,
        })
    }
}

fn nearest_face(
    collider: &Collider,
    position: Vec3,
    rotation: Quat,
    point: Vec3,
) -> Option<WitnessFace> {
    let mesh = collider.shape_scaled().as_trimesh()?;
    let local_point = rotation.inverse() * (point - position);
    // Scan this hit mesh only; this diagnostic is off the production movement path. Compare
    // point-to-triangle distance, not centroid distance, which picks the wrong face on long tris.
    (0..mesh.num_triangles() as u32)
        .filter_map(|index| {
            let tri = mesh.triangle(index);
            let normal = rotation * tri.normal()?;
            let gap = tri
                .project_local_point(local_point, false)
                .point
                .distance(local_point);
            Some(WitnessFace {
                index,
                normal,
                point_gap: gap,
            })
        })
        .min_by(|a, b| {
            a.point_gap
                .total_cmp(&b.point_gap)
                .then(a.index.cmp(&b.index))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_face_uses_surface_distance_and_world_pose() {
        // The first triangle has a distant centroid but contains the witness; the second has a
        // closer centroid yet lies a yard away. Rotation and translation must both be undone.
        let collider = Collider::trimesh(
            vec![
                Vec3::ZERO,
                Vec3::X * 100.0,
                Vec3::Z * 100.0,
                Vec3::Y,
                Vec3::Y + Vec3::Z,
                Vec3::Y + Vec3::X,
            ],
            vec![[0, 2, 1], [3, 4, 5]],
        );
        let position = Vec3::new(7.0, 3.0, -4.0);
        let rotation = Quat::from_rotation_z(0.7);
        let point = position + rotation * Vec3::new(0.1, 0.0, 0.1);
        let face = nearest_face(&collider, position, rotation, point).unwrap();
        assert_eq!(face.index, 0);
        assert!(face.point_gap < 1e-4);
        assert!(face.normal.distance(rotation * Vec3::Y) < 1e-4);
    }

    #[test]
    fn convex_has_no_triangle_and_unknown_is_not_inferred_from_layers() {
        assert!(
            nearest_face(&Collider::sphere(1.0), Vec3::ZERO, Quat::IDENTITY, Vec3::X).is_none()
        );
        assert_eq!(ColliderSource::default().label(), "runtime/unknown");
    }
}
