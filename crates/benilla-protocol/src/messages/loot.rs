//! Loot wire messages: the loot window, group rolls, master loot and item-push results.

/// One item row of `SMSG_LOOT_RESPONSE` (vmangos `LootMgr.cpp:837-845,900-912`). `slot` is the
/// 0-based wire slot that [`autostore_loot_item`] sends back; quest items follow at
/// `items.len() + i` (`LootMgr.cpp:970`). The wire's `randomSuffix` is always 0 and is discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootItem {
    pub slot: u8,
    pub item_id: u32,
    pub count: u32,
    /// `item_template.display_id` → icon via `ItemDisplayInfo.dbc`.
    pub display_info_id: u32,
    pub random_property_id: u32,
    /// A [`slot_type`] code; solo looting always writes `ALLOW_LOOT` (`LootMgr.cpp:918-926`).
    pub slot_type: u8,
}

/// `LootSlotType` (vmangos `LootMgr.h:75-83`): the `u8` that ends each [`LootItem`] row.
pub mod slot_type {
    pub const ALLOW_LOOT: u8 = 0;
    /// A group roll is in progress on this item: shown, not yet lootable.
    pub const ROLL_ONGOING: u8 = 1;
    /// Only the loot master can hand this out ([`loot_master_give`]). Under master loot vmangos
    /// sends it on every row, even greys its take handler allows (`LootMgr.cpp:917-941`).
    pub const MASTER: u8 = 2;
    /// Shown red and not lootable: a requirement the viewer fails, in a group.
    pub const LOCKED: u8 = 3;
    /// Owner-permission solo looting, binding checks skipped.
    pub const OWNER: u8 = 4;
}

/// `LootType` (vmangos `LootMgr.h:49-61`). Only these four reach the wire: `SendLoot` remaps
/// skinning and insignia to `PICKPOCKETING`, fishing holes and fails to `FISHING`
/// (`Player.cpp:8120-8136`); `CMSG_LOOT` always gets `CORPSE` (`LootHandler.cpp:380`).
pub mod loot_type {
    pub const CORPSE: u8 = 1;
    pub const PICKPOCKETING: u8 = 2;
    pub const FISHING: u8 = 3;
    pub const DISENCHANTING: u8 = 4;
}

/// `LootError` (vmangos `LootMgr.h:85-100`): the code on the error shape of `SMSG_LOOT_RESPONSE`.
/// `CMSG_LOOT` can draw `DIDNT_KILL`, `PLAYER_NOT_FOUND`, `PLAY_TIME_EXCEEDED`, `NOTSTANDING`,
/// `STUNNED` (`LootHandler.cpp:342-373`) and `TOO_FAR` (`Player.cpp:7952-7956`); the `MASTER_*`
/// codes answer a refused [`loot_master_give`], to the master looter only.
pub mod loot_error {
    pub const DIDNT_KILL: u8 = 0;
    pub const TOO_FAR: u8 = 4;
    pub const BAD_FACING: u8 = 5;
    pub const LOCKED: u8 = 6;
    pub const NOTSTANDING: u8 = 8;
    pub const STUNNED: u8 = 9;
    pub const PLAYER_NOT_FOUND: u8 = 10;
    pub const PLAY_TIME_EXCEEDED: u8 = 11;
    pub const MASTER_INV_FULL: u8 = 12;
    pub const MASTER_UNIQUE_ITEM: u8 = 13;
    pub const MASTER_OTHER: u8 = 14;
    pub const ALREADY_PICKPOCKETED: u8 = 15;
    pub const NOT_WHILE_SHAPESHIFTED: u8 = 16;
}

/// The two shapes of `SMSG_LOOT_RESPONSE`: a `lootType` of 0 after the guid is the error shape.
#[derive(Debug, Clone, PartialEq)]
pub enum LootResponseBody {
    /// `u32 gold, u8 itemCount`, then the [`LootItem`] rows (vmangos `LootMgr.cpp:848-873`).
    Items {
        loot_type: u8,
        gold: u32,
        items: Vec<LootItem>,
    },
    /// Only a [`loot_error`] code follows (`Player.cpp:7744-7750`).
    Error { error: u8 },
}

