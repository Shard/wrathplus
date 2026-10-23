//! Build 12340's GM response, the half of the Help Request that answers a ticket, from the help
//! dispatcher `FUN_005ad240`. Its events reach the stock `HelpFrame` and `TicketStatusFrame`
//! (`HelpFrame.lua:175`, `:382`):
//! - `SMSG_GMRESPONSE_RECEIVED` (`0x4ef`) stores the response and ticket ids, reads the ticket text
//!   into a 2,000-byte buffer and the four response parts, 4,000 bytes each, into one 16,000-byte
//!   buffer, then fires `GMRESPONSE_RECEIVED` (event `0x28f`) with `"%s%s"`: text, response.
//! - `SMSG_GMRESPONSE_STATUS_UPDATE` (`0x4f1`) clears both ids, then a nonzero byte fires
//!   `GMSURVEY_DISPLAY` (`0x1dd`) and a zero one `UPDATE_TICKET` (`0x181`) with no arguments.
//!
//! The survey offer goes through [`crate::ui_gm_ticket`]'s own survey path, which publishes the
//! current survey before it fires. Labelled: `SMSG_COMPLAIN_RESULT` (`0x3c8`) has no traced UI
//! effect here, so it is logged only; the stored ids feed no reader in this client.

use bevy::prelude::*;
use wrathplus_protocol::{wrath_help::Packet, SessionEvent, SessionEventKind};
use wrathplus_ui::script::{ScriptValue, UiScript};

use crate::app_core::schedule::UiFeed;
use crate::net::NetHandlerApp;

/// The ticket text's buffer (`local_7e8`, 2,000 bytes with the NUL).
const TICKET_TEXT_BYTES: usize = 1_999;
/// One response part's read buffer (`push 0xfa0`).
const PART_BYTES: usize = 3_999;
/// The joined response's buffer (`local_5608`, 16,000 bytes with the NUL).
const RESPONSE_BYTES: usize = 15_999;

/// The events owed to the in-game VM, in arrival order.
#[derive(Resource, Default)]
pub struct GmResponseEvents(pub Vec<(&'static str, Vec<ScriptValue>)>);

/// Register the sole `WrathHelp` owner and its feed.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<GmResponseEvents>()
        .net_handler(SessionEventKind::WrathHelp, on_help)
        .net_handler(SessionEventKind::Disconnected, on_session_end)
        .add_systems(
            Update,
            feed.in_set(UiFeed)
                .run_if(crate::app_core::ui_state::ingame_ui_up),
        );
}

/// Truncate to `max` bytes at a char boundary, as the fixed buffers do.
fn clamp(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn on_help(
    In(event): In<SessionEvent>,
    mut events: ResMut<GmResponseEvents>,
    mut ticket: ResMut<crate::ui_gm_ticket::GmTicketState>,
) {
    let SessionEvent::WrathHelp { packet } = event else {
        return;
    };
    match packet {
        Packet::GmResponse {
            ticket_text,
            response,
            ..
        } => {
            let joined: String = response.iter().map(|p| clamp(p, PART_BYTES)).collect();
            events.0.push((
                "GMRESPONSE_RECEIVED",
                vec![
                    ScriptValue::Str(clamp(&ticket_text, TICKET_TEXT_BYTES).to_string()),
                    ScriptValue::Str(clamp(&joined, RESPONSE_BYTES).to_string()),
                ],
            ));
        }
        Packet::GmResponseStatus { show_survey: 0 } => events.0.push(("UPDATE_TICKET", vec![])),
        Packet::GmResponseStatus { .. } => crate::ui_gm_ticket::net::status_update(3, &mut ticket),
        Packet::ComplainResult { raw } => debug!("gm response: complaint received ({raw})"),
    }
}

fn on_session_end(In(_): In<SessionEvent>, mut events: ResMut<GmResponseEvents>) {
    events.0.clear();
}

fn feed(script: Option<NonSendMut<UiScript>>, mut events: ResMut<GmResponseEvents>) {
    let Some(mut script) = script else {
        return;
    };
    for (name, args) in std::mem::take(&mut events.0) {
        script.fire_event(name, args);
    }
}
