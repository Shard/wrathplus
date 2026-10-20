//! The questgiver family (opcodes 386-402): the NPC accept, progress and reward panels, which
//! carry their own text. Layouts from vmangos `Server/Packets/Quest.cpp`.

/// vmangos `QUEST_EMOTE_COUNT` (`QuestDef.h:43`): the emote pairs DETAILS always writes.
pub const QUEST_EMOTE_COUNT: u32 = 4;

/// vmangos `DIALOG_STATUS_*` (`QuestDef.h:120-129`): an NPC's `SMSG_QUESTGIVER_STATUS`, which
/// drives the `!`/`?` over its head, and a quest row's `icon`.
pub mod dialog_status {
    pub const NONE: u32 = 0;
    pub const UNAVAILABLE: u32 = 1;
    pub const CHAT: u32 = 2;
    pub const INCOMPLETE: u32 = 3;
    pub const REWARD_REP: u32 = 4;
    pub const AVAILABLE: u32 = 5;
    pub const REWARD_OLD: u32 = 6;
    pub const REWARD2: u32 = 7;
}

/// One reward or choice item on a questgiver panel (vmangos `Quest.cpp:244-249`). `display_id`
/// is an `ItemDisplayInfo.dbc` row; an all-zero triple means the server had no item template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestRewardItem {
    pub item_id: u32,
    pub count: u32,
    pub display_id: u32,
}

/// A required item on the progress panel, the same triple (vmangos `Quest.cpp:376-381`).
pub type QuestRequiredItem = QuestRewardItem;

/// Native 3.3.5 quest columns (AzerothCore GossipDef.cpp); absent on vanilla packets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WrathQuestInfo {
    pub flags: u32,
    pub repeatable: bool,
    pub suggested_players: u32,
    pub auto_accept: bool,
    pub finished: bool,
    pub reward_xp: u32,
    /// Query response's QuestXP.dbc column index, not a concrete XP award.
    pub reward_xp_id: u32,
    /// Panels carry honor times ten; query responses carry unscaled honor addition.
    pub reward_honor: u32,
    pub honor_multiplier: f32,
    pub title_id: u32,
    pub bonus_talents: u32,
    pub arena_points: u32,
    /// The query response's `RequiredPlayerKills`, the PvP objective; its count is slot counter
    /// [`QUEST_PVP_KILL_COUNTER`] (AzerothCore `QuestDef.h:43`).
    pub players_slain: u32,
    /// The query response's reputation rewards by slot: `RewardFactionId`, `RewardFactionValueId`
    /// and `RewardFactionValueIdOverride`, sent as three runs of five (AzerothCore
    /// `QuestDef.cpp:391-398`).
    pub reward_factions: [(u32, i32, i32); 5],
    /// `RewardRepShowMask`, which AzerothCore always sends 0 (`QuestDef.cpp:368`).
    pub reward_rep_show_mask: u32,
    /// The objectives' `ItemDrop` and `ItemDropQuantity`, the source items the quest log lists.
    pub item_drops: [(u32, u32); 4],
}

/// The quest-log slot counter a PvP objective counts in, `QUEST_PVP_KILL_SLOT`: the server bumps
/// it beside `SMSG_QUESTUPDATE_ADD_PVP_KILL` (AzerothCore `PlayerQuest.cpp:2563`).
pub const QUEST_PVP_KILL_COUNTER: usize = 0;

/// One greeting-panel quest row; `icon` is a [`dialog_status`] value, a `u32` (`Quest.h:270`).
#[derive(Debug, Clone, PartialEq)]
pub struct QuestListEntry {
    pub wrath: Option<WrathQuestInfo>,
    pub quest_id: u32,
    pub icon: u32,
    pub level: u32,
    pub title: String,
}

/// `SMSG_QUESTGIVER_QUEST_LIST` (vmangos `Quest.cpp:159-203`): the multi-quest greeting panel.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestGiverList {
    pub npc: u64,
    pub greeting: String,
    pub emote_delay: u32,
    pub emote: u32,
    pub quests: Vec<QuestListEntry>,
}

/// `SMSG_QUESTGIVER_QUEST_DETAILS` (vmangos `Quest.cpp:205-273`): the accept panel. `money` is
/// signed, negative for a quest that costs money (`GetRewOrReqMoney`).
#[derive(Debug, Clone, PartialEq)]
pub struct QuestDetails {
    pub wrath: Option<WrathQuestInfo>,
    pub npc: u64,
    pub quest_id: u32,
    pub title: String,
    pub details: String,
    pub objectives: String,
    pub auto_finish: u32,
    pub choices: Vec<QuestRewardItem>,
    pub rewards: Vec<QuestRewardItem>,
    pub money: i32,
    pub reward_spell: u32,
}

/// `SMSG_QUESTGIVER_OFFER_REWARD` (vmangos `Quest.cpp:275-339`): the reward panel.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestOfferReward {
    pub wrath: Option<WrathQuestInfo>,
    pub npc: u64,
    pub quest_id: u32,
    pub title: String,
    pub offer_text: String,
    pub auto_finish: u32,
    pub choices: Vec<QuestRewardItem>,
    pub rewards: Vec<QuestRewardItem>,
    pub money: i32,
    pub quest_flags: u32,
    pub reward_spell: u32,
}

