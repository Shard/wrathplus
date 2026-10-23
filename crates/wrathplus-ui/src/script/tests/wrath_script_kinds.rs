//! The four ordinary Wrath script methods: `HasScript`, `GetScript`, `SetScript`, `HookScript`.

use crate::script::UiScript;

fn wrath() -> UiScript {
    UiScript::new().unwrap()
}

#[test]
fn hook_script_runs_after_the_original_for_every_widget_kind() {
    let script = wrath();
    assert!(script
        .eval::<bool>(
            r#"
            local function hooks(kind, script_kind)
                local frame = CreateFrame(kind)
                if frame:HasScript(string.lower(script_kind)) ~= 1 then return false end
                local trace = {}
                frame:SetScript(string.lower(script_kind), function() trace[#trace + 1] = "set" end)
                frame:HookScript(string.upper(script_kind), function() trace[#trace + 1] = "hook" end)
                frame:GetScript(script_kind)(frame)
                return #trace == 2 and trace[1] == "set" and trace[2] == "hook"
            end
            local function hooks_empty(kind, script_kind)
                local frame = CreateFrame(kind)
                local ran = false
                frame:HookScript(script_kind, function() ran = true end)
                frame:GetScript(script_kind)(frame)
                return ran
            end
            return hooks("Frame", "OnEvent") and hooks("Button", "OnClick")
                and hooks("EditBox", "OnEditFocusGained") and hooks("StatusBar", "OnValueChanged")
                and hooks("Frame", "OnAttributeChanged") and hooks_empty("Frame", "OnShow")
                and not pcall(CreateFrame("Frame").HookScript, CreateFrame("Frame"), "OnNothing", print)
            "#,
        )
        .unwrap());
}

#[test]
fn get_attribute_resolves_the_secure_templates_modified_lookup() {
    let script = wrath();
    assert!(script
        .eval::<bool>(
            r#"
            local b = CreateFrame("Button")
            b:SetAttribute("type", "action")
            b:SetAttribute("*unit2", "focus")
            b:SetAttribute("shift-type1", "macro")
            return b:GetAttribute("", "type", "1") == "action"
                and b:GetAttribute("shift-", "type", "1") == "macro"
                and b:GetAttribute("", "unit", "2") == "focus"
                and b:GetAttribute("ctrl-", "unit", "1") == nil
                and b:GetAttribute("type") == "action"
            "#,
        )
        .unwrap());
}

/// `RegisterForClicks("AnyUp")` (`ActionButton.lua:90`): every button's release clicks, no press.
#[test]
fn any_up_registers_every_buttons_release() {
    let mut s = wrath();
    s.run(
        r#"
        clicks = {}
        local b = CreateFrame("Button", "AnyUpButton", UIParent)
        b:SetPoint("BOTTOMLEFT", 0, 0); b:SetWidth(200); b:SetHeight(200); b:EnableMouse(true)
        b:RegisterForClicks("AnyUp")
        b:SetScript("OnClick", function(self, button, down) clicks[#clicks + 1] = button end)
        "#,
    )
    .unwrap();
    s.resolve();
    s.mouse_button(50.0, 50.0, "LeftButton", true);
    assert_eq!(s.eval::<i64>("return #clicks").unwrap(), 0, "a press is not a click");
    s.mouse_button(50.0, 50.0, "LeftButton", false);
    s.mouse_button(50.0, 50.0, "RightButton", true);
    s.mouse_button(50.0, 50.0, "RightButton", false);
    assert_eq!(
        s.eval::<String>("return table.concat(clicks, ',')").unwrap(),
        "LeftButton,RightButton"
    );
}

/// 3.3.5's quest log moves its shown detail pane from a hidden window under the open log.
#[test]
fn a_shown_frame_moved_under_a_visible_parent_becomes_visible() {
    let script = wrath();
    assert!(script
        .eval::<bool>(
            r#"
            local hidden = CreateFrame("Frame", nil, UIParent) hidden:Hide()
            local open = CreateFrame("Frame", nil, UIParent)
            local pane = CreateFrame("Frame", nil, hidden)
            local before = pane:IsVisible()
            pane:SetParent(open)
            return not before and pane:IsVisible() == 1
            "#,
        )
        .unwrap());
}
