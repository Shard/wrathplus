//! The flush's hold: with no inline measurer, a focused box's `OnTextChanged` waits for the caret
//! leg, and an unfocused box's leg places at a fallback, so `OnCursorChanged` precedes it as in
//! the reference (`0x77d475` before `0x77d498`).

use crate::script::UiScript;

/// A focused, logging box `E` on a VM with no measurer, its first-show fires drained.
fn focused_box() -> UiScript {
    let mut s = UiScript::new().expect("construct UiScript");
    s.set_screen_size(800.0, 600.0);
    s.run(
        r#"
        LOG = {}
        E = CreateFrame("EditBox", "E")
        E:SetPoint("BOTTOMLEFT", 100, 50)
        E:SetWidth(200); E:SetHeight(32)
        E:SetScript("OnCursorChanged", function(this, x) table.insert(LOG, "cursor@" .. x) end)
        E:SetScript("OnTextChanged", function(this) table.insert(LOG, "text:" .. this:GetText()) end)
        E:SetFocus()
    "#,
    )
    .unwrap();
    s.resolve();
    s.tick(0.016);
    take_log(&s);
    s
}

fn take_log(s: &UiScript) -> String {
    s.eval::<String>("local l = table.concat(LOG, ' ') LOG = {} return l")
        .unwrap()
}

#[test]
fn a_pending_table_holds_on_text_changed_until_the_caret_places() {
    let mut s = focused_box();
    s.run(r#"E:SetText("ab")"#).unwrap();
    s.resolve();
    s.tick(0.016);
    assert_eq!(take_log(&s), "", "held while the table is pending");
    s.tick(0.016);
    assert_eq!(take_log(&s), "", "still held: no table yet");

    let req = s.editbox_advances_request().expect("the focused box asks");
    let cum = (0..=req.text.len()).map(|i| i as f32 * 7.0).collect();
    s.set_editbox_advances(req.id, req.key, cum, vec![0], 0.0);
    s.tick(0.016);
    assert_eq!(take_log(&s), "cursor@14 text:ab");
    s.tick(0.016);
    assert_eq!(take_log(&s), "", "fired once");
}

/// An unfocused box never gets the host's table, so its caret leg places at the fallback x 0,
/// and `OnCursorChanged` still precedes `OnTextChanged`, which stock `ScrollingEdit` needs.
#[test]
fn an_unfocused_box_with_no_table_places_its_caret_before_on_text_changed() {
    let mut s = focused_box();
    s.run(r#"E:ClearFocus() LOG = {} E:SetText("ab")"#).unwrap();
    s.resolve();
    s.tick(0.016);
    assert_eq!(take_log(&s), "cursor@0 text:ab");
    s.tick(0.016);
    assert_eq!(take_log(&s), "", "fired once");
}
