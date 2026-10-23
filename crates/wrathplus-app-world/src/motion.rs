//! Per-frame motion for streamed units between server packets. A [`Spline`] is a server path
//! sampled at constant speed, from `SMSG_MONSTER_MOVE` or joined mid-path from a create block;
//! [`RemoteMotion`] is another player's flag-driven dead-reckoning, snapped to each new packet,
//! with a jump played out locally under gravity. Poses live in raw WoW space on the components;
//! the `Transform` is derived from them and keeps the renderer's baked scale.

use wrathplus_world::schedule::WorldStage;
use bevy::prelude::*;

mod facing;
mod handlers;
mod modes;
mod relay;
mod remote;
mod spline;
#[cfg(test)]
mod tests;

pub use facing::drive_display_facing;
use facing::resolve_facing;
pub(crate) use facing::DisplayFacing;
pub(crate) use facing::FacingStep;
pub use modes::UnitMoveModes;
pub(crate) use modes::ROOT_APPLY_WIPE;
pub(crate) use relay::{PendingMove, RelayMove};
pub(crate) use remote::jump_seed;
pub use remote::RemoteMotion;
use remote::{apply_move, arrival_snap, trace_relay, RelayOutcome};
use remote::{drain_pending_moves, extrapolate_remote_units};
use spline::{
    create_spline, ground_clamp_creatures, mark_swimming_creatures, monster_move_spline,
    sample_splines, trace_create_spline, trace_move_snap,
};
pub use spline::{ground_derived, grounded_y};
pub use spline::{CreatureSwimming, GroundClamped, Spline, SplineStopped};

/// The movement model: the movement packets' handlers, each after the object layer's on the same
/// event, and the per-frame movers, chained after the drain ([`crate::net::NetApply`]).
pub struct MotionPlugin;

impl Plugin for MotionPlugin {
    fn build(&self, app: &mut App) {
        handlers::register(app);
        app.add_systems(
            Update,
            (
                sample_splines,
                // Swim state from the water at the feet (the wire never carries it for
                // creatures), before the clamp so a swimmer is exempt the same frame.
                mark_swimming_creatures,
                // After the raw spline Z: re-ground walkers (the client discards a ground
                // spline's Z); a swimmer keeps its wire Z.
                ground_clamp_creatures,
                // Due relays fire before the extrapolator advances the fresh state.
                drain_pending_moves,
                extrapolate_remote_units,
                // After the movers, so a turn reads the goal's position this frame.
                drive_display_facing,
            )
                .chain()
                .after(crate::net::NetApply)
                .in_set(WorldStage::Net),
        );
    }
}

// The pose helpers the net spawn shares, moved to `app_core` for the crate split.
use crate::app_core::movement::wire_yaw;
#[cfg(test)]
use crate::app_core::movement::{gameobject_rotation, pose_transform};

/// Writes a raw-WoW pose onto an existing entity, keeping the renderer's baked scale; an entity
/// not yet query-visible on its spawn frame is skipped, as its spawn pose already placed it.
///
/// A wire pose overrides the local facing turn, so it drops any [`DisplayFacing`] and the smoother
/// re-seeds from this pose, as the reference's resync sets the goal and wipes the ring
/// (`0x601020`).
pub(crate) fn write_pose(
    commands: &mut Commands,
    transforms: &mut Query<&mut Transform>,
    e: Entity,
    position: [f32; 3],
    rotation: Quat,
) {
    if crate::app_core::movement::set_pose(transforms, e, position, rotation) {
        commands.entity(e).remove::<DisplayFacing>();
    }
}

/// The WoW yaw of a unit rotation, which is always built by [`Quat::from_rotation_y`].
fn yaw_of(rotation: Quat) -> f32 {
    rotation.to_euler(EulerRot::YXZ).0
}
