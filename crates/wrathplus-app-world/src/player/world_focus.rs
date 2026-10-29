//! What the game tells the world, and what it does with what the world publishes back:
//! [`publish_view_focus`] says where to stream from, ahead of the stream stage, and
//! [`release_post_snap_hold`] ends the mover's settle hold on the residency the world publishes.
//! The hold ends on residency, never on ground contact, the same way for every mover mode.

use crate::player::state::PlayerSettle;
use bevy::prelude::*;

use super::{Player, SETTLE_TIMEOUT};
use wrathplus_world::terrain_stream::{ViewFocus, WorldLoadProgress};
use wrathplus_world::view::Viewer;

/// Tells the world where the avatar's body is ([`Viewer`]), with the focus's position and gate.
pub(super) fn publish_viewer(
    mut viewer: ResMut<Viewer>,
    player: Option<Res<Player>>,
    rig: Option<Res<super::CameraControl>>,
    screen: Option<Res<crate::loading_screen::LoadingScreen>>,
    store: Query<
        &crate::app_core::session::ObjectStore,
        With<crate::app_core::session::SelfPlayer>,
    >,
) {
    // Drunk and ghost are whole-screen effects of our own character, so they read `SelfPlayer`,
    // not the body we drive: possessing a boar does not sober you up.
    let (drunk, ghost) = match store.single() {
        Ok(s) => (
            s.0.player_drunk_byte()
                .map_or(0.0, |b| f32::from(b.min(100)) / 100.0),
            s.0.player_is_ghost(),
        ),
        Err(_) => (0.0, false),
    };
    // `WOW_GHOST_PROBE` overrides the flag the death light and the DeathClouds sky both read.
    let ghost = crate::death::ghost_probe().unwrap_or(ghost);
    let body = match player.as_deref() {
        Some(p) if p.active && !p.detached => Viewer {
            at: Some(p.pos),
            move_flags: p.move_flags(),
            planar_speed: p.planar_speed(),
            height: p.collision_height.0,
            ..Viewer::default()
        },
        _ => Viewer::default(),
    };
    *viewer = Viewer {
        drunk,
        ghost,
        // The zoom feather alone: the mesh side folds in the aura factor, so it must not be here.
        self_fade: rig.as_deref().map_or(1.0, super::CameraControl::self_fade),
        world_covered: screen.as_deref().is_some_and(|s| s.covering()),
        ..body
    };
}

/// Tells the world where to stream from each frame, before the stream stage; a cinematic, which
/// streams from the camera, also puts the body on the settle hold.
pub(super) fn publish_view_focus(
    mut focus: ResMut<ViewFocus>,
    mut player: Option<ResMut<Player>>,
    roster: Option<Res<crate::app_core::state::Roster>>,
    cinematic: Option<Res<crate::cinematic::Cinematic>>,
    mut was_flying: Local<bool>,
) {
    let entry = roster
        .as_deref()
        .and_then(crate::app_core::state::Roster::pending_entry);
    // A cinematic flies the eye away from the body (a Tauren intro opens 1741 yd out), so the
    // stream follows the camera, as in free-fly.
    let flying = cinematic.as_deref().is_some_and(|c| c.is_playing());
    // Unlike free-fly, where `control` skips the body, a cinematic simulates it while its tiles
    // unload, so it takes the settle hold, which only residency of its own tile ends. `world_stale`
    // keeps the stall backstop from turning gravity back on mid-shot. Armed on the rising edge
    // only, so a shot that stays on the body's tile lets the release end the hold.
    if flying && !*was_flying {
        if let Some(p) = player.as_deref_mut().filter(|p| p.active) {
            p.settling = true;
            p.world_stale = true;
            info!("cinematic: body held — the stream follows the camera off its ground");
        }
    }
    *was_flying = flying;
    // Spawn caps pace only a live avatar in a settled world; under the loading cover (entry, a
    // teleport, a world swap) a cap would only lengthen the reveal.
    *focus = match player.as_deref() {
        Some(p) if p.active => {
            let wow = wrathplus_assets::coords::bevy_to_wow(p.pos);
            let paced = !p.settling && !p.world_stale;
            if p.detached || flying {
                ViewFocus::detached(wow, paced)
            } else {
                ViewFocus::body(wow, paced)
            }
        }
        // No avatar: the picked character's row for the entry window, else
        // whatever the camera can see.
        _ => match entry {
            Some((map, pos)) => ViewFocus::entry(map, pos),
            None => ViewFocus::camera(),
        },
    };
}

/// Seats shallow server/floor overlaps before the residency gate releases the load hold.
pub(super) fn seat_post_snap_floor(
    mut player: ResMut<Player>,
    world: wrathplus_world::collision::WorldCollision,
) {
    if player.active && player.settling {
        super::mover::seat_streamed_floor(&mut player, &world);
    }
}

/// Ends the post-snap hold when the destination's world has arrived. Runs after the stream stage,
/// so `colliders_pending` is this frame's count.
pub(super) fn release_post_snap_hold(
    mut player: ResMut<Player>,
    progress: Option<Res<WorldLoadProgress>>,
    time: Res<Time>,
    mut last_counters: Local<Option<[usize; 6]>>,
    net_cmds: Option<Res<crate::net::NetCommands>>,
) {
    let Some(p) = progress else { return };
    // Residency counts only for the tile under the body. On a mismatch (the teleport's snap frame,
    // a free-fly eye) the release and the stale-clear are refused and only the backstop runs.
    let focus_matches = p.focus_tile.is_some_and(|t| {
        let wow = wrathplus_assets::coords::bevy_to_wow(player.pos);
        let (tx, ty) = wrathplus_formats::world_to_tile(wow[0], wow[1]);
        t == (tx as i32, ty as i32)
    });
    // Residency means this map's own tile (a WMO-only map's one building) is spawned, which a
    // swap reaches only after draining every tile of the map we left.
    if p.focus_resident && p.total > 0 && focus_matches {
        player.world_stale = false;
    }
    // Any counter moving is the destination still arriving; tracked every frame, so the first
    // settling frame has a baseline.
    let counters = [
        p.ready,
        p.total,
        p.colliders_pending,
        p.merge_pending,
        p.placements_pending,
        p.gx_pending,
    ];
    let progressed = last_counters.replace(counters) != Some(counters);
    if !player.settling {
        return;
    }
    // The release needs scene and colliders, never ground contact. The timeout is a stall budget:
    // the deadline is pushed while the resident world is still the map we left and while the
    // stream advances, so only a stream with no progress for the whole budget times out.
    let now = time.elapsed_secs();
    if p.presentable() && !player.world_stale && focus_matches {
        player.end_settle(true, now);
    } else if player.world_stale || progressed {
        player.settle_deadline = now + SETTLE_TIMEOUT;
    } else if now >= player.settle_deadline {
        player.end_settle(false, now);
    }
    // The worldport ack, the reference's post-load `0xDC`, goes at either end of the hold: the
    // server keeps an unacked transfer out of world until logout.
    if !player.settling {
        if let (Some(token), Some(net)) = (player.native_worldport_ack, net_cmds.as_deref()) {
            player.native_worldport_ack = None;
            let _ = net
                .0
                .send(crate::net::ClientCommand::NativeWorldportAck(token));
        }
    }
    if !player.settling && player.owes_worldport_ack {
        if let Some(net) = net_cmds.as_deref() {
            player.owes_worldport_ack = false;
            let _ = net.0.send(crate::net::ClientCommand::WorldportAck);
            info!("worldport: ack sent at settle release");
        }
    }
}

#[cfg(test)]
#[path = "world_focus_tests.rs"]
mod tests;
