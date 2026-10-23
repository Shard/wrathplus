//! Build 12340's two Help-frame globals 1.12 lacks: `GMReportLag` (`0x5ad020`) and
//! `GMResponseNeedMoreHelp` (`0x5ad150`). Both queue a [`GmTicketIntent`] in call order with the
//! ticket verbs; the app fills in the map and position. Untraced: the `0x5191c0(0x11)` UI-state
//! gate in front of `GMResponseNeedMoreHelp`'s send, which this does not apply.

use mlua::{Lua, Value};

use super::{GmTicketIntent, Model};

fn usage(text: &str) -> mlua::Error {
    mlua::Error::runtime(format!("Usage: {text}"))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // A number (rounded, as `0x5ad020` does), else the usage error.
    g.set(
        "GMReportLag",
        lua.create_function(|lua, kind: Value| {
            let lag_type = match kind {
                Value::Integer(n) => n,
                Value::Number(n) => n.round() as i64,
                _ => return Err(usage("GMReportLag(number)")),
            };
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .gm_ticket_intents
                .push(GmTicketIntent::ReportLag {
                    lag_type: lag_type as u32,
                });
            Ok(())
        })?,
    )?;
    g.set(
        "GMResponseNeedMoreHelp",
        lua.create_function(|lua, text: Value| {
            let Value::String(text) = text else {
                return Err(usage("GMResponseNeedMoreHelp(newTicketDescription)"));
            };
            let text = text.to_str()?.to_string();
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .gm_ticket_intents
                .push(GmTicketIntent::NeedMoreHelp { text });
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{GmTicketIntent, UiScript};

    #[test]
    fn help_globals_queue_intents_in_call_order_and_check_arguments() {
        let mut ui = UiScript::new().unwrap();
        ui.run("GMReportLag(2.6); GMResponseNeedMoreHelp('still stuck')")
            .unwrap();
        assert_eq!(
            ui.take_gm_ticket_intents(),
            vec![
                GmTicketIntent::ReportLag { lag_type: 3 },
                GmTicketIntent::NeedMoreHelp {
                    text: "still stuck".into()
                },
            ]
        );
        assert!(ui.run("GMReportLag('x')").is_err());
        assert!(ui.run("GMResponseNeedMoreHelp(5)").is_err());
        assert!(ui.take_gm_ticket_intents().is_empty());
    }
}
