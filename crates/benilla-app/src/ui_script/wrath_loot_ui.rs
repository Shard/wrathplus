//! Audited stock build-12340 LootFrame closure for manual solo corpse loot.

use benilla_ui::script::UiScript;
use bevy::prelude::*;

use super::reference_ui;

/// Stock files read from the player's patch chain. The adapter removes only group/master-loot and
/// tooltip/modified-click callbacks whose dependencies are outside the admitted solo-loot slice.
const LOOT_FILES: &[&str] = &[
    r"Interface\FrameXML\ItemButtonTemplate.xml",
    r"Interface\FrameXML\LootFrame.xml",
];

/// Load the actual installed stock files after the existing GlobalStrings/Fonts/FontStyles/
/// UIPanelTemplates bootstrap. No stock bytes are stored in this repository.
pub(super) fn load(script: &UiScript) -> Vec<String> {
    let files = LOOT_FILES
        .iter()
        .map(|path| (*path).to_string())
        .collect::<Vec<_>>();
    let mut failures = reference_ui::addon(vec![files[0].clone()]).load_files(script, &files[..1]);
    let path = &files[1];
    let loaded = (|| -> Result<(), String> {
        let bytes = reference_ui::read(path).ok_or_else(|| format!("{path}: not found"))?;
        let mut document = benilla_ui::framexml::parse(&benilla_ui::source::decode(&bytes))
            .map_err(|error| error.to_string())?;
        // The installed file also instantiates a master-loot dropdown whose OnLoad enters the
        // unsupported dropdown service. Exclude that one top-level object before materializing;
        // keep the actual solo frame, templates, included Lua and layout byte-derived.
        document.items.retain(|item| {
            !matches!(item,
                benilla_ui::framexml::TopLevel::Instance(element)
                    if element.name() == Some("GroupLootDropDown")
            )
        });
        let provider = |request: &str| {
            let resolved = if request.contains('\\') || request.contains('/') {
                request.to_string()
            } else {
                format!(r"Interface\FrameXML\{request}")
            };
            reference_ui::read(&resolved)
        };
        let report = benilla_ui::loader::load_in(script, &document, path, &provider);
        if !report.errors.is_empty()
            || !report.missing_files.is_empty()
            || !report.warnings.is_empty()
        {
            return Err(format!("{path}: {report:?}"));
        }
        Ok(())
    })();
    if let Err(error) = loaded {
        script.report_load_failure(&error);
        failures.push(error);
    }
    if !failures.is_empty() {
        return failures;
    }

    if let Err(error) = adapt_manual_solo_loot(script) {
        let failure = format!("Wrath stock LootFrame bounded adapter: {error}");
        error!("ui_script: {failure}");
        script.report_load_failure(&failure);
        failures.push(failure);
    } else {
        info!(
            "ui_script: Wrath stock milestone loaded: manual solo corpse LootFrame; bind popup, master/group loot, tooltip and modified-click controls unavailable"
        );
    }
    failures
}

fn adapt_manual_solo_loot(script: &UiScript) -> Result<(), String> {
    // LootFrame.xml materializes its ordinary solo rows before this runs. GroupLootDropDown and all
    // virtual roll templates are hidden/unreferenced; removing their callbacks prevents later host
    // events from entering unsupported dropdown, popup, tooltip or roll APIs.
    script
        .run(
            r#"
        ITEM_QUALITY_COLORS = {}
        for quality = 0, 7 do
            local r, g, b, hex = GetItemQualityColor(quality)
            ITEM_QUALITY_COLORS[quality] = { r = r, g = g, b = b, hex = hex }
        end
        local stockOnEvent = LootFrame:GetScript("OnEvent")
        LootFrame:SetScript("OnEvent", function(self, event, ...)
            if event == "LOOT_CLOSED" then HideUIPanel(self)
            else stockOnEvent(self, event, ...) end
        end)
        LootFrame:UnregisterEvent("OPEN_MASTER_LOOT_LIST")
        LootFrame:UnregisterEvent("UPDATE_MASTER_LOOT_LIST")
        LootFrame:SetScript("OnHide", function() CloseLoot() end)
        for i = 1, LOOTFRAME_NUMBUTTONS do
            local button = _G["LootButton"..i]
            button:SetScript("OnEnter", nil)
            button:SetScript("OnLeave", nil)
            button:SetScript("OnUpdate", nil)
            button:SetScript("OnClick", function(self)
                -- The stock take callback first touches an unsupported distribution popup.
                -- This explicit solo adapter keeps its take verb without a fake popup global.
                LootSlot(self.slot)
            end)
        end
        "#,
        )
        .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "wrath_loot_ui_tests.rs"]
mod tests;
