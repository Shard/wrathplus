//! Build 12340's arena opponents (`BattlefieldInfo.cpp`), which the stock `Blizzard_ArenaUI`
//! frames draw: the `arenaN` tokens, `GetNumArenaOpponents` and
//! `ARENA_OPPONENT_UPDATE(unit, kind)`. Two read-only Ghidra rounds on `Wow.exe`:
//! - A player created inside an arena (instance type 4, `0xbea570`) whose `PLAYER_BYTES_3` byte 3,
//!   AzerothCore's `BattlefieldArenaFaction`, is not ours (`0xbea54a`, the status-3 byte) takes an
//!   opponent slot (the player's post-create `0x6e8280` → `0x54d100`,
//!   [`ArenaOpponents::assign`]); a new guid in it resets the slot's info cache, and the slot
//!   fires `"seen"` (`0x54b840`).
//! - An opponent leaving visibility, the update's out-of-range block, fires `"unseen"` (`0x4d7230`
//!   → `0x54abf0`); `SMSG_ARENA_UNIT_DESTROYED` (0x4C7), which AzerothCore sends ahead of every
//!   arena destroy (`Object::DestroyForPlayer`), fires `"destroyed"` (`0x54b5e0` → `0x54ad10`).
//! - Leaving the battleground clears every slot and fires `"cleared"` for all ten tokens
//!   (`0x54ca50`).
//!
//! An unseen opponent keeps its slot and its last snapshot, which no longer exists: the reference
//! answers `UnitClass` from the slot's cache while `UnitExists` is false (`Blizzard_ArenaUI.lua:133`).
//!
//! Unresolved, labelled: the pet slots' filler (`0x54d1c0`, reached from `0x73fcc0`/`0x7402b0`) is
//! not traced, so `arenapetN` stays empty; the event's `"seen"` is the string at `0xa0c3d8`, read
//! as the one kind the stock handler takes besides the three named here; a faction change on a
//! player already created (`0x6e1d70`) does not assign.

use bevy::prelude::*;
use wrathplus_protocol::types::{Field, ObjectType};
use wrathplus_protocol::wrath_battleground::Packet;
use wrathplus_ui::script::{ScriptValue, UiScript, UnitState};

use crate::app_core::arena::ArenaOpponents;
use crate::app_core::session::{ObjectStore, SelfPlayer};
use crate::app_core::state::Factions;
use crate::app_core::vm_memo::VmMemo;
use crate::names::NameCache;
use crate::net::{Guid, GuidIndex, NetCommands, Reputations};
use crate::ui_battlefield::NativeBattlegroundRest;
use crate::ui_dialog_verbs::BattlefieldQueue;
use crate::ui_unit::held::HeldUnits;

/// `Map.dbc`'s arena instance type.
const ARENA: u32 = 4;

pub fn register(app: &mut App) {
    app.init_resource::<ArenaOpponents>()
        .init_resource::<ArenaFeed>()
        .add_systems(
            Update,
            feed_arena
                .in_set(crate::app_core::schedule::UiFeed)
                .run_if(crate::app_core::ui_state::ingame_ui_up),
        );
}

/// What the feed remembers between frames.
#[derive(Resource, Default)]
pub struct ArenaFeed {
    in_arena: bool,
    /// The opponent was streamed when last looked at, so its leaving reads as `"unseen"` once.
    streamed: [bool; 5],
    /// The slot's last snapshot, the reference's info cache (`0xbe8f90`).
    cache: [Option<UnitState>; 5],
}

/// The tables and caches a held unit's snapshot reads, as `ui_unit`'s token feed takes them.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SnapshotInputs<'w, 's> {
    tables: crate::ui_unit::SnapshotTables<'w>,
    names: Res<'w, NameCache>,
    commands: Res<'w, NetCommands>,
    factions: Option<Res<'w, Factions>>,
    reputations: Res<'w, Reputations>,
    group: Res<'w, crate::app_core::group::GroupState>,
    me: Query<'w, 's, &'static ObjectStore, With<SelfPlayer>>,
    index: Option<Res<'w, GuidIndex>>,
    stores: Query<'w, 's, &'static ObjectStore>,
}

/// The objects streamed in since the feed last ran, other than us.
type Created<'w, 's> =
    Query<'w, 's, (&'static ObjectStore, &'static Guid), (Added<ObjectStore>, Without<SelfPlayer>)>;

