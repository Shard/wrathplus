//! The seven GM ticket globals `HelpFrame.lua` calls. The five ticket verbs share one ordered
//! queue, because call order is the reference's wire order, and two calls are two sends: the
//! ticket toast re-polls `GetGMTicket()` every 10 minutes. `GetGMTicket()` is a send, answered by
//! the `UPDATE_TICKET` event, so nothing here holds a ticket.

use mlua::{Lua, MultiValue, Value};

use super::Model;

/// One ticket verb the window called, drained in call order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GmTicketIntent {
    /// `GetGMTicket()`: `CMSG_GMTICKET_GETTICKET`.
    Ask,
    /// `GetGMStatus()`: `CMSG_GMTICKET_SYSTEMSTATUS`.
    AskStatus,
    /// `DeleteGMTicket()`: `CMSG_GMTICKET_DELETETICKET`.
    Delete,
    /// `NewGMTicket` or `UpdateGMTicket`.
    Write(GmTicketWrite),
    /// Build 12340 `GMReportLag(type)` (`0x5ad020` → `0x5acf30`): `CMSG_GM_REPORT_LAG`.
    ReportLag { lag_type: u32 },
    /// Build 12340 `GMResponseNeedMoreHelp(text)` (`0x5ad150` → `0x5ac8f0(text, 1, 1)`): a new
    /// ticket asking for a response and flagged needs-more-help, with the caller's text.
    NeedMoreHelp { text: String },
}

/// One `NewGMTicket(ticketDescription, needResponse)` or `UpdateGMTicket(ticketDescription)`
/// call, build 12340's shapes (their usage strings); 12340 files no category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmTicketWrite {
    /// The text as typed; the EditBox caps it at 500, under the reference's 1999.
    pub text: String,
    /// `NewGMTicket`'s `needResponse`, as Lua truth (`HelpFrame.lua:326`); false for an update.
    pub need_response: bool,
    /// `NewGMTicket` or `UpdateGMTicket`, two opcodes: the window picks by its own `hasTicket`
    /// flag, and the app must not re-derive it.
    pub is_new: bool,
}

impl super::UiScript {
    /// The `GMTicketCategory.dbc` rows in file order, pushed once; the ids are the wire's values,
    /// so renumbering them would misfile every ticket.
    pub fn set_gm_ticket_categories(&mut self, categories: Vec<(u32, String)>) {
        self.model_mut().gm_ticket_categories = categories;
    }

    /// Drain the ticket verbs in call order, one packet each.
    pub fn take_gm_ticket_intents(&mut self) -> Vec<GmTicketIntent> {
        std::mem::take(&mut self.model_mut().gm_ticket_intents)
    }

    /// Drain the `Stuck()` calls, each one cast of spell 7355.
    pub fn take_stuck_casts(&mut self) -> u32 {
        std::mem::take(&mut self.model_mut().stuck_casts)
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // A flat `id, name, ...` vararg list, walked in pairs by `HelpFrameGM_UpdateCategories`;
    // empty until the app pushes the DBC.
    g.set(
        "GetGMTicketCategories",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let mut out = Vec::with_capacity(model.gm_ticket_categories.len() * 2);
            for (id, name) in &model.gm_ticket_categories {
                out.push(Value::Integer(i64::from(*id)));
                out.push(Value::String(lua.create_string(name)?));
            }
            Ok(MultiValue::from_vec(out))
        })?,
    )?;

    // Sends: `GetGMTicket` is answered by `UPDATE_TICKET`, `GetGMStatus` by `UPDATE_GM_STATUS`.
    for (name, intent) in [
        ("GetGMTicket", GmTicketIntent::Ask),
        ("GetGMStatus", GmTicketIntent::AskStatus),
        ("DeleteGMTicket", GmTicketIntent::Delete),
    ] {
        g.set(
            name,
            lua.create_function(move |lua, ()| {
                lua.app_data_mut::<Model>()
                    .expect("model app_data")
                    .gm_ticket_intents
                    .push(intent.clone());
                Ok(())
            })?,
        )?;
    }

    // Submit and Save Changes (`HelpFrame.lua:324-326`): `NewGMTicket(ticketDescription,
    // needResponse)` and `UpdateGMTicket(ticketDescription)`, each raising its usage string on a
    // non-string description.
    for (name, is_new, usage) in [
        (
            "NewGMTicket",
            true,
            "Usage: NewGMTicket(ticketDescription, needResponse)",
        ),
        (
            "UpdateGMTicket",
            false,
            "Usage: UpdateGMTicket(ticketDescription)",
        ),
    ] {
        g.set(
            name,
            lua.create_function(move |lua, (text, need_response): (Value, Value)| {
                let text = super::binding_abi::string_arg(lua, text, usage)?;
                let need_response =
                    is_new && !matches!(need_response, Value::Nil | Value::Boolean(false));
                lua.app_data_mut::<Model>()
                    .expect("model app_data")
                    .gm_ticket_intents
                    .push(GmTicketIntent::Write(GmTicketWrite {
                        text,
                        need_response,
                        is_new,
                    }));
                Ok(())
            })?,
        )?;
    }

    // The Help window's Auto-Unstuck, not a ticket verb: it casts spell 7355, whose
    // `SPELL_EFFECT_STUCK` moves the player to a safe position server-side.
    g.set(
        "Stuck",
        lua.create_function(|lua, ()| {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .stuck_casts += 1;
            Ok(())
        })?,
    )?;

    Ok(())
}
