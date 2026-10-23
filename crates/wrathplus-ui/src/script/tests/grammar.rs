//! The text setter's grammar pass on the sinks that run it: the FontString setter and the message
//! frames' lines store `|4singular:plural;` resolved ([`crate::grammar`]).

use super::common::script;

#[test]
fn a_fontstring_stores_the_plural_its_number_picks() {
    let s = script();
    let got = s
        .eval::<(String, String, String)>(
            r#"
            local f = CreateFrame("Frame")
            local fs = f:CreateFontString(nil, "ARTWORK")
            local LEVEL_UP_CHAR_POINTS = "You have gained %d talent |4point:points;."
            fs:SetText(format(LEVEL_UP_CHAR_POINTS, 1))
            local one = fs:GetText()
            fs:SetFormattedText(LEVEL_UP_CHAR_POINTS, 3)
            local three = fs:GetText()
            fs:SetText("||4kept:literal;")
            return one, three, fs:GetText()
        "#,
        )
        .unwrap();
    assert_eq!(
        got,
        (
            "You have gained 1 talent point.".to_string(),
            "You have gained 3 talent points.".to_string(),
            "||4kept:literal;".to_string()
        )
    );
}

#[test]
fn a_chat_line_stores_the_plural_its_number_picks() {
    let s = script();
    let got = s
        .eval::<String>(
            r#"
            local f = CreateFrame("ScrollingMessageFrame")
            f:AddMessage("You have gained 2 talent |4point:points;.")
            return (f:GetMessageInfo(1))
        "#,
        )
        .unwrap();
    assert_eq!(got, "You have gained 2 talent points.");
}

#[test]
fn a_button_label_resolves_and_takes_the_12340_nil_rule() {
    let s = script();
    let got = s
        .eval::<(bool, String, bool)>(
            r#"
            local b = CreateFrame("Button")
            -- No label yet: "" and nil create none (`0x96f860`).
            b:SetText("")
            b:SetText(nil)
            local none = b:GetFontString() == nil
            -- MailFrame.lua:195's expiry, through the setter's grammar pass.
            b:SetText("|cff20ff20" .. format("%d |4Day:Days;", 29) .. " |r")
            local text = b:GetText()
            -- With a label, nil clears it.
            b:SetText(nil)
            return none, text, b:GetText() == nil
        "#,
        )
        .unwrap();
    assert_eq!(got, (true, "|cff20ff2029 Days |r".to_string(), true));
}

#[test]
fn a_tooltip_line_resolves_its_plural() {
    let s = script();
    let got = s
        .eval::<String>(
            r#"
            local tt = CreateFrame("GameTooltip", "PluralTip")
            tt:SetOwner(UIParent or CreateFrame("Frame"), "ANCHOR_NONE")
            tt:AddLine(format("Duration: %d |4day:days;", 1))
            return PluralTipTextLeft1:GetText()
        "#,
        )
        .unwrap();
    assert_eq!(got, "Duration: 1 day");
}
