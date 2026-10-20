//! The party quest-share wire (411-413, 630). The sharer gets one `MSG_QUEST_PUSH_RESULT` per
//! member; an eligible member gets an ordinary `SMSG_QUESTGIVER_QUEST_DETAILS` with the sharer's
//! player guid as `npcGuid`, and Accept sends `CMSG_QUESTGIVER_ACCEPT_QUEST` to that guid
//! (`QuestHandler.cpp:111-114, 403-459`). The push-result guid names a different player each way.

/// `QuestShareMessages` (AzerothCore `QuestDef.h:64-77`): the `u8` verdict of
/// `MSG_QUEST_PUSH_RESULT`. A newtype, not an enum, so an unknown value survives the parse; build
/// 12340 also names 11 (`ERR_QUEST_PUSH_DIFFERENT_SERVER_DAILY_S`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QuestShareMsg(pub u8);

impl QuestShareMsg {
    /// `QUEST_PARTY_MSG_SHARING_QUEST`: the push went out to this member.
    pub const SHARING_QUEST: Self = Self(0);
    /// `QUEST_PARTY_MSG_CANT_TAKE_QUEST`: the member fails `CanTakeQuest` (level, race, prereqs).
    pub const CANT_TAKE_QUEST: Self = Self(1);
    /// `QUEST_PARTY_MSG_ACCEPT_QUEST`: sent by the server when the member accepts.
    pub const ACCEPT_QUEST: Self = Self(2);
    /// `QUEST_PARTY_MSG_DECLINE_QUEST`: the one value the client sends.
    pub const DECLINE_QUEST: Self = Self(3);
    /// `QUEST_PARTY_MSG_BUSY`: the member already has a share latched.
    pub const BUSY: Self = Self(4);
    /// `QUEST_PARTY_MSG_LOG_FULL`: the member's quest log is full.
    pub const LOG_FULL: Self = Self(5);
    /// `QUEST_PARTY_MSG_HAVE_QUEST`: the member is already on the quest.
    pub const HAVE_QUEST: Self = Self(6);
    /// `QUEST_PARTY_MSG_FINISH_QUEST`: the member has already completed the quest.
    pub const FINISH_QUEST: Self = Self(7);
    /// `QUEST_PARTY_MSG_CANT_BE_SHARED_TODAY`: a daily the sharer cannot share again today.
    pub const CANT_BE_SHARED_TODAY: Self = Self(8);
    /// `QUEST_PARTY_MSG_SHARING_TIMER_EXPIRED`: a timed quest whose timer ran out.
    pub const SHARING_TIMER_EXPIRED: Self = Self(9);
    /// `QUEST_PARTY_MSG_NOT_IN_PARTY`: the sharer is in no group.
    pub const NOT_IN_PARTY: Self = Self(10);
    /// The client's `ERR_QUEST_PUSH_DIFFERENT_SERVER_DAILY_S`, which AzerothCore never sends.
    pub const DIFFERENT_SERVER_DAILY: Self = Self(11);
}

/// `MSG_QUEST_PUSH_RESULT` from the server (`Quest.cpp:81-85`). Its guid is the member the verdict
/// is about, never the sharer, and fills the `%s` of `ERR_QUEST_PUSH_*` (`Player.cpp:14596-14608`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestPushResult {
    pub member: u64,
    pub msg: QuestShareMsg,
}

/// `SMSG_QUEST_CONFIRM_ACCEPT` (`Quest.cpp:131-136`): the escort confirm, sent to the other
/// eligible members when one accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestConfirmAccept {
    /// Echoed back in [`quest_confirm_accept`].
    pub quest_id: u32,
    /// Server-localized (`Player.cpp:14575-14586`); the receiver may not have the quest cached.
    pub title: String,
    /// The member who accepted: the first `%s` of `QUEST_ACCEPT`.
    pub sender: u64,
}

// ── CMSG encoders ────────────────────────────────────────────────────────────────────────────────

/// Body of `CMSG_QUEST_CONFIRM_ACCEPT` (`Quest.cpp:63-66`): the escort confirm's yes. A no sends
/// nothing (`QuestHandler.cpp:172-193`).
pub fn quest_confirm_accept(quest_id: u32) -> Vec<u8> {
    quest_id.to_le_bytes().to_vec()
}

/// Body of `MSG_QUEST_PUSH_RESULT` from the client (`Quest.cpp:73-77`): the sharer being answered,
/// the `npcGuid` the shared DETAILS came under. vmangos ignores it (`QuestHandler.cpp:461-467`).
pub fn quest_push_result(sharer: u64, msg: QuestShareMsg) -> Vec<u8> {
    let mut b = Vec::with_capacity(9);
    b.extend_from_slice(&sharer.to_le_bytes());
    b.push(msg.0);
    b
}

// ── SMSG readers ─────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_result_body_is_guid_then_msg() {
        let b = quest_push_result(0x0000_0000_0000_002A, QuestShareMsg::DECLINE_QUEST);
        assert_eq!(b, vec![0x2A, 0, 0, 0, 0, 0, 0, 0, 3]);
    }
}
