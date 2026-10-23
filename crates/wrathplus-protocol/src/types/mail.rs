//! Mail wire messages. There is no `SMSG_SHOW_MAILBOX`: the client opens the window itself. Each
//! mailbox CMSG leads with the mailbox guid, range-checked (5 yd) by vmangos `CheckMailBox`.

/// [`MailListEntry::message_type`]: each value fixes the form of the row's sender field.
pub mod mail_message_type {
    /// A player: the sender field is an 8-byte guid.
    pub const NORMAL: u8 = 0;
    /// An auction-house notice: the sender field is a u32 auction id.
    pub const AUCTION: u8 = 2;
    /// A creature: the sender field is a u32 creature entry.
    pub const CREATURE: u8 = 3;
    /// A GameObject: the sender field is a u32 GameObject entry.
    pub const GAMEOBJECT: u8 = 4;
    /// A pre-made item mail (a GM or starter mail): no sender field at all.
    pub const ITEM: u8 = 5;
}

/// `SMSG_SEND_MAIL_RESULT`'s `action` (vmangos `Mail.h` `MailResponseType`).
pub mod mail_action {
    pub const SEND: u32 = 0;
    pub const MONEY_TAKEN: u32 = 1;
    pub const ITEM_TAKEN: u32 = 2;
    pub const RETURNED: u32 = 3;
    pub const DELETED: u32 = 4;
    pub const MADE_PERMANENT: u32 = 5;
}

/// `SMSG_SEND_MAIL_RESULT`'s `error` (vmangos `Mail.h` `MailResponseResult`).
pub mod mail_error {
    pub const OK: u32 = 0;
    pub const EQUIP_ERROR: u32 = 1;
    pub const CANNOT_SEND_TO_SELF: u32 = 2;
    pub const NOT_ENOUGH_MONEY: u32 = 3;
    pub const RECIPIENT_NOT_FOUND: u32 = 4;
    pub const NOT_YOUR_TEAM: u32 = 5;
    pub const INTERNAL_ERROR: u32 = 6;
    pub const TRIAL_ACCOUNT: u32 = 14;
    pub const TOO_MANY_ATTACHMENTS: u32 = 15;
}

/// A mail's one attachment (`MAX_MAIL_ITEMS` is 1 in 1.12): a fixed-width block present on every
/// `SMSG_MAIL_LIST_RESULT` row, all zero when there is no item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailAttachment {
    pub entry: u32,
    pub perm_enchant: u32,
    pub random_prop_id: u32,
    pub suffix_factor: u32,
    pub count: u8,
    pub charges: u32,
    pub durability_max: u32,
    pub durability: u32,
}

/// One `SMSG_MAIL_LIST_RESULT` row (vmangos `HandleGetMailList`, `Handlers/MailHandler.cpp`).
/// `sender_guid` and `sender_id` are one wire field, whose form `message_type` picks.
#[derive(Debug, Clone, PartialEq)]
pub struct MailListEntry {
    pub message_id: u32,
    pub message_type: u8,
    pub sender_guid: Option<u64>,
    pub sender_id: Option<u32>,
    pub subject: String,
    /// `0` = no letter body to fetch; nonzero = a [`super::item_text_query`] key.
    pub item_text_id: u32,
    pub stationery: u32,
    pub item: Option<MailAttachment>,
    pub money: u32,
    pub cod: u32,
    /// vmangos `Mail.h` flags: READ 0x1, RETURNED 0x2, COPIED 0x4, COD_PAYMENT 0x8, HAS_BODY 0x10.
    pub checked: u32,
    pub expire_days: f32,
    pub mail_template_id: u32,
}

fn mailbox_and_mail_id(mailbox: u64, mail_id: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&mailbox.to_le_bytes());
    body.extend_from_slice(&mail_id.to_le_bytes());
    body
}

/// `CMSG_MAIL_TAKE_MONEY`: answered by a [`mail_action::MONEY_TAKEN`] result.
pub fn mail_take_money(mailbox: u64, mail_id: u32) -> Vec<u8> {
    mailbox_and_mail_id(mailbox, mail_id)
}

/// `CMSG_ITEM_TEXT_QUERY` (vmangos `Server/Packets/Mail.cpp`): `u32 textId, u32 mailId, u32 unk`.
/// The 1.12 client writes all three; vmangos only uses `textId`.
pub fn item_text_query(text_id: u32, mail_id: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&text_id.to_le_bytes());
    body.extend_from_slice(&mail_id.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes()); // the unk vmangos never reads back
    body
}
