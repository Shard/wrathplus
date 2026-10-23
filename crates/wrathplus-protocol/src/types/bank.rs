//! Bank messages: open the window, buy a bag slot, move items in and out. The vault itself
//! streams in the player descriptor (bank slots 39-62, bank bags 63-68).

/// `SMSG_BUY_BANK_SLOT_RESULT`'s reason (`ERR_BANKSLOT_*`, vmangos `Player.h:91-94`).
pub mod bank_slot_result {
    pub const FAILED_TOO_MANY: u32 = 0;
    pub const INSUFFICIENT_FUNDS: u32 = 1;
    pub const NOTBANKER: u32 = 2;
    pub const OK: u32 = 3;
}
