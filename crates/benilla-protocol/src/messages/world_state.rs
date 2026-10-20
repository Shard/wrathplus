//! World states, the server's key/value table (`SMSG_INIT_WORLD_STATES` `0x2C2`,
//! `SMSG_UPDATE_WORLD_STATE` `0x2C3`) that NPC-text `$<n>w`/`$<n>e` tokens and scoreboards read.
//!
//! On `0x2C2` the reference clears the table and takes map and zone as its UI filter (`0x4c5650`)
//! before the pairs run. vmangos counts a trailing `(0, 0)` pair (`Player.cpp:8295-8369`), which
//! is read as data. Ids and values stay raw dwords: `$<n>e` reads the table at the negated key, so
//! `$2077e` looks up `0xFFFFF7E3`.

/// `SMSG_INIT_WORLD_STATES`: the whole table for a zone, sent on login and every zone change.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InitWorldStates {
    /// The map the states are scoped to; the reference keeps it at `[0xb71e84]`.
    pub map: u32,
    /// The zone vmangos fills in; the reference keeps it at `[0xb71ea8]` and matches it against
    /// `WorldStateUI.dbc`'s `AreaID`.
    pub zone: u32,
    /// `(id, value)` verbatim, including the trailing `(0, 0)` terminator.
    pub states: Vec<(u32, u32)>,
}
