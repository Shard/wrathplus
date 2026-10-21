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
