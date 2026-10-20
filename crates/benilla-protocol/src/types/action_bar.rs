//! Action-bar messages: the 120-slot login snapshot, the one-slot write, and the bar toggles.
//! The bar is client-authoritative: the server stores the slots, sends them at login, and takes
//! one `CMSG_SET_ACTION_BUTTON` per local change.

/// The action-button kind, bits 24-31 of a slot word (`ActionButtonType`, vmangos `Player.h`).
pub const ACTION_KIND_SPELL: u8 = 0x00;
pub const ACTION_KIND_MACRO: u8 = 0x40;
pub const ACTION_KIND_ITEM: u8 = 0x80;

/// An occupied slot of `SMSG_ACTION_BUTTONS`, 120 packed `u32`s
/// (`MasterPlayer::SendInitialActionButtons`); a zero word is an empty slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionButton {
    /// The bar slot index (0..119). Slots 0–11 are the main bar's buttons 1–12.
    pub slot: u8,
    /// The spell/macro/item id (bits 0–23).
    pub action: u32,
    /// The kind byte (bits 24-31), an `ACTION_KIND_*`; the enum's 0x01 ("click?") is carried raw.
    pub kind: u8,
}

/// `CMSG_SET_ACTION_BUTTON` (296): `button u8`, `packetData u32` packed as in [`ActionButton`]
/// (`Server/Packets/Misc.cpp:87-90`). Zero clears the slot (no reply); a drag-swap is two sends.
pub fn set_action_button(button: u8, packed: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(5);
    body.push(button);
    body.extend_from_slice(&packed.to_le_bytes());
    body
}
