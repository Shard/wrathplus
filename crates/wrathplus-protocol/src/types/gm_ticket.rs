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
