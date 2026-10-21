//! The open loot window's state, its Wrath extension and the item pushes awaiting it, read by
//! world and net code; `ui_loot` keeps the systems and re-exports these from its old paths.

use std::collections::VecDeque;

use benilla_protocol::types::{slot_type, LootItem};

use super::group::{GroupState, GROUPTYPE_RAID, GROUP_MEMBER_SUBGROUP};
use bevy::prelude::*;

#[derive(Default)]
pub(crate) struct WrathLootState {
    pub(crate) source: Option<u64>,
    pub(crate) phase: Phase,
    pub(crate) takes: VecDeque<u8>,
    pub(crate) money: bool,
}

impl WrathLootState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn begin(&mut self, guid: u64) -> bool {
        if self.phase != Phase::Idle {
            return false;
        }
        self.source = Some(guid);
        self.phase = Phase::Requesting;
        true
    }

    pub(crate) fn admit_response(&mut self, guid: u64) -> bool {
        if self.phase != Phase::Requesting || self.source != Some(guid) {
            return false;
        }
        self.phase = Phase::Active;
        true
    }

    /// A response we did not request (a chest or lockbox our use opened, fishing, a pickpocket):
    /// adopted as the open window when nothing else is in flight. A repeat of the open window is
    /// admitted too, as the latch still names it (`0x6d53b0`).
    pub(crate) fn adopt_response(&mut self, guid: u64) -> bool {
        if self.phase == Phase::Active && self.source == Some(guid) {
            return true;
        }
        if self.phase != Phase::Idle {
            return false;
        }
        self.source = Some(guid);
        self.phase = Phase::Active;
        true
    }

    /// Whether our own `CMSG_LOOT` for `guid` is unanswered: the reference arms its latch at that
    /// send, where ours stays cold until the response, so this stands in for the latch match.
    pub(crate) fn is_requesting(&self, guid: u64) -> bool {
        self.phase == Phase::Requesting && self.source == Some(guid)
    }

    /// The response handler zeroes the latch on a refusal and on a releasing error (`0x6d53b0`),
    /// which drops an unanswered request with it; an open window's session is untouched.
    pub(crate) fn drop_request(&mut self) {
        if self.phase == Phase::Requesting {
            self.reset();
        }
    }

    pub(crate) fn begin_take(&mut self, slot: u8) -> bool {
        if self.phase != Phase::Active || self.takes.contains(&slot) {
            return false;
        }
        self.takes.push_back(slot);
        true
    }

    pub(crate) fn undo_take(&mut self, slot: u8) {
        if let Some(index) = self.takes.iter().position(|pending| *pending == slot) {
            self.takes.remove(index);
        }
    }

    pub(crate) fn acknowledge_take(&mut self, slot: u8) -> bool {
        let Some(index) = self.takes.iter().position(|pending| *pending == slot) else {
            return false;
        };
        self.takes.remove(index);
        true
    }

    pub(crate) fn refuse_take(&mut self) -> bool {
        self.phase == Phase::Active && self.takes.pop_front().is_some()
    }

    pub(crate) fn begin_money(&mut self) -> bool {
        if self.phase != Phase::Active || self.money {
            return false;
        }
        self.money = true;
        true
    }

    pub(crate) fn undo_money(&mut self) {
        self.money = false;
    }

    pub(crate) fn acknowledge_money_notify(&self) -> bool {
        self.phase == Phase::Active && self.money
    }

    pub(crate) fn acknowledge_money_clear(&mut self) -> bool {
        self.phase == Phase::Active && std::mem::take(&mut self.money)
    }

    pub(crate) fn begin_release(&mut self, guid: u64) {
        if self.source == Some(guid) && self.phase == Phase::Active {
            self.phase = Phase::Releasing;
            self.takes.clear();
            self.money = false;
        }
    }

    pub(crate) fn acknowledge_release(&mut self, guid: u64) -> bool {
        if self.source != Some(guid) || !matches!(self.phase, Phase::Active | Phase::Releasing) {
            return false;
        }
        self.reset();
        true
    }

    #[cfg(test)]
    pub(crate) fn pending_take_count(&self) -> usize {
        self.takes.len()
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    #[default]
    Idle,
    Requesting,
    Active,
    Releasing,
}

/// A push waiting on its item template. `OnItemPush 0x491a60` defers both of its outputs the same
/// way: on a cache miss (`0x491a93`) it re-enters from the cache callback `0x491ee0`.
pub(crate) struct PendingReceive {
    /// Crafted or conjured: "You create: …".
    pub(crate) created: bool,
    pub(crate) entry: u32,
    pub(crate) count: u32,
    pub(crate) from_npc: bool,
    pub(crate) random_property_id: u32,
    pub(crate) suffix_factor: u32,
    /// `showInChat` gates the line only: `ITEM_PUSH` fires (`0x491be8`) before the chat test
    /// (`0x491bf3`), so a silent push still animates.
    pub(crate) in_chat: bool,
    pub(crate) container: i64,
    pub(crate) tries: u16,
}