/// `kind` for one token, as the event carries it.
fn event(token: String, kind: &str) -> (String, String) {
    (token, kind.to_string())
}

#[allow(clippy::too_many_arguments)]
fn feed_arena(
    script: Option<NonSendMut<UiScript>>,
    queue: Option<Res<BattlefieldQueue>>,
    mut table: ResMut<ArenaOpponents>,
    mut feed: ResMut<ArenaFeed>,
    created: Created,
    rest: Option<ResMut<NativeBattlegroundRest>>,
    inputs: SnapshotInputs,
    mut memo: Local<VmMemo<[Option<UnitState>; 5]>>,
) {
    let Some(mut script) = script else {
        return;
    };
    let (in_arena, ours) = queue.as_deref().map_or((false, 0), |q| {
        (q.active_instance_type() == ARENA, q.arena_faction)
    });
    let mut events = Vec::new();

    // `0x54ca50`: the battleground's end clears every slot, and says so for each token.
    if feed.in_arena && !in_arena {
        *table = ArenaOpponents::default();
        *feed = ArenaFeed::default();
        for n in 1..=5 {
            events.push(event(format!("arena{n}"), "cleared"));
            events.push(event(format!("arenapet{n}"), "cleared"));
        }
    }
    feed.in_arena = in_arena;

    if in_arena {
        // `0x6e8280`: a created player of the other arena faction.
        for (store, guid) in &created {
            if store.0.object_type() != Some(ObjectType::Player) {
                continue;
            }
            let faction = store
                .0
                .get(Field::PlayerBytes3)
                .map_or(0, |b| (b >> 24) as u8);
            if faction == ours {
                continue;
            }
            if let Some((slot, fresh)) = table.assign(guid.0) {
                if fresh {
                    feed.cache[slot] = None;
                }
                feed.streamed[slot] = true;
                events.push(event(format!("arena{}", slot + 1), "seen"));
            }
        }
    }

    // `0x54b5e0`: consumed everywhere, acted on only in an arena.
    if let Some(mut rest) = rest {
        rest.0.retain(|packet| {
            let Packet::ArenaUnitDestroyed { guid } = *packet else {
                return true;
            };
            if let Some(slot) = table.slot_of(guid).filter(|_| in_arena) {
                feed.streamed[slot] = false;
                events.push(event(format!("arena{}", slot + 1), "destroyed"));
            }
            false
        });
    }

    let lookup = crate::target::GuidStores {
        index: inputs.index.as_deref(),
        stores: &inputs.stores,
    };
    let held = HeldUnits {
        names: &inputs.names,
        commands: &inputs.commands,
        factions: inputs.factions.as_deref(),
        reputations: &inputs.reputations,
        group: &inputs.group,
        self_store: inputs.me.iter().next(),
        classes: inputs.tables.classes(),
        types: inputs.tables.types(&inputs.names),
        relations: crate::target::RelationContext::new(Some(&inputs.names), &lookup),
    };
    let store_of = |guid: u64| {
        let entity = *inputs.index.as_deref()?.0.get(&guid)?;
        inputs.stores.get(entity).ok()
    };
    for slot in 0..5 {
        let guid = table.arena[slot];
        if guid == 0 {
            feed.cache[slot] = None;
            continue;
        }
        match store_of(guid) {
            Some(store) => {
                feed.streamed[slot] = true;
                feed.cache[slot] = Some(held.state(store, guid));
            }
            None => {
                // `0x4d7230`: gone without a destroy first, so out of range.
                if std::mem::take(&mut feed.streamed[slot]) {
                    events.push(event(format!("arena{}", slot + 1), "unseen"));
                }
                if let Some(cached) = feed.cache[slot].as_mut() {
                    cached.exists = false;
                    cached.has_object = false;
                }
            }
        }
    }

    script.set_arena_units(table.arena, table.pets);
    let pushed = memo.get(&script);
    for (slot, (pushed, cached)) in pushed.iter_mut().zip(&feed.cache).enumerate() {
        if pushed != cached {
            script.set_unit(&format!("arena{}", slot + 1), cached.clone());
            pushed.clone_from(cached);
        }
    }
    for (unit, kind) in events {
        script.fire_event(
            "ARENA_OPPONENT_UPDATE",
            vec![ScriptValue::Str(unit), ScriptValue::Str(kind)],
        );
    }
}

#[cfg(test)]
#[path = "wrath_arena_tests.rs"]
mod tests;
