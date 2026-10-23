//! Build 12340's dungeon and raid difficulty, which `GetDungeonDifficulty`/`GetRaidDifficulty`
//! answer and the portrait menu checks (`UnitPopup.lua:309`, `:331`).
//!
//! AzerothCore sends `MSG_SET_DUNGEON_DIFFICULTY` (0x329) at login and both it and
//! `MSG_SET_RAID_DIFFICULTY` (0x4EB) on every change, solo or grouped, as `u32 difficulty,
//! u32 1, u32 isInGroup` (`Player::SendDungeonDifficulty`, `SendRaidDifficulty`); a group's change
//! sets every member's own difficulty too (`Group::SetDungeonDifficulty`). One read-only Ghidra
//! round on `Wow.exe` traced the handler `0x526530`:
//! - Each kind keeps two stores, the player's own (dungeon `0xbd0898`, raid `0xbd089c`) and the
//!   group's (`0xbd1978`, `0xbd197c`). The second word writes the own store, the third the group's
//!   (`0x525530`, `0x5255a0`). The group list's loot block writes the group's here as well.
//! - The answer is the group's store while grouped (group guid `0xbd1948`), else the own one.
//! - When a packet moves the answer, the chat line `ERR_DUNGEON_DIFFICULTY_CHANGED_S` (row
//!   `0x1f7`) or `ERR_RAID_DIFFICULTY_CHANGED_S` (`0x29e`) prints with the global string
//!   `DUNGEON_DIFFICULTY<n>` or `RAID_DIFFICULTY<n>`, `n` the new value plus one (`0x524720`,
//!   `0x524780`). No Lua event fires.
//!
//! Unresolved, labelled: the flag `0xbeb608` that makes a grouped player answer its own store is
//! read as clear; inside a dynamic-difficulty map (Map.dbc flag `0x100`) the raid packet's group
//! word goes to `0x52d8d0` instead, and inside a raid instance the raid line names the map's
//! difficulty row: neither is modelled, so the plain store and line apply there too.

use bevy::prelude::*;
use wrathplus_protocol::wrath_instance_control::Packet;
use wrathplus_protocol::SessionEvent;
use wrathplus_ui::script::UiScript;

use crate::app_core::errors::{UiError, UiErrorKeys};
use crate::app_core::group::GroupState;
use crate::net::wrath_instance_control_capture::NativeInstanceControlPackets;

pub fn register(app: &mut App) {
    use crate::net::NetHandlerApp;
    use wrathplus_protocol::SessionEventKind as K;
    app.init_resource::<NativeDifficulty>()
        .net_handler(K::Connected, clear)
        .net_handler(K::Disconnected, clear)
        .add_systems(
            Update,
            feed_difficulty
                .in_set(crate::app_core::schedule::UiFeed)
                .run_if(crate::app_core::ui_state::ingame_ui_up),
        );
}

/// One kind's two stores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stores {
    pub own: u32,
    pub group: u32,
}

impl Stores {
    pub fn answer(&self, grouped: bool) -> u32 {
        if grouped {
            self.group
        } else {
            self.own
        }
    }
}

/// The dungeon and raid stores, and the group list's loot pair last applied to the group's.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeDifficulty {
    pub dungeon: Stores,
    pub raid: Stores,
    seen_loot: Option<(u8, u8)>,
}

impl NativeDifficulty {
    /// One packet: its stores, and the line when it moves the answer.
    fn apply(&mut self, packet: &Packet, grouped: bool) -> Option<(&'static str, String)> {
        let (stores, key, global, difficulty, own, group) = match *packet {
            Packet::DungeonDifficulty {
                difficulty,
                unknown,
                in_group,
            } => (
                &mut self.dungeon,
                "ERR_DUNGEON_DIFFICULTY_CHANGED_S",
                "DUNGEON_DIFFICULTY",
                difficulty,
                unknown,
                in_group,
            ),
            Packet::RaidDifficulty {
                difficulty,
                unknown,
                in_group,
            } => (
                &mut self.raid,
                "ERR_RAID_DIFFICULTY_CHANGED_S",
                "RAID_DIFFICULTY",
                difficulty,
                unknown,
                in_group,
            ),
            _ => return None,
        };
        let before = stores.answer(grouped);
        if own != 0 {
            stores.own = difficulty;
        }
        if group != 0 {
            stores.group = difficulty;
        }
        let after = stores.answer(grouped);
        (after != before).then(|| (key, format!("{global}{}", after + 1)))
    }
}

/// A session's stores start over: the next login's packet sets them again.
fn clear(In(event): In<SessionEvent>, mut state: ResMut<NativeDifficulty>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        *state = NativeDifficulty::default();
    }
}

fn feed_difficulty(
    script: Option<NonSendMut<UiScript>>,
    packets: Option<ResMut<NativeInstanceControlPackets>>,
    mut state: ResMut<NativeDifficulty>,
    group: Res<GroupState>,
    errors: Option<ResMut<UiErrorKeys>>,
) {
    let Some(mut script) = script else {
        return;
    };
    let grouped = group.wrath_roster.is_some();
    let loot = group
        .wrath_roster
        .as_ref()
        .and_then(|r| r.loot.as_ref())
        .map(|l| (l.dungeon_difficulty, l.raid_difficulty));
    if loot.is_some() && loot != state.seen_loot {
        let (dungeon, raid) = loot.unwrap_or_default();
        state.dungeon.group = u32::from(dungeon);
        state.raid.group = u32::from(raid);
    }
    state.seen_loot = loot;

    let mut lines = Vec::new();
    if let Some(mut packets) = packets {
        packets.0.retain(|packet| {
            if !matches!(
                packet,
                Packet::DungeonDifficulty { .. } | Packet::RaidDifficulty { .. }
            ) {
                return true;
            }
            lines.extend(state.apply(packet, grouped));
            false
        });
    }
    if let Some(mut errors) = errors {
        for (key, global) in lines {
            // `0x819d40`: the line carries the global string's text, not its name.
            let text = script
                .eval::<Option<String>>(&format!("return {global}"))
                .ok()
                .flatten()
                .unwrap_or(global);
            errors.0.push(UiError::s(key, text));
        }
    }
    let as_index = |v: u32| u8::try_from(v + 1).unwrap_or(u8::MAX);
    script.set_difficulties(
        as_index(state.dungeon.answer(grouped)),
        as_index(state.raid.answer(grouped)),
    );
}

#[cfg(test)]
#[path = "wrath_difficulty_tests.rs"]
mod tests;
