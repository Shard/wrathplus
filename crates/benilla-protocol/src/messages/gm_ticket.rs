//! The GM ticket family behind the Help window: create, update text, get, delete and queue
//! status, each a request and a `u32`-led answer; the get, delete and status requests are empty.

/// The second cstring of `CMSG_GMTICKET_CREATE`, the 1.12 client's constant at `0x860708`.
pub const RESERVED_FOR_FUTURE_USE: &str = "Reserved for future use";

/// `SMSG_GMTICKET_GETTICKET`'s status when a ticket follows (`GMTICKET_STATUS_HASTEXT`).
pub const GMTICKET_STATUS_HASTEXT: u32 = 0x06;

/// `SMSG_GMTICKET_GETTICKET`'s status for no ticket (`GMTICKET_STATUS_DEFAULT`): a 4-byte body,
/// and the ordinary answer to the client's post-login ask.
pub const GMTICKET_STATUS_DEFAULT: u32 = 0x0A;

/// The queue is accepting tickets (`GMTICKET_QUEUE_STATUS_ENABLED`).
pub const GMTICKET_QUEUE_ENABLED: i32 = 1;

/// The player's open ticket from `SMSG_GMTICKET_GETTICKET`, in wire order: text before category
/// (`GmTicket.cpp:66-79`), which the Lua `UPDATE_TICKET` event reverses. The `f32`s are days, and
/// a negative one means unknown (the stock Help frame then shows no wait time).
#[derive(Debug, Clone, PartialEq)]
pub struct GmTicket {
    /// What the player typed; vmangos appends a GM's answer here, as 1.12 has no reply channel
    /// (`GMTicketMgr.cpp:124-136`).
    pub text: String,
    /// The `GMTicketCategory.dbc` id the ticket was filed under (1..10).
    pub category: u8,
    /// Days since the ticket was last modified (FrameXML `arg3`).
    pub ticket_age: f32,
    /// Days since the realm's oldest open ticket was modified, 0 when none, negative when unknown;
    /// the FrameXML's wait estimate is this minus `ticket_age`.
    pub oldest_ticket_age: f32,
    /// Age of the oldest-ticket figure in days; negative or over 0.042 (about an hour) is stale.
    pub update_time: f32,
    /// 0 unassigned, 1 assigned to a GM, 2 escalated; vmangos clamps its 3 to 2
    /// (`GMTicketMgr.cpp:147`).
    pub assigned_to_gm: u8,
    /// 1 once a GM has actually opened the ticket, 0 before that.
    pub opened_by_gm: u8,
}

/// `CMSG_GMTICKET_CREATE` body as the 1.12 client sends it (`0x5ef740`): a `u8` category, the
/// player's map and position, the text, then [`RESERVED_FOR_FUTURE_USE`].
///
/// Deviation: the 1.12 client appends a zlib chat transcript to a category 2 ticket (`0x5ef936`);
/// we omit it because vmangos never reads it and logs the leftover bytes. The client also cuts
/// the text at 1999 chars (`0x5ef7fd`); we do not, as the Help window stops at 500.
pub fn gm_ticket_create(category: u8, map: u32, pos: [f32; 3], text: &str) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(1 + 4 + 12 + text.len() + 1 + RESERVED_FOR_FUTURE_USE.len() + 1);
    out.push(category);
    out.extend_from_slice(&map.to_le_bytes());
    for c in pos {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    out.extend_from_slice(RESERVED_FOR_FUTURE_USE.as_bytes());
    out.push(0);
    out
}

/// `CMSG_GMTICKET_UPDATETEXT` body: the category byte (`0x5efb2d`), then the text. vmangos stores
/// the byte, so an edit re-files the ticket under it (`GMTicketHandler.cpp:59-61`).
pub fn gm_ticket_updatetext(category: u8, text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + text.len() + 1);
    out.push(category);
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The field order and widths of the 1.12 client's sender (`0x5ef740`).
    #[test]
    fn the_create_body_is_a_category_byte_then_map_position_text_and_the_reserved_string() {
        let body = gm_ticket_create(4, 1, [-8949.95, -132.493, 83.5312], "My sword vanished.");
        let mut want = vec![0x04];
        want.extend_from_slice(&1u32.to_le_bytes());
        want.extend_from_slice(&(-8949.95f32).to_le_bytes());
        want.extend_from_slice(&(-132.493f32).to_le_bytes());
        want.extend_from_slice(&(83.5312f32).to_le_bytes());
        want.extend_from_slice(b"My sword vanished.\0");
        want.extend_from_slice(b"Reserved for future use\0");
        assert_eq!(body, want);
        assert_eq!(
            gm_ticket_create(2, 0, [0.0; 3], "x").len(),
            1 + 4 + 12 + 2 + 24,
            "no chat-log tail, even for category 2"
        );
    }

    /// As the 1.12 client's sender writes it (`0x5efac0`).
    #[test]
    fn the_updatetext_body_leads_with_the_category_byte() {
        assert_eq!(
            gm_ticket_updatetext(7, "Still stuck."),
            b"\x07Still stuck.\0"
        );
    }
}
