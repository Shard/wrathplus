//! The honor wire messages: the inspect-honor request/reply pair and the honor-gain credit.
//! Durable honor state rides the player descriptor; these carry what it cannot, another player's
//! stats and the moment a kill pays out.

/// Another player's honor stats, the `MSG_INSPECT_HONOR_STATS` reply (opcode 726): 50 bytes in
/// vmangos `Misc.cpp:301-325` order, from the target's `PLAYER_FIELD_*` honor block. Their
/// current rank is not here; it streams publicly in `PLAYER_BYTES_3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectHonorStats {
    /// Echoed from the request. There is no error reply: a missing, distant (10 yd) or hostile
    /// target gets no answer at all (`MiscHandler.cpp:977`).
    pub player_guid: u64,
    /// The highest lifetime rank (`MiscHandler.cpp:985`), internal 0..18 like
    /// [`crate::messages::ObjectFields::player_honor_rank`], not the visual number.
    pub highest_rank: u8,
    /// Today's kills, the packed `PLAYER_FIELD_SESSION_KILLS` dword (`MiscHandler.cpp:988`).
    pub session_kills: u32,
    /// Yesterday's honorable kills, the low half of `PLAYER_FIELD_YESTERDAY_KILLS`.
    pub yesterday_hk: u16,
    /// Always 0 from vmangos (`MiscHandler.cpp:994`); decoded, not skipped.
    pub unknown_old1: u16,
    /// Last week's honorable kills.
    pub last_week_hk: u16,
    /// Always 0 from vmangos.
    pub unknown_old2: u16,
    /// This week's honorable kills.
    pub this_week_hk: u16,
    /// Always 0 from vmangos.
    pub unknown_old3: u16,
    /// Lifetime honorable kills (`PLAYER_FIELD_LIFETIME_HONORBALE_KILLS`, vmangos's spelling).
    pub lifetime_hk: u32,
    /// Lifetime dishonorable kills.
    pub lifetime_dhk: u32,
    /// Yesterday's honor points (`PLAYER_FIELD_YESTERDAY_CONTRIBUTION`).
    pub yesterday_honor: u32,
    /// Last week's honor points.
    pub last_week_honor: u32,
    /// This week's honor points.
    pub this_week_honor: u32,
    /// Last week's standing, the ladder position and not a rank; 0 if unranked that week.
    pub last_week_rank: u32,
    /// Progress within the target's current rank, 0..255, computed as
    /// [`crate::messages::ObjectFields::player_honor_rank_bar`] is.
    pub rank_bar: u8,
}

impl InspectHonorStats {
    /// The packed field split into `(honorable, dishonorable)`, low half then high half.
    pub fn session_kills(&self) -> (u16, u16) {
        (self.session_kills as u16, (self.session_kills >> 16) as u16)
    }
}

/// `SMSG_PVP_CREDIT` (opcode 652): one honor payout, dishonorable ones included (vmangos
/// `HonorMgr.cpp:1061-1093`). The descriptor holds only totals; this is the only per-payout notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PvpCredit {
    /// Honor, truncated server-side; negative for a dishonorable kill (`HonorMgr.cpp:807`).
    pub honor: i32,
    /// Who we killed; 0 for an honor row with no source (`HonorMgr.cpp:1069-1071`), which
    /// needs a victim-less phrasing rather than a name lookup.
    pub victim_guid: u64,
    /// The victim's internal rank (0..18), indexing the `PVP_RANK_<rank>_<team>` strings; the
    /// badge's visual rank is `internal > 4 ? internal - 4 : -internal` (`HonorMgr.cpp:991`).
    /// A creature sends 19 if a racial leader, else 0; vmangos floors a player victim at 5
    /// (`HonorMgr.cpp:1078-1089`), a server fallback rather than a rule to re-implement.
    pub victim_rank: i32,
}
