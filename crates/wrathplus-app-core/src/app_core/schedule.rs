//! System sets that systems outside their owning feature order on. Defined here so a lower layer
//! can name them without depending on the feature; each owner re-exports its set.

use bevy::prelude::*;

/// `ui_aura::feed_auras` fires `UNIT_AURA` (and the addon-only `PLAYER_AURAS_CHANGED`) inline,
/// so a feed whose state their handlers re-read (the stance feed) runs `.before(AuraEvents)` or
/// they read last frame's.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuraEvents;

/// The bindings module's systems inside [`UiInput`]. The UI key feed runs before it, so a key a
/// focused box consumed is already in the capture gate when dispatch runs.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindingSet;

/// The chat line drain, whose `DoEmote` arm writes the local [`crate::creature_anim::EmoteAnim`]:
/// the animation chain runs after it, so the same frame's driver sees the play.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChatDrain;

/// The one "only while in the world" gate; a member's own `run_if` ANDs with it. Configured for
/// `Update` only: a set's conditions are per schedule, so a member elsewhere needs the set
/// configured there too.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InWorldGated;

/// The wire drain, `net::apply_net_updates` alone: what a core system that reads this frame's
/// packets orders after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetDrain;

/// The player-UI feed phase, after [`wrathplus_world::schedule::WorldStage::Net`] and ungated
/// ([`UnitFeed`] is the gated sub-phase): every push this frame's tick must see, as in the
/// reference's frame, where packet dispatch precedes every `FrameScript_SignalEvent` and both
/// precede `OnUpdate`. Every VM holder in `Update` is in this set or `.after(UiInput)`, as
/// `game_plugins::schedule_tests` checks.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiFeed;

/// The player-UI input pass (tick, hit-test, handlers), before
/// [`wrathplus_world::schedule::WorldStage::Input`] so its [`crate::ui_script::PlayerUiHover`]
/// reaches `PointerOverUi` before `player::control` reads it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiInput;

/// Producers that append to [`crate::ui_pass::UiQuads`] after the script extract. In Update after
/// [`wrathplus_world::schedule::WorldStage::Input`], the world-anchored ones (combat numbers,
/// nameplates) project through this frame's camera `Transform` (`GlobalTransform::from`; the
/// camera is a root entity), not the propagated one a frame behind. Never PostUpdate:
/// `ui_pass::rebuild_ui_mesh` spawns `Mesh2d` batches, and bevy_sprite_render cannot specialize
/// one spawned there (the whole UI vanishes).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiQuadAppend;

/// The unit-feed pass, gated so none of its login one-shots or per-VM memos runs before the in-game
/// interface exists; the demo override orders itself after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitFeed;