/// `CMSG_LOOT` (`Loot.cpp:8-11`): the guid to open, always answered by `SMSG_LOOT_RESPONSE`.
pub fn loot(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `CMSG_AUTOSTORE_LOOT_ITEM` (`Loot.cpp:3-6`): a [`LootItem::slot`]; the server picks the bag.
pub fn autostore_loot_item(loot_slot: u8) -> Vec<u8> {
    vec![loot_slot]
}

/// `CMSG_LOOT_MONEY`: an empty body (vmangos `Opcodes_1_12_1.h:351`).
pub fn loot_money() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_LOOT_RELEASE` (`Loot.cpp:13-16`): a guid the server ignores for the loot guid it stored.
pub fn loot_release(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `RollVote` (vmangos `Group/Group.h:88-99`): the server rejects a vote `>= 3`
/// (`GroupHandler.cpp:367-368`). Overloaded on `SMSG_LOOT_ROLL`: see [`LootRoll::is_dice`].
pub mod roll_vote {
    pub const PASS: u8 = 0;
    pub const NEED: u8 = 1;
    pub const GREED: u8 = 2;
}

/// `CMSG_LOOT_ROLL` (`Loot.cpp:18-23`): `u64 lootedTarget, u32 itemSlot, u8 rollType`. The
/// corpse and slot identify the roll; the FrameXML `rollID` never reaches the wire.
pub fn loot_roll(looted_target: u64, item_slot: u32, roll_type: u8) -> Vec<u8> {
    let mut body = looted_target.to_le_bytes().to_vec();
    body.extend_from_slice(&item_slot.to_le_bytes());
    body.push(roll_type);
    body
}

/// `CMSG_LOOT_MASTER_GIVE` (`Loot.cpp:25-30`): `u64 lootGuid, u8 slotId, u64 playerGuid`; the
/// slot is a `u8` here, a `u32` in the roll family. A refusal is a `MASTER_*` [`loot_error`].
pub fn loot_master_give(loot_guid: u64, slot: u8, player_guid: u64) -> Vec<u8> {
    let mut body = loot_guid.to_le_bytes().to_vec();
    body.push(slot);
    body.extend_from_slice(&player_guid.to_le_bytes());
    body
}

/// `SMSG_LOOT_START_ROLL` (`Loot.cpp:43-51`): a group roll opened on one drop, sent to every
/// eligible roller. Its `randomSuffix` is always 0 (`Group/Group.cpp:765`) and is discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootStartRoll {
    pub looted_target: u64,
    /// The wire [`LootItem::slot`], carried as a `u32` here.
    pub item_slot: u32,
    pub item_id: u32,
    pub random_property_id: u32,
    /// How long the roll stays open, in ms (vmangos: `60_000`, `Group.cpp:67`).
    pub countdown_ms: u32,
}

/// `SMSG_LOOT_ROLL` (`Loot.cpp:53-63`): one player's vote or dice roll, sent to every roller.
/// `(roll_number, roll_type)` is overloaded: a vote is `(0, 0)` Need, `(128, 128)` Pass or
/// `(128, 2)` Greed; a dice result is `(1..=100, 1 or 2)` (`Group/Group.cpp:970-990,1163,1214`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootRoll {
    pub looted_target: u64,
    pub item_slot: u32,
    pub roller: u64,
    pub item_id: u32,
    pub random_property_id: u32,
    pub roll_number: u8,
    pub roll_type: u8,
}

impl LootRoll {
    /// A dice result rather than a vote: vmangos rolls `urand(1, 100)` (`Group.cpp:1162,1213`).
    pub fn is_dice(&self) -> bool {
        (1..=100).contains(&self.roll_number)
    }

    /// The [`roll_vote`] this announcement reports, when it is a vote.
    pub fn vote(&self) -> Option<u8> {
        match (self.roll_number, self.roll_type) {
            (0, 0) => Some(roll_vote::NEED),
            (128, 128) => Some(roll_vote::PASS),
            (128, roll_vote::GREED) => Some(roll_vote::GREED),
            _ => None,
        }
    }
}

/// `SMSG_LOOT_ROLL_WON` (`Loot.cpp:65-75`): `winner` took the item. Unlike [`LootRoll`], the
/// winner guid comes after the item fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootRollWon {
    pub looted_target: u64,
    pub item_slot: u32,
    pub item_id: u32,
    pub random_property_id: u32,
    pub winner: u64,
    /// The winning dice value, or 100 with Need when only one player may roll (`Group.cpp:1110`).
    pub roll_number: u8,
    /// The [`roll_vote`] that won.
    pub roll_type: u8,
}

/// `SMSG_LOOT_ALL_PASSED` (`Loot.cpp:77-84`): everyone passed; the item stays for normal looting.
/// Unlike the rest of the family, `itemRandomPropId` comes before `randomSuffixId` here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootAllPassed {
    pub looted_target: u64,
    pub item_slot: u32,
    pub item_id: u32,
    pub random_property_id: u32,
}

/// `SMSG_ITEM_PUSH_RESULT` (vmangos `Server/Packets/Item.cpp:211-224`, both post-1.10.2 fields
/// present at 5875): the "You receive …" chat line. `item_slot` is `0xFFFF_FFFF` when it stacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemPushResult {
    pub player_guid: u64,
    /// `false` = looted, `true` = received from an NPC (vendor purchase, quest reward).
    pub from_npc: bool,
    /// Crafted or conjured, giving the "You create: …" line; not a stack-merge flag (vmangos
    /// `SendNewItem`: "0=received, 1=created").
    pub created: bool,
    pub show_in_chat: bool,
    /// The bag it went to, numbered as [`super::items::BAG_PLAYER_INVENTORY`] and
    /// [`super::items::SLOT_BAG_FIRST`].
    pub bag_slot: u8,
    pub item_slot: u32,
    pub item_entry: u32,
    pub suffix_factor: u32,
    pub random_property_id: u32,
    pub count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::opcode;

    fn hx(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn cmsg_bodies_golden() {
        assert_eq!(
            loot(0x1234_5678_9abc_def0),
            hx("f0debc9a78563412"),
            "CMSG_LOOT body"
        );

        assert_eq!(
            autostore_loot_item(3),
            hx("03"),
            "CMSG_AUTOSTORE_LOOT_ITEM body"
        );

        assert_eq!(loot_money(), Vec::<u8>::new(), "CMSG_LOOT_MONEY body");

        assert_eq!(
            loot_release(0x1234_5678_9abc_def0),
            hx("f0debc9a78563412"),
            "CMSG_LOOT_RELEASE body"
        );

        assert_eq!(
            loot_roll(0x1234_5678_9abc_def0, 2, roll_vote::GREED),
            hx("f0debc9a785634120200000002"),
            "CMSG_LOOT_ROLL body"
        );

        let give = loot_master_give(0x1234_5678_9abc_def0, 2, 0x0fed_cba9_8765_4321);
        assert_eq!(
            give,
            hx("f0debc9a785634120221436587a9cbed0f"),
            "CMSG_LOOT_MASTER_GIVE body"
        );
        assert_eq!(give.len(), 17, "8 + 1 + 8, not 8 + 4 + 8");
    }

    /// The roll and master-loot opcodes (vmangos `Opcodes_1_12_1.h:671-677`).
    #[test]
    fn roll_opcode_values() {
        assert_eq!(opcode::SMSG_LOOT_ALL_PASSED, 670);
        assert_eq!(opcode::SMSG_LOOT_ROLL_WON, 671);
        assert_eq!(opcode::CMSG_LOOT_ROLL, 672);
        assert_eq!(opcode::SMSG_LOOT_START_ROLL, 673);
        assert_eq!(opcode::SMSG_LOOT_ROLL, 674);
        assert_eq!(opcode::CMSG_LOOT_MASTER_GIVE, 675);
        assert_eq!(opcode::SMSG_LOOT_MASTER_LIST, 676);
    }
}
