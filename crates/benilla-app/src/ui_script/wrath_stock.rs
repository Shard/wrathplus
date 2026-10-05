//! Bounded stock build-12340 FrameXML plan: the real GameMenu art and Resume/Logout/Quit scripts.

use benilla_ui::script::UiScript;
use bevy::prelude::*;

use super::reference_ui;

/// Deliberately not `FrameXML.toc`: each added row needs its own supported dependency review.
const GAME_MENU_FILES: &[&str] = &[
    r"Interface\FrameXML\GlobalStrings.lua",
    r"Interface\FrameXML\Fonts.xml",
    r"Interface\FrameXML\FontStyles.xml",
    r"Interface\FrameXML\UIPanelTemplates.xml",
    r"Interface\FrameXML\GameMenuFrame.xml",
];

const UNSUPPORTED_BUTTONS: &[&str] = &[
    "GameMenuButtonOptions",
    "GameMenuButtonSoundOptions",
    "GameMenuButtonUIOptions",
    "GameMenuButtonMacOptions",
    "GameMenuButtonKeybindings",
    "GameMenuButtonMacros",
    "GameMenuButtonRatings",
];

/// Load only the audited stock dependency closure. Options, bindings, macros, ratings, voice and
/// bag services are not loaded or imitated; their buttons are hidden and the stock frame reports
/// that bounded shape through its actual remaining button scripts.
pub(super) fn load(script: &UiScript) -> Vec<String> {
    let mut failures = validate_plan();
    if !failures.is_empty() {
        for failure in &failures {
            error!("ui_script: Wrath stock GameMenu unavailable: {failure}");
            script.report_load_failure(failure);
        }
        return failures;
    }

    let files: Vec<String> = GAME_MENU_FILES
        .iter()
        .map(|path| (*path).to_string())
        .collect();
    failures.extend(reference_ui::addon(files.clone()).load_files(script, &files));
    if !failures.is_empty() {
        return failures;
    }

    if let Err(error) = adapt_bounded_menu(script) {
        let failure = format!("Wrath stock GameMenu bounded adapter: {error}");
        error!("ui_script: {failure}");
        script.report_load_failure(&failure);
        failures.push(failure);
    } else {
        info!(
            "ui_script: Wrath stock milestone loaded: GameMenu Resume/Logout/Quit; options, bindings, macros, ratings, voice and bag integration unavailable"
        );
    }
    failures
}

fn validate_plan() -> Vec<String> {
    let Some(bytes) = reference_ui::read(reference_ui::TOC) else {
        return vec![format!(
            "{} is absent from the player's Wrath patch chain; no stock menu was loaded",
            reference_ui::TOC
        )];
    };
    let toc = benilla_ui::toc::Toc::parse(&benilla_ui::source::decode(&bytes));
    let mut previous = None;
    let mut failures = Vec::new();
    for required in GAME_MENU_FILES {
        let leaf = required.rsplit('\\').next().unwrap_or(required);
        let position = toc
            .files
            .iter()
            .position(|entry| entry.eq_ignore_ascii_case(leaf));
        match position {
            None => failures.push(format!(
                "{} does not list required bounded dependency {leaf}",
                reference_ui::TOC
            )),
            Some(position) if previous.is_some_and(|old| position <= old) => {
                failures.push(format!(
                    "{} lists {leaf} outside the audited build-12340 dependency order",
                    reference_ui::TOC
                ))
            }
            Some(position) => previous = Some(position),
        }
    }
    failures
}

fn adapt_bounded_menu(script: &UiScript) -> Result<(), String> {
    let hidden = UNSUPPORTED_BUTTONS
        .iter()
        .map(|name| format!("{name}:Hide()"))
        .collect::<Vec<_>>()
        .join(" ");
    // These stock OnShow/OnHide calls require unimplemented MainMenuBar, bag and voice modules.
    // Replacing only the frame callbacks is an explicit bounded adapter, not a fake engine API.
    script
        .run(&format!(
            r#"
        {hidden}
        GameMenuFrame:SetScript("OnShow", nil)
        GameMenuFrame:SetScript("OnHide", nil)
        -- Stock button OnShow requires the absent CAMP/QUIT StaticPopup module. The real host
        -- logout pending state owns Enable/Disable instead; retain the stock OnClick callbacks.
        GameMenuButtonLogout:SetScript("OnShow", nil)
        GameMenuButtonQuit:SetScript("OnShow", nil)
        GameMenuFrame:SetHeight(132)
        GameMenuButtonLogout:ClearAllPoints()
        GameMenuButtonLogout:SetPoint("CENTER", GameMenuFrame, "TOP", 0, -39)
        GameMenuButtonQuit:ClearAllPoints()
        GameMenuButtonQuit:SetPoint("TOP", GameMenuButtonLogout, "BOTTOM", 0, -1)
        GameMenuButtonContinue:ClearAllPoints()
        GameMenuButtonContinue:SetPoint("TOP", GameMenuButtonQuit, "BOTTOM", 0, -16)
        "#
        ))
        .map_err(|error| error.to_string())
}

pub(super) fn toggle(script: &UiScript) -> Result<bool, String> {
    script
        .eval(
            "if GameMenuFrame:IsShown() then HideUIPanel(GameMenuFrame); return false \
             else ShowUIPanel(GameMenuFrame); return true end",
        )
        .map_err(|error| error.to_string())
}

pub(super) fn set_session_actions_enabled(script: &UiScript, enabled: bool) -> Result<(), String> {
    script
        .run(if enabled {
            "GameMenuButtonLogout:Enable(); GameMenuButtonQuit:Enable()"
        } else {
            "GameMenuButtonLogout:Disable(); GameMenuButtonQuit:Disable()"
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_ui::script::{ScriptDialect, SessionRequest};

    #[test]
    fn installed_chain_loads_only_the_bounded_real_game_menu() {
        benilla_formats::wow_data_or_skip!();
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let root = super::super::wrath_addons::load_bootstrap(&script);
        assert!(root.is_empty(), "{root:#?}");
        let failures = load(&script);
        assert!(failures.is_empty(), "{failures:#?}");
        assert!(!script.frame_visible("GameMenuFrame"));
        assert!(toggle(&script).unwrap());
        script.resolve();
        assert!(script.frame_visible("GameMenuFrame"));
        assert!(script
            .eval::<bool>(
                "return not GameMenuButtonOptions:IsShown() and GameMenuButtonContinue:IsShown()"
            )
            .unwrap());
        script.run("GameMenuButtonContinue:Click()").unwrap();
        assert!(!script
            .eval::<bool>("return GameMenuFrame:IsShown()")
            .unwrap());
    }

    #[test]
    fn stock_logout_and_quit_callbacks_queue_distinct_real_session_intents() {
        benilla_formats::wow_data_or_skip!();
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        assert!(super::super::wrath_addons::load_bootstrap(&script).is_empty());
        assert!(load(&script).is_empty());
        script.run("GameMenuButtonLogout:Click()").unwrap();
        assert_eq!(script.take_session_requests(), [SessionRequest::Logout]);
        script.run("GameMenuButtonQuit:Click()").unwrap();
        assert_eq!(script.take_session_requests(), [SessionRequest::Quit]);
    }
}
