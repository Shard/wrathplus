//! What world code asks of the UI's state without reaching into the UI layer: whether the in-game
//! UI is up (the run condition for in-world feeds) and the seam scale across the VM boundary.

use bevy::prelude::*;

use super::ui_flags::PendingEntryUiLoad;

/// Run condition for every in-world feed: the in-game UI is up on the VM in the world, since a
/// feed that fires or drains into a VM with no frames loses what it sends. The reference likewise
/// processes no world state before its UI is up: world entry (`0x420d63`) runs after the same
/// iteration's inbound drain (`0x420d55`, its container `0x420c00` entered once per process),
/// socket threads only enqueue (`0x537b7c`; `SMSG_PONG` at `0x537b56` the one bypass), and the
/// login request waits for the UI (`0x46c272`).
///
/// Two terms, because the latch trails the wire by a frame: `apply_net_updates` drains `Connected`
/// and the login burst behind it in one pass, and the `InWorld` transition that arms the latch and
/// parks the VM runs at the next frame's `StateTransition`; the `InWorld` term covers that frame.
pub fn ingame_ui_up(
    pending: Option<Res<PendingEntryUiLoad>>,
    state: Option<Res<State<crate::app_core::state::ClientState>>>,
) -> bool {
    pending.is_none() && state.is_some_and(|s| *s.get() == crate::app_core::state::ClientState::InWorld)
}

/// The seam scale `s`, window px per UI unit (`windowH/768 × uiScale`), used at every crossing of
/// the VM boundary; identity for a degenerate window (h ≤ 0, before winit).
pub fn seam_scale(window_h: f32, ui_scale: f32) -> f32 {
    if window_h > 0.0 {
        window_h / 768.0 * ui_scale
    } else {
        1.0
    }
}
