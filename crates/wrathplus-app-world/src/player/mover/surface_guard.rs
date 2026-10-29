//! Independent feet-segment evidence for a missed capsule contact.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct Recovery {
    pub feet: Vec3,
    pub entity: Entity,
}

/// Deviation: the reference uses its one-sided walking volume (`0x632700`) alone. An independent
/// point segment prevents our capsule's overlap rejection from crossing a walkable solid face.
/// No upward search or remembered pose: cliffs, holes and already-below starts have no evidence.
pub(super) fn crossing(
    world: &wrathplus_world::collision::WorldCollision<'_, '_>,
    before: Vec3,
    after: Vec3,
) -> Option<Recovery> {
    let delta = after - before;
    if !before.is_finite() || !after.is_finite() || delta.y >= 0.0 {
        return None;
    }
    let dir = Dir3::new(delta).ok()?;
    let length = delta.length();
    // ray_mover is ray_body's authored-frontface/body-layer query plus actor exclusions.
    let hit = world.ray_mover(before, dir, length)?;
    let closing = -dir.dot(hit.normal);
    if hit.normal.y < GROUND_COS || closing <= 0.0 {
        return None;
    }
    // A normal landing ending on the face is not a recovery. Only certified penetration counts.
    if (length - hit.distance) * closing <= 1.0e-4 {
        return None;
    }
    // Nearest safe point on this very segment, held one skin short along the face normal.
    let distance = (hit.distance - SKIN_WIDTH / closing).max(0.0);
    let feet = before + *dir * distance;
    warn!(
        source = world.collider_source(hit.entity).label(),
        entity = ?hit.entity,
        ?before, ?after, ?feet,
        hit_distance = hit.distance,
        normal = ?hit.normal,
        "mover solid-surface crossing recovered"
    );
    Some(Recovery {
        feet,
        entity: hit.entity,
    })
}

#[cfg(test)]
mod tests;

/// Reconcile a current walkable capsule contact with the authored surface at the feet.
/// Deviation: parry can report a zero-distance landing with feet inside the face. Only the
/// skin band is reconciled, using a fresh mover-filtered frontface ray, never a previous pose.
pub(super) fn support(
    world: &wrathplus_world::collision::WorldCollision<'_, '_>,
    feet: Vec3,
) -> Option<Recovery> {
    if !feet.is_finite() {
        return None;
    }
    let hit = world.ray_mover(feet + Vec3::Y * SKIN_WIDTH, Dir3::NEG_Y, SKIN_WIDTH * 2.0)?;
    if hit.normal.y < GROUND_COS || hit.distance >= SKIN_WIDTH {
        return None;
    }
    Some(Recovery {
        feet: feet + Vec3::Y * (SKIN_WIDTH - hit.distance),
        entity: hit.entity,
    })
}
