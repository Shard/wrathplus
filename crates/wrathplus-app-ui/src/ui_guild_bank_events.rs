//! The guild bank's FrameScript events: the frame's, off a change of view, and
//! `GUILDBANK_TEXT_CHANGED`, off `SMSG_GUILD_EVENT`.
use super::GuildBank;
use bevy::prelude::*;
use wrathplus_protocol::SessionEvent;
use wrathplus_ui::script::{ScriptValue, UiScript, WrathGuildBankView};

/// `SMSG_GUILD_EVENT` case `0x13` (`0x6d92d0`): another member edited a tab's text; the frame
/// re-queries it when that tab is showing (`Blizzard_GuildBankUI.lua:151-155`). Fired whether or
/// not the vault is open, as the reference does. Labelled: the cached text stays until the new
/// one lands; the reference's `0x5a4310(tab, 1)` beside the fire is untraced.
pub(super) fn text_changed(In(event): In<SessionEvent>, mut bank: ResMut<GuildBank>) {
    use wrathplus_protocol::types::guild_event::BANK_TEXT_CHANGED;
    if let SessionEvent::GuildEvent(notice) = event {
        // `atol` of the first string, as the reference reads it.
        let tab = notice
            .params
            .first()
            .and_then(|p| p.trim().parse::<u8>().ok());
        if let (BANK_TEXT_CHANGED, Some(tab)) = (notice.event, tab) {
            bank.text_changed.push(tab);
        }
    }
}

/// Fire the queued `GUILDBANK_TEXT_CHANGED(tab)`, 1-based (`"%d"`, `0x9e289c`).
pub(super) fn fire_text_changed(bank: &mut GuildBank, script: &mut UiScript) {
    for tab in std::mem::take(&mut bank.text_changed) {
        script.queue_event(
            "GUILDBANK_TEXT_CHANGED",
            vec![ScriptValue::Int(i64::from(tab) + 1)],
        );
    }
}

/// The frame's events for a change of view, in the order the reference's reply handler raises
/// them; a closed bank is the default view.
pub(super) fn events(
    before: Option<&WrathGuildBankView>,
    after: &WrathGuildBankView,
) -> Vec<(&'static str, Vec<ScriptValue>)> {
    let closed = WrathGuildBankView::default();
    let was_open = before.is_some_and(|b| *b != closed);
    let is_open = *after != closed;
    let before = before.unwrap_or(&closed);
    let mut out = Vec::new();
    match (was_open, is_open) {
        (false, true) => out.push(("GUILDBANKFRAME_OPENED", Vec::new())),
        (true, false) => {
            out.push(("GUILDBANKFRAME_CLOSED", Vec::new()));
            return out;
        }
        (false, false) => return out,
        (true, true) => {}
    }
    if before.tabs != after.tabs {
        out.push(("GUILDBANK_UPDATE_TABS", Vec::new()));
    }
    if before.money != after.money {
        out.push(("GUILDBANK_UPDATE_MONEY", Vec::new()));
    }
    if before.withdraw_money_remaining != after.withdraw_money_remaining {
        out.push(("GUILDBANK_UPDATE_WITHDRAWMONEY", Vec::new()));
    }
    if before.slots != after.slots {
        out.push(("GUILDBANKBAGSLOTS_CHANGED", Vec::new()));
    }
    for (i, text) in after.tab_text.iter().enumerate() {
        if before.tab_text.get(i) != Some(text) {
            out.push((
                "GUILDBANK_UPDATE_TEXT",
                vec![ScriptValue::Int(i as i64 + 1)],
            ));
        }
    }
    if before.money_log != after.money_log || before.tab_logs != after.tab_logs {
        out.push(("GUILDBANKLOG_UPDATE", Vec::new()));
    }
    out
}