/// The open loot as the wire delivered it, plus the push queue, which outlives the window. The
/// layout is fixed at open: a looted row is a gap, as `LOOT_SLOT_CLEARED` hides its button in
/// place and `numLootItems` is read once at `OnShow` (`LootFrame.lua:22-37`, `:132`).
#[derive(Resource, Default)]
pub(crate) struct LootState {
    /// The looted object; `None` when no loot is open.
    pub(crate) source: Option<u64>,
    /// The coin pile in copper, 0 once taken.
    pub(crate) gold: u32,
    /// Whether layout position 1 is the coin pile: fixed at open, kept after the coin is taken.
    pub(crate) coin_slot: bool,
    /// The item rows in wire order; never shrinks while open, a taken row lands in `taken`.
    pub(crate) items: Vec<LootItem>,
    /// Wire slots taken by anyone (`SMSG_LOOT_REMOVED`), left as gaps.
    pub(crate) taken: Vec<u8>,
    pub(crate) receives: Vec<PendingReceive>,
    /// A removal just emptied the window, so the client releases it, as the reference does on the
    /// last slot (`0x4c2a70` → `0x48f200`); vmangos does not. Never set at open: an empty window
    /// stays up (`LOOTWINDOWOPENEMPTY`).
    pub(crate) auto_release: bool,
    /// Wire `loot_type` 3, for `IsFishingLoot()`: `LootFrame_OnShow`'s reel-in sound and icon.
    pub(crate) fishing: bool,
    /// The open window's master-loot candidates (`SMSG_LOOT_MASTER_LIST`), in wire order.
    pub(crate) master_candidates: Vec<u64>,
    /// The row a `LOOT_BIND_CONFIRM` is open for, the reference's `[0x847cec]`: set by a click
    /// (`0x4c2790`), cleared by the confirmed send (`0x4c281a`) and per window (`0x4c1df5`).
    /// Deviation: it is the display row, where the reference's is the item index plus one
    /// (`0x4c2885`), so an addon's `GetLootSlotInfo(arg1)` reads the row the dialog is for.
    pub(crate) pending_bind_confirm: Option<u32>,
    /// The list staged for the next open: vmangos sends it from inside `Player::SendLoot`
    /// (`Player.cpp:8080`), just ahead of its response. Staging keeps it out of later windows.
    pub(crate) pending_master_candidates: Vec<u64>,
    /// The wire `loot_type` (`0x4c2740`); a disenchant window with rows left survives movement.
    pub(crate) loot_type: u8,
    /// Build-12340's one-source request/ack correlation. It deliberately survives local close
    /// until the matching release response, so slot-less late packets cannot reach a newer source.
    pub(crate) wrath: WrathLootState,
}

impl LootState {
    /// Opens or replaces the window; quest items ride the same rows at `slot = items.len() + i`.
    pub(crate) fn open(&mut self, source: u64, loot_type: u8, gold: u32, items: Vec<LootItem>) {
        self.source = Some(source);
        // An open window is an active build-12340 session; a response admission already made it one.
        self.wrath.adopt_response(source);
        self.gold = gold;
        self.coin_slot = gold > 0; // fixed for the window's lifetime
        self.items = items;
        self.taken.clear();
        self.auto_release = false; // an empty window at open stays open
        self.pending_bind_confirm = None; // `0x4c1df5`: the copier resets the stash
        self.loot_type = loot_type;
        self.fishing = loot_type == benilla_protocol::types::loot_type::FISHING;
        self.master_candidates = std::mem::take(&mut self.pending_master_candidates);
    }

    /// Stages a list for the next open; an open window takes it at once, as a refresh.
    pub(crate) fn set_master_candidates(&mut self, candidates: Vec<u64>) {
        if self.source.is_some() {
            self.master_candidates.clone_from(&candidates);
        }
        self.pending_master_candidates = candidates;
    }

    /// `SMSG_LOOT_REMOVED`: the row becomes a gap; an emptied window arms the auto-close.
    pub(crate) fn remove_slot(&mut self, wire_slot: u8) {
        if self.items.iter().any(|it| it.slot == wire_slot) && !self.taken.contains(&wire_slot) {
            self.taken.push(wire_slot);
        }
        self.arm_auto_release();
    }

