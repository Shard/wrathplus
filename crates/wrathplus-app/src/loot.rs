//! The loot state machine's world half: the transitions the selection, the click and the anim
//! driver share ([`begin_wrath_corpse_loot`], [`close_interaction`]) and the kneel predicate
//! ([`resolve_loot_kneel`]). `ui_loot` keeps the Lua feed, the drain and the net handlers.

use bevy::prelude::*;

use crate::net::{ClientCommand, NetCommands};

use crate::app_core::loot::LootState;
use crate::app_core::state::LootLatch;

/// Whether the character kneels at the latched object, the reference's predicate B `0x612710`:
/// yes for a corpse or any GameObject but a fishing bobber, no for a live unit, an item or an
/// unresolved guid. Recomputed each frame between the net drain and the anim driver, as the
/// reference force-plays the pose at the arm itself (`0x5ed619`).
#[derive(Resource, Default)]
pub(crate) struct LootKneel(pub(crate) bool);

/// `GAMEOBJECT_TYPE_ID` 17, `FISHINGNODE`: the one GameObject type that does not kneel
/// (`0x612772`).
const GO_TYPE_FISHINGNODE: i32 = 17;

/// Recomputes [`LootKneel`]; `pub(crate)` so [`crate::creature_anim`]'s driver can order after it.
pub(crate) fn resolve_loot_kneel(
    latch: Res<LootLatch>,
    index: Res<crate::net::GuidIndex>,
    objects: Query<(&crate::net::NetEntity, &crate::net::ObjectStore)>,
    mut kneel: ResMut<LootKneel>,
) {
    let allowed = latch
        .0
        .and_then(|guid| index.0.get(&guid).copied())
        .and_then(|e| objects.get(e).ok())
        .is_some_and(|(net_entity, store)| match net_entity.kind {
            // `0x612764`: a GameObject kneels unless it is the bobber.
            wrathplus_protocol::EntityKind::GameObject => {
                store.0.gameobject_type_id() != GO_TYPE_FISHINGNODE
            }
            // `0x61278c`: a unit kneels once its health reads 0 off the descriptor, where an
            // unsent field is 0 too; not `unit_is_dead`, whose `max_health` guard it lacks.
            wrathplus_protocol::EntityKind::Unit | wrathplus_protocol::EntityKind::Player => {
                store.0.unit_health().unwrap_or(0) == 0
            }
            // An item never kneels (`0x612797`); we stream none, so a lockbox latch never resolves.
            _ => false,
        });
    if kneel.0 != allowed {
        kneel.0 = allowed;
    }
}

/// `CloseInteraction 0x48f200` past its early returns, the one close every loot close shares: the
/// latch drops (`0x48f2c9`), the release goes out when `release` is given (the `cl` argument,
/// `0x48f2da`) and the frame closes (`0x48f33d`). Returns the closed source for
/// `ui_loot`'s dead-unit deselect, the close's last step.
pub(crate) fn close_interaction(
    loot: &mut LootState,
    latch: &mut LootLatch,
    release: Option<&NetCommands>,
) -> Option<u64> {
    let source = loot.source?;
    latch.clear_for(source);
    if let Some(commands) = release {
        if commands
            .0
            .send(ClientCommand::LootRelease { guid: source })
            .is_ok()
        {
            // No-op for Vanilla's idle correlation state. Wrath must retain its source fence
            // through target teardown/movement closes until the matching server release arrives.
            loot.wrath.begin_release(source);
        }
    }
    loot.clear();
    Some(source)
}

/// Starts one build-12340 corpse-loot request. The latch stays cold until the matching response,
/// so a failed channel send cannot predict either an open window or a kneel.
pub(crate) fn begin_wrath_corpse_loot(
    loot: &mut LootState,
    latch: &mut LootLatch,
    net: &NetCommands,
    guid: u64,
) -> bool {
    if loot.source().is_some() || latch.0.is_some() || !loot.wrath.begin(guid) {
        return false;
    }
    if net.0.send(ClientCommand::Loot { guid }).is_err() {
        loot.wrath.reset();
        return false;
    }
    true
}
