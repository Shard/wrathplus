//! The four ordinary Wrath script methods: `HasScript`, `GetScript`, `SetScript`, `HookScript`.

use crate::script::{ScriptDialect, UiScript};

fn wrath() -> UiScript {
    UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap()
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
fn vanilla_keeps_flat_script_acceptance_and_has_no_hookscript() {
    let script = UiScript::new().unwrap();
    assert!(script
        .eval::<bool>(
            r#"
            local frame = CreateFrame("Frame")
            local handler = function() end
            if frame.HookScript ~= nil then return false end
            if frame:HasScript("OnClick") ~= 1 or frame:HasScript("onvaluechanged") ~= 1 then
                return false
            end
            if frame:GetScript("OnClick") ~= nil then return false end
            frame:SetScript("onclick", handler)
            return frame:GetScript("ONCLICK") == handler
                and frame:HasScript("NotARealScript") == nil
                and frame:GetScript("NotARealScript") == nil
            "#,
        )
        .unwrap());
}