    /// A roll that resolved (`SMSG_LOOT_ROLL_WON`, `SMSG_LOOT_ALL_PASSED` → `0x6dc770`): the row's
    /// slot type drops to `ALLOW_LOOT` while the window is open on the roll's source.
    pub(crate) fn release_roll(&mut self, source: u64, wire_slot: u8) {
        if self.source != Some(source) {
            return;
        }
        if let Some(it) = self.items.iter_mut().find(|it| it.slot == wire_slot) {
            it.slot_type = slot_type::ALLOW_LOOT;
        }
    }

    /// `SMSG_LOOT_CLEAR_MONEY`: the coin row becomes a gap; an emptied window arms the auto-close.
    pub(crate) fn clear_money(&mut self) {
        self.gold = 0;
        self.arm_auto_release();
    }

    pub(crate) fn arm_auto_release(&mut self) {
        if self.source.is_some() && self.is_empty() {
            self.auto_release = true;
        }
    }

    pub(crate) fn take_auto_release(&mut self) -> bool {
        std::mem::take(&mut self.auto_release)
    }

    /// Closes the window; queued pushes outlive it.
    pub(crate) fn clear(&mut self) {
        self.source = None;
        self.gold = 0;
        self.coin_slot = false;
        self.items.clear();
        self.taken.clear();
        self.auto_release = false;
        self.fishing = false;
        self.loot_type = 0;
        self.pending_bind_confirm = None;
        self.master_candidates.clear();
        self.pending_master_candidates.clear();
    }

    /// Session end: drops the window and the queued pushes.
    pub(crate) fn clear_session(&mut self) {
        self.clear();
        self.receives.clear();
        self.wrath.reset();
    }

    /// Queued pushes, for the net handler's self-gate test.
    #[cfg(test)]
    pub(crate) fn pending_receive_count(&self) -> usize {
        self.receives.len()
    }

    pub(crate) fn has_coin(&self) -> bool {
        self.gold > 0
    }

    /// The open loot's guid; the chest lid watcher in [`crate::go_anim`] reads it too.
    pub(crate) fn source(&self) -> Option<u64> {
        self.source
    }

    /// Nothing lootable left: the reference's empty check `0x4c2a70`.
    pub(crate) fn is_empty(&self) -> bool {
        !self.has_coin() && self.items.iter().all(|it| self.taken.contains(&it.slot))
    }

    /// The candidate slots as `0x61c550` lays them out: wire order in a party (`0x61c5b9`); in a
    /// raid, each guid in the first free slot of its subgroup's block of five
    /// (`0x61c5c9`-`0x61c637`), holes kept for `GroupLootDropDown_Initialize`'s "Group N" submenus
    /// (`LootFrame.lua:197-213`). The feed and the drain share it, so an index means one candidate.
    pub(crate) fn placed_candidates(&self, group: &GroupState) -> Vec<Option<u64>> {
        if group.group_type != GROUPTYPE_RAID {
            return self.master_candidates.iter().copied().map(Some).collect();
        }
        let mut slots: Vec<Option<u64>> = vec![None; MASTER_LOOT_CANDIDATE_SLOTS];
        for &guid in &self.master_candidates {
            // A guid missing from the roster is ours: `SMSG_GROUP_LIST` lists only the others,
            // and `Group::MasterLoot` includes us.
            let flags = group
                .members
                .iter()
                .find(|m| m.guid == guid)
                .map_or(group.own_flags, |m| m.flags);
            let base = usize::from(flags & GROUP_MEMBER_SUBGROUP) * MEMBERS_PER_RAID_GROUP;
            let end = (base + MEMBERS_PER_RAID_GROUP).min(MASTER_LOOT_CANDIDATE_SLOTS);
            if let Some(free) = (base..end).find(|&i| slots[i].is_none()) {
                slots[free] = Some(guid);
            }
        }
        while slots.last().is_some_and(Option::is_none) {
            slots.pop(); // trailing empties read as nil either way
        }
        slots
    }

    /// The candidate at a 1-based menu index, through [`LootState::placed_candidates`].
    pub(crate) fn master_candidate(&self, index_1based: u32, group: &GroupState) -> Option<u64> {
        self.placed_candidates(group)
            .get(index_1based.checked_sub(1)? as usize)
            .copied()
            .flatten()
    }
}

/// The client's candidate array: 40 slots at `0xc4dc38`, bound-checked by the getter `0x61c660`.
pub(crate) const MASTER_LOOT_CANDIDATE_SLOTS: usize = 40;

/// A raid subgroup's block within it (`0x61c609`).
pub(crate) const MEMBERS_PER_RAID_GROUP: usize = 5;
