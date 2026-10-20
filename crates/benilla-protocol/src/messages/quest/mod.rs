//! Quest messages: [`giver`] for accepting and turning in at an NPC (386-402), [`log`] for the
//! quest log (92/93, 403-410), and `share` for the party quest share (411-413, 630).

mod giver;
mod log;
mod share;

pub use giver::{
    dialog_status, QuestComplete, QuestDetails, QuestGiverList, QuestListEntry, QuestOfferReward,
    QuestRequestItems, QuestRequiredItem, QuestRewardItem, WrathQuestInfo, QUEST_EMOTE_COUNT,
    QUEST_PVP_KILL_COUNTER,
};
pub use log::{
    quest_flags, QuestObjective,
    QuestTemplate, QUEST_OBJECTIVES_COUNT, QUEST_REWARDS_COUNT, QUEST_REWARD_CHOICES_COUNT,
};
pub use share::{
    quest_confirm_accept, quest_push_result, QuestConfirmAccept,
    QuestPushResult, QuestShareMsg,
};

