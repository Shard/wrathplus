//! Meeting-stone wire messages: the queue state, the right-click join and the leave request.

/// `SMSG 0x295` (reference handler `0x4ca230`): the queued area and a status byte, which the
/// client turns into one of five messages before firing `MEETINGSTONE_CHANGED`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeetingStoneSetQueue {
    pub area: u32,
    pub status: u8,
}

/// Display-only replies (reference handler `0x4ca3c0`): none changes the queue or fires an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeetingStoneNotice {
    /// `0x297`, empty.
    Success,
    /// `0x298`, empty.
    InProgress,
    /// `0x299`, `u64 guid`; the line waits for the name cache.
    MemberAdded { guid: u64 },
    /// `0x2BB`, `u8 code`.
    JoinFailed { code: u8 },
}
