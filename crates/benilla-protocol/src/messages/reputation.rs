//! The reputation pane's send verbs (vmangos `Misc.cpp`). A faction is addressed by its
//! reputation-list id (`Faction.dbc` `reputationIndex`, its `SMSG_INITIALIZE_FACTIONS` slot),
//! never by the DBC id or the panel row. None is acked: at-war and inactive return only at the
//! next login, so the client keeps its own copy; the watched index returns as a descriptor update.

/// Length of the reputation list `SMSG_INITIALIZE_FACTIONS` is positional in (vmangos `Misc.h:607`,
/// `MAX_FACTION_COUNT`); every `repListId` on this wire indexes it.
pub const FACTION_LIST_LEN: usize = 64;

/// "Watch no faction" for `CMSG_SET_WATCHED_FACTION` and `PLAYER_FIELD_WATCHED_FACTION_INDEX`. Not
/// 0: slot 0 is a real faction (Bloodsail Buccaneers) and vmangos stores the value as sent
/// (`HandleSetWatchedFactionOpcode`). FrameXML's row 0 means none; the binding maps it to this.
pub const WATCHED_FACTION_NONE: i32 = -1;
