//! The client-volunteered pose bodies, sheath and stand state: the server stores what we send
//! unvalidated and echoes it through our unit fields, which every observer's body reads.

/// Body of `CMSG_SETSHEATHED`: a `u32` sheath state (0 stowed, 1 melee, 2 ranged), stored
/// unvalidated into `UNIT_FIELD_BYTES_2` (vmangos `CombatHandler.cpp:80-87`).
pub fn set_sheathed(state: u32) -> Vec<u8> {
    state.to_le_bytes().to_vec()
}
