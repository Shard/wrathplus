//! The stock chat frame scrolls: the wheel over `ChatFrame1` under the 12340 `chatMouseScroll`
//! default, and the button frame's up, down and bottom buttons with the bottom button's flash.

use wrathplus_ui::script::UiScript;

/// A stock session with 40 lines in `ChatFrame1`, so there is history to scroll into.
fn chat() -> Option<UiScript> {
    let data = wrathplus_formats::wow_data_or_skip!(None);
    let (mut s, _failures) = super::super::stock_session(&data);
    s.clear_instruction_budget();
    // The chat cache's restore event, as the app fires it at login (`ui_chat::settings`):
    // ChatFrame1 skips `FloatingChatFrame_OnLoad`, so this is what seats its button frame
    // (`FloatingChatFrame_Update` → `FCF_UpdateButtonSide`).
    s.fire_event("UPDATE_CHAT_WINDOWS", vec![]);
    s.run("for i = 1, 40 do ChatFrame1:AddMessage('line '..i) end")
        .unwrap();
    s.tick(0.05);
    s.resolve();
    Some(s)
}

/// The centre of a frame in UI space.
fn centre(s: &UiScript, frame: &str) -> (f32, f32) {
    s.eval(&format!("local x, y = {frame}:GetCenter() return x, y"))
        .unwrap()
}

/// Lines scrolled up from the bottom, and whether `AtBottom` holds. 12340 has no offset getter
/// for a script to read, so this steps down to the bottom and back up the same count.
fn scroll(s: &UiScript) -> String {
    s.eval(
        "local f, n = ChatFrame1, 0 local bottom = f:AtBottom() \
         while not f:AtBottom() and n < 100 do f:ScrollDown() n = n + 1 end \
         for _ = 1, n do f:ScrollUp() end \
         return n..(bottom and ' bottom' or ' up')",
    )
    .unwrap()
}

/// Getters answer 1 or nil, as 12340's do. `chatMouseScroll` is 1 in 12340, so
/// `VARIABLES_LOADED` gives `ChatFrame1` its wheel script (`ChatFrame.lua:2548`) while the frame
/// stays mouse-disabled (`FloatingChatFrame.xml:505`). A
/// notch up scrolls one line into the history and the handler reports it took the notch, which
/// keeps it from the camera's binding; a notch down returns to the bottom. A line added at the
/// bottom keeps the frame there.
#[test]
fn the_wheel_over_the_chat_frame_scrolls_it() {
    let Some(mut s) = chat() else { return };
    let setup: String = s
        .eval(
            "return GetCVar('chatMouseScroll')..' '..tostring(ChatFrame1:IsMouseWheelEnabled())\
                 ..' '..tostring(ChatFrame1:IsMouseEnabled())",
        )
        .unwrap();
    assert_eq!(setup, "1 1 nil");
    let (x, y) = centre(&s, "ChatFrame1");
    assert!(s.mouse_wheel(x, y, 1.0), "the chat frame takes the notch");
    assert_eq!(scroll(&s), "1 up");
    assert!(s.mouse_wheel(x, y, -1.0));
    assert_eq!(scroll(&s), "0 bottom");
    s.run("ChatFrame1:AddMessage('line 41')").unwrap();
    assert_eq!(scroll(&s), "0 bottom", "a new line keeps the bottom");
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

/// The button frame's up, down and bottom buttons show with the chat frame. A click on up
/// scrolls one line (`OnClick` on the release, `FloatingChatFrame.xml:652`) and holding it
/// repeats every 0.05 s (`MessageFrameScrollButton_OnUpdate`); while scrolled up the bottom
/// button's flash blinks every 0.5 s (`ChatFrame_OnUpdate`), and a bottom click returns and
/// stops it.
#[test]
fn the_chat_scroll_buttons_scroll_and_the_bottom_button_flashes() {
    let Some(mut s) = chat() else { return };
    let shown: String = s
        .eval(
            "local t = {} for _, b in ipairs({'Up', 'Down', 'Bottom'}) do \
                 local f = _G['ChatFrame1ButtonFrame'..b..'Button'] \
                 t[#t + 1] = b..'='..tostring(f and f:IsVisible()) end \
             return table.concat(t, ' ')",
        )
        .unwrap();
    assert_eq!(shown, "Up=1 Down=1 Bottom=1");
    let click = |s: &mut UiScript, frame: &str, held: f32| {
        let (x, y) = centre(s, frame);
        s.mouse_move(x, y);
        s.mouse_button(x, y, "LeftButton", true);
        let mut t = 0.0;
        while t < held {
            s.tick(0.01);
            t += 0.01;
        }
        s.mouse_button(x, y, "LeftButton", false);
    };
    click(&mut s, "ChatFrame1ButtonFrameUpButton", 0.0);
    assert_eq!(scroll(&s), "1 up", "a click scrolls one line");
    click(&mut s, "ChatFrame1ButtonFrameDownButton", 0.0);
    assert_eq!(scroll(&s), "0 bottom");
    click(&mut s, "ChatFrame1ButtonFrameUpButton", 0.3);
    let held = scroll(&s);
    let lines: u32 = held.split(' ').next().unwrap().parse().unwrap();
    assert!(lines > 3, "holding up repeats: {held}");

    let flash = "return ChatFrame1ButtonFrameBottomButtonFlash:IsShown()";
    let mut blinks = 0;
    let mut last = false;
    for _ in 0..12 {
        s.tick(0.1);
        let now: bool = s.eval(flash).unwrap();
        blinks += u32::from(now != last);
        last = now;
    }
    assert!(
        blinks >= 2,
        "the bottom button blinks while scrolled up: {blinks}"
    );
    click(&mut s, "ChatFrame1ButtonFrameBottomButton", 0.0);
    s.tick(0.1);
    assert_eq!(scroll(&s), "0 bottom");
    assert!(
        !s.eval::<bool>(flash).unwrap(),
        "the flash stops at the bottom"
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}
