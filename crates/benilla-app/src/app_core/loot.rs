//! The open loot window's state, its Wrath extension and the item pushes awaiting it, read by
//! world and net code; `ui_loot` keeps the systems and re-exports these from its old paths.

use std::collections::VecDeque;

use benilla_protocol::messages::LootItem;
use bevy::prelude::*;

#[derive(Default)]
pub(crate) struct WrathLootState {
    pub(crate) source: Option<u64>,
    pub(crate) phase: Phase,
    pub(crate) takes: VecDeque<u8>,
    pub(crate) money: bool,
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
