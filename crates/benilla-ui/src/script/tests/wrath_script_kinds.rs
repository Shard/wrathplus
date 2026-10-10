//! Bounded build-12340 receiver-kind policy for the four ordinary script methods.
//!
//! The cases are synthetic host fixtures. In particular, the base-only SimpleHTML and Minimap
//! assertions describe the deliberately bounded implementation policy; they do not claim that
//! unaudited native resolvers lack additional names.

use crate::script::{ScriptDialect, UiScript};

fn wrath() -> UiScript {
    UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap()
}

#[test]
fn wrath_script_methods_share_the_bounded_receiver_kind_matrix() {
    let script = wrath();
    assert!(script
        .eval::<bool>(
            r#"
            local function accepts(kind, script_kind)
                local frame = CreateFrame(kind)
                if frame:HasScript(string.lower(script_kind)) ~= 1 then return false end
                if frame:GetScript(string.upper(script_kind)) ~= nil then return false end

                local trace = {}
                local base = function() trace[#trace + 1] = "set" end
                frame:SetScript(string.lower(script_kind), base)
                if frame:GetScript(string.upper(script_kind)) ~= base then return false end
                frame:HookScript(string.upper(script_kind), function()
                    trace[#trace + 1] = "hook"
                end)

                local installed = frame:GetScript(script_kind)
                if installed ~= frame:GetScript(string.lower(script_kind)) then return false end
                if #trace ~= 0 then return false end
                installed(frame)
                return #trace == 2 and trace[1] == "set" and trace[2] == "hook"
            end

            local function rejects(kind, script_kind)
                local frame = CreateFrame(kind)
                local trace = {}
                frame:SetScript("OnEvent", function() trace[#trace + 1] = "base" end)
                frame:HookScript("OnEvent", function() trace[#trace + 1] = "hook" end)
                local before = frame:GetScript("OnEvent")

                if frame:HasScript(script_kind) ~= nil then return false end
                if pcall(frame.GetScript, frame, script_kind) then return false end
                if pcall(frame.SetScript, frame, script_kind, function() end) then return false end
                if pcall(frame.HookScript, frame, script_kind, function() end) then return false end
                if frame:GetScript("OnEvent") ~= before then return false end
                before(frame)
                return #trace == 2 and trace[1] == "base" and trace[2] == "hook"
            end

            local accepted = {
                {"Frame", "OnEvent"},
                {"Button", "OnClick"},
                {"CheckButton", "OnClick"},
                {"LootButton", "OnClick"},
                {"EditBox", "OnEditFocusGained"},
                {"EditBox", "OnEditFocusLost"},
                {"StatusBar", "OnValueChanged"},
                {"Slider", "OnValueChanged"},
                {"ScrollFrame", "OnHorizontalScroll"},
                {"ScrollFrame", "OnVerticalScroll"},
                {"ScrollFrame", "OnScrollRangeChanged"},
                {"Model", "OnUpdateModel"},
                {"Model", "OnAnimFinished"},
                {"PlayerModel", "OnUpdateModel"},
                {"PlayerModel", "OnAnimFinished"},
                {"DressUpModel", "OnUpdateModel"},
                {"DressUpModel", "OnAnimFinished"},
                {"TabardModel", "OnUpdateModel"},
                {"TabardModel", "OnAnimFinished"},
                {"GameTooltip", "OnTooltipAddMoney"},
                {"GameTooltip", "OnTooltipCleared"},
                {"MessageFrame", "OnHyperlinkClick"},
                {"ScrollingMessageFrame", "OnHyperlinkClick"},
                {"ColorSelect", "OnColorSelect"},
                {"SimpleHTML", "OnEvent"},
                {"Minimap", "OnEvent"},
            }
            for _, case in ipairs(accepted) do
                if not accepts(case[1], case[2]) then return false end
            end

            local rejected = {
                {"Frame", "OnClick"},
                {"EditBox", "OnClick"},
                {"ScrollFrame", "OnValueChanged"},
                {"Slider", "OnHorizontalScroll"},
                {"Frame", "OnUpdateModel"},
                {"Frame", "OnTooltipAddMoney"},
                {"Frame", "OnTooltipCleared"},
                {"Frame", "OnHyperlinkClick"},
                {"Frame", "OnColorSelect"},
                -- Bounded policy for resolver families not closed by the audit, not a native claim.
                {"SimpleHTML", "OnValueChanged"},
                {"Minimap", "OnValueChanged"},
            }
            for _, case in ipairs(rejected) do
                if not rejects(case[1], case[2]) then return false end
            end
            return true
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

#[test]
fn wrath_matrix_does_not_enable_secure_execution_or_protected_templates() {
    let script = wrath();
    assert!(!script.supports_secure_execution());
    assert!(script
        .eval::<bool>(
            r#"
            local accepted = pcall(function()
                CreateFrame("Button", nil, nil, "SecureActionButtonTemplate")
            end)
            return not accepted
                and issecure == nil
                and securecall == nil
                and hooksecurefunc == nil
                and InCombatLockdown == nil
                and IsProtected == nil
            "#,
        )
        .unwrap());
}
