//! The stock `UIMenu` kit as the chat menu uses it: rows 16 high, as wide as `UIMenu_AutoSize`
//! makes them, the label the button's own text, the shortcut a sibling anchored RIGHT. `SetText` anchors a new
//! label by the normal font's `justifyH` (`CSimpleButton::SetFontString`, `0x778d20`).

use wrathplus_ui::script::UiScript;

use super::test_ui::load_ui as load_xml;

/// The chat stack the menu lives in, with a fixed-advance font so the rows have widths.
fn chat_menu() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // Before the load: `ChatMenu_OnLoad` measures its rows (`UIMenu_AutoSize`).
    s.set_text_measurer(Box::new(super::FixedWidthFont(8.0)));
    for f in [
        r"Interface\FrameXML\GlobalStrings.lua",
        r"Interface\FrameXML\Fonts.xml",
        r"Interface\FrameXML\BasicControls.xml",
        r"Interface\FrameXML\UIParent.xml",
        r"Interface\FrameXML\MoneyFrame.lua",
        r"Interface\FrameXML\MoneyFrame.xml",
        r"Interface\FrameXML\GameTooltip.xml",
        r"Interface\FrameXML\UIMenu.xml",
        r"Interface\FrameXML\UIDropDownMenu.xml",
        r"Interface\FrameXML\UIPanelTemplates.lua",
        r"Interface\FrameXML\UIPanelTemplates.xml",
        r"Interface\FrameXML\StaticPopup.xml",
        r"Interface\FrameXML\ChatFrame.xml",
        r"Interface\FrameXML\FloatingChatFrame.xml",
    ] {
        load_xml(&mut s, f);
    }
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    // As the session does at login (`ui_chat::settings`): `FloatingChatFrame_Update` sides the
    // button frame the menu button, and so the menu, hang from (`FloatingChatFrame.lua:167`).
    s.fire_event("UPDATE_CHAT_WINDOWS", vec![]);
    let _ = s.errors();
    s
}

fn edges(s: &mut UiScript, name: &str) -> (f32, f32) {
    s.eval::<(f32, f32)>(&format!("return {name}:GetLeft(), {name}:GetRight()"))
        .unwrap_or_else(|e| panic!("{name}'s rect: {e}"))
}

/// Label at the row's left edge, shortcut at its right, so "Macro" and "/macro" never meet.
#[test]
fn chat_menu_rows_left_align_their_label_and_right_align_their_shortcut() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = chat_menu();
    s.run("ChatMenu:Show()").unwrap();
    assert!(s.errors().is_empty(), "opening it raises: {:?}", s.errors());
    s.resolve();

    // Macro is the twelfth row 3.3.5's `ChatMenu_OnLoad` adds (`ChatFrame.lua:4250`).
    assert_eq!(
        s.eval::<String>("return ChatMenuButton12:GetText()")
            .unwrap(),
        "Macro"
    );
    assert_eq!(
        s.eval::<String>("return ChatMenuButton12ShortcutText:GetText()")
            .unwrap(),
        "/macro"
    );
    // `UIMenu_AutoSize` widens every row to the widest label plus shortcut, plus 20 of padding
    // (`UIMenu.lua:143-171`), as `ChatMenu_OnLoad` ends (`ChatFrame.lua:4251`).
    let widest = s
        .eval::<f32>(
            "local w = 0 for i = 1, ChatMenu.numButtons do \
               local b = _G['ChatMenuButton'..i] \
               local t = _G['ChatMenuButton'..i..'ShortcutText']:GetText() or '' \
               w = math.max(w, 8 * (#b:GetText() + #t)) end return w",
        )
        .unwrap();
    let (row_l, row_r) = edges(&mut s, "ChatMenuButton12");
    assert_eq!(row_r - row_l, widest + 20.0, "the auto-sized row");
    assert_eq!(
        s.eval::<f32>("return ChatMenu:GetWidth()").unwrap(),
        widest + 20.0 + 24.0,
        "and the menu, its two 12-wide borders around it"
    );

    // The label's implicit anchor is LEFT to LEFT (0, 0), from `<NormalFont justifyH="LEFT"/>`.
    let (p, rp, x, y): (String, String, f32, f32) = s
        .eval(
            "local p, _, rp, x, y = ChatMenuButton12:GetFontString():GetPoint(1) \
             return p, rp, x, y",
        )
        .unwrap();
    assert_eq!(
        (p.as_str(), rp.as_str(), x, y),
        ("LEFT", "LEFT", 0.0, 0.0),
        "the label is anchored by the normal font's justify"
    );
    let (label_l, label_r) = edges(&mut s, "ChatMenuButton12:GetFontString()");
    assert_eq!(label_l, row_l, "the label starts at the row's left edge");
    // The template anchors the shortcut string RIGHT.
    let (short_l, short_r) = edges(&mut s, "ChatMenuButton12ShortcutText");
    assert_eq!(short_r, row_r, "the shortcut ends at the row's right edge");
    // With 8 px glyphs "Macro" spans 40 and "/macro" 48: left and right, they never meet.
    assert!(
        label_r <= short_l,
        "label {label_l}..{label_r} collides with shortcut {short_l}..{short_r}"
    );

    for i in 1..=s.eval::<i64>("return ChatMenu.numButtons").unwrap() {
        let row = format!("ChatMenuButton{i}");
        let (rl, _) = edges(&mut s, &row);
        let (ll, _) = edges(&mut s, &format!("{row}:GetFontString()"));
        assert_eq!(ll, rl, "row {i}'s label starts at its left edge");
    }
}

/// A lazily created label answers the normal font's justify and object (`0x779810`).
#[test]
fn a_menu_rows_label_reports_the_normal_fonts_justify_and_object() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let s = chat_menu();
    s.run("ChatMenu:Show()").unwrap();
    assert_eq!(
        s.eval::<String>("return ChatMenuButton1:GetFontString():GetJustifyH()")
            .unwrap(),
        "LEFT"
    );
    assert!(
        s.eval::<bool>(
            "return ChatMenuButton1:GetFontString():GetFontObject() == GameFontNormalLeft"
        )
        .unwrap(),
        "the label inherits the button's normal font object (`UIMenu.xml:31`)"
    );
}
