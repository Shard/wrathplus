//! One installed check per line family [`super::frames::route`] fires, against the text the stock
//! 3.3.5 `ChatFrame_MessageEventHandler` hands `ChatFrame1:AddMessage`: TEXT_EMOTE, PARTY, SYSTEM,
//! and a channel's join and leave notices. The handler concatenates arg11 into every player
//! link, so a line fired with the 1.12 ten arguments raised and printed nothing.

use super::tests::chat_vm;
use super::{ChatEvent, ChatEventKind as K};
use benilla_ui::script::UiScript;

/// Every text `ChatFrame1:AddMessage` takes, in order, in the global `Printed`, and its colour in
/// `PrintedColor`.
pub(super) const PRINTED: &str = r#"
    Printed, PrintedColor = {}, {}
    local add = ChatFrame1.AddMessage
    ChatFrame1.AddMessage = function(self, text, r, g, b, ...)
        table.insert(Printed, text)
        table.insert(PrintedColor, { r, g, b })
        return add(self, text, r, g, b, ...)
    end
"#;

/// The stock chat stack with the [`PRINTED`] hook on its window.
fn printing_vm() -> UiScript {
    let s = chat_vm();
    s.run(PRINTED).unwrap();
    s
}

pub(super) fn printed(s: &UiScript) -> Vec<String> {
    s.eval::<Vec<String>>("return Printed").unwrap()
}

fn line(kind: K, text: &str, sender: &str) -> ChatEvent {
    ChatEvent {
        kind: Some(kind),
        text: text.into(),
        sender: sender.into(),
        ..Default::default()
    }
}

/// Fire `event` through [`super::frames::route`]; what the window printed, and no handler error.
fn route(event: &ChatEvent) -> Vec<String> {
    let mut s = printing_vm();
    let mut windows = super::frames::ChatWindows::default();
    super::frames::route(&mut s, &mut windows, event);
    assert!(s.errors().is_empty(), "handler errors: {:?}", s.errors());
    printed(&s)
}

/// `body = gsub(arg1, arg2, pflag..playerLink..coloredName.."|h", 1)`: the sender's name in the
/// sentence becomes the link, whose payload is `arg2:arg11:chatGroup`.
#[test]
fn a_text_emote_links_the_performer_in_its_sentence() {
    let _data = benilla_formats::wow_data_or_skip!();
    assert_eq!(
        route(&line(K::TextEmote, "Bob waves at you.", "Bob")),
        ["|Hplayer:Bob:0:TEXT_EMOTE|hBob|h waves at you."]
    );
}

/// `format(CHAT_PARTY_GET..arg1, pflag..playerLink.."["..coloredName.."]".."|h")`.
#[test]
fn a_party_line_wears_the_party_link_and_the_linked_name() {
    let _data = benilla_formats::wow_data_or_skip!();
    assert_eq!(
        route(&line(K::Party, "inc 3", "Ann")),
        ["|Hchannel:PARTY|h[Party]|h |Hplayer:Ann:0:PARTY|h[Ann]|h: inc 3"]
    );
}

/// SYSTEM prints arg1 as it is.
#[test]
fn a_system_line_prints_verbatim() {
    let _data = benilla_formats::wow_data_or_skip!();
    assert_eq!(
        route(&ChatEvent::text_only(
            K::System,
            "You are now Away: AFK".into()
        )),
        ["You are now Away: AFK"]
    );
}

/// `format(CHAT_<token>_NOTICE, arg8, arg4)`, once the window carries the channel; the leave
/// drops the window's record after the line.
#[test]
fn a_channels_join_and_leave_notices_print_their_numbered_link() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = printing_vm();
    let mut windows = super::frames::ChatWindows::default();
    let mut channels = super::edit::ChannelState::default();
    channels.claim_slot("General - Elwynn Forest");
    s.run("ChatFrame_AddChannel(ChatFrame1, 'General - Elwynn Forest')")
        .unwrap();

    for byte in ["2", "3"] {
        // YOU_JOINED, then YOU_LEFT.
        let mut e = ChatEvent::text_only(K::ChannelNotice, String::new());
        e.channel = "General - Elwynn Forest".into();
        e.notice = byte.into();
        super::feed::deliver(&mut s, &mut windows, &mut channels, &mut e);
    }
    assert!(s.errors().is_empty(), "handler errors: {:?}", s.errors());
    assert_eq!(
        printed(&s),
        [
            "Joined Channel: |Hchannel:CHANNEL:1|h[1. General - Elwynn Forest]|h",
            "Left Channel: |Hchannel:CHANNEL:1|h[1. General - Elwynn Forest]|h",
        ]
    );
}