/// `SMSG_QUESTGIVER_REQUEST_ITEMS` (vmangos `Quest.cpp:341-391`): the progress panel. The client
/// ANDs four flag words to enable Continue; `is_complete` comes from the second (`Quest.cpp:388`).
#[derive(Debug, Clone, PartialEq)]
pub struct QuestRequestItems {
    pub wrath: Option<WrathQuestInfo>,
    pub npc: u64,
    pub quest_id: u32,
    pub title: String,
    pub request_text: String,
    pub emote: u32,
    pub close_on_cancel: u32,
    pub required_money: u32,
    pub required_items: Vec<QuestRequiredItem>,
    pub is_complete: bool,
}

/// `SMSG_QUESTGIVER_QUEST_COMPLETE` (vmangos `Quest.cpp:96-108`): the XP, money and fixed items
/// granted; the item rows carry no display id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestComplete {
    pub quest_id: u32,
    pub xp: u32,
    pub money: u32,
    pub items: Vec<(u32, u32)>,
}

// ── CMSG encoders (vmangos `Quest.cpp:6-50`) ─────────────────────────────────────────────────────

/// Body of `CMSG_QUESTGIVER_STATUS_QUERY` (`Quest.cpp:11-14`): the `u64` NPC guid.
pub fn questgiver_status_query(npc: u64) -> Vec<u8> {
    npc.to_le_bytes().to_vec()
}

/// Body of `CMSG_QUESTGIVER_HELLO` (`Quest.cpp:16-19`): the NPC guid. The server answers it like
/// `CMSG_GOSSIP_HELLO`, through `SendPreparedGossip` (`QuestHandler.cpp:104`).
pub fn questgiver_hello(npc: u64) -> Vec<u8> {
    npc.to_le_bytes().to_vec()
}

/// Body of `CMSG_QUESTGIVER_QUERY_QUEST` (`Quest.cpp:27-31`): `u64 guid, u32 quest`, sent by a
/// quest row's click and always answered with the details panel (`QuestHandler.cpp:224`).
pub fn questgiver_query_quest(npc: u64, quest: u32) -> Vec<u8> {
    guid_quest(npc, quest)
}

/// Body of `CMSG_QUESTGIVER_ACCEPT_QUEST` (`Quest.cpp:21-25`): `u64 guid, u32 quest`. The server
/// logs the quest, then closes the gossip window (`SMSG_GOSSIP_COMPLETE`).
pub fn questgiver_accept_quest(npc: u64, quest: u32) -> Vec<u8> {
    guid_quest(npc, quest)
}

/// Body of `CMSG_QUESTGIVER_COMPLETE_QUEST` (`Quest.cpp:46-50`): `u64 guid, u32 quest`, answered
/// with the progress panel (`QuestHandler.cpp:383-397`).
pub fn questgiver_complete_quest(npc: u64, quest: u32) -> Vec<u8> {
    guid_quest(npc, quest)
}

/// Body of `CMSG_QUESTGIVER_REQUEST_REWARD` (`Quest.cpp:40-44`): `u64 guid, u32 quest`, the
/// progress panel's Continue, answered with the reward panel (`QuestHandler.cpp:286-312`).
pub fn questgiver_request_reward(npc: u64, quest: u32) -> Vec<u8> {
    guid_quest(npc, quest)
}

/// Body of `CMSG_QUESTGIVER_CHOOSE_REWARD` (`Quest.cpp:33-38`): `u64 guid, u32 quest, u32 reward`,
/// `reward` a 0-based choice index (0 with no choice). Sent by the reward panel's Complete and
/// answered with `SMSG_QUESTGIVER_QUEST_COMPLETE` (`QuestHandler.cpp:239-284`).
pub fn questgiver_choose_reward(npc: u64, quest: u32, reward: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(16);
    body.extend_from_slice(&npc.to_le_bytes());
    body.extend_from_slice(&quest.to_le_bytes());
    body.extend_from_slice(&reward.to_le_bytes());
    body
}

fn guid_quest(npc: u64, quest: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&npc.to_le_bytes());
    body.extend_from_slice(&quest.to_le_bytes());
    body
}

// ── SMSG parsers ─────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Byte-buffer builders ─────────────────────────────────────────────────────────────────────
    fn push_u32(b: &mut Vec<u8>, v: u32) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    fn push_u64(b: &mut Vec<u8>, v: u64) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    // ── CMSG encode goldens ──────────────────────────────────────────────────────────────────────

    #[test]
    fn cmsg_guid_only_bodies() {
        assert_eq!(
            questgiver_status_query(0x0102_0304_0506_0708),
            vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );
        assert_eq!(questgiver_hello(0xAABB), vec![0xBB, 0xAA, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn cmsg_guid_quest_bodies() {
        let mut want = Vec::new();
        push_u64(&mut want, 0x42);
        push_u32(&mut want, 3);
        assert_eq!(questgiver_query_quest(0x42, 3), want);
        assert_eq!(questgiver_accept_quest(0x42, 3), want);
        assert_eq!(questgiver_complete_quest(0x42, 3), want);
        assert_eq!(questgiver_request_reward(0x42, 3), want);
    }

    #[test]
    fn cmsg_choose_reward_body() {
        let mut want = Vec::new();
        push_u64(&mut want, 0x99);
        push_u32(&mut want, 40);
        push_u32(&mut want, 2);
        assert_eq!(questgiver_choose_reward(0x99, 40, 2), want);
    }

    // ── SMSG parse fixtures ──────────────────────────────────────────────────────────────────────
}
