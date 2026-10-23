//! B4c: the stock glue options panels on the glue VM. They are GlueXML's own copies of the
//! options frames, served by the same bindings as in game; their writes reach the client's CVar
//! table through `cvars::sync_cvars`, which drains whichever VM is up, the glue VM included.

use wrathplus_ui::script::{ScriptValue, UiScript};

use crate::ui_script::glue::load_glue_ui;

/// The glue VM on the login screen with the shipped CVars, as a live session seeds it.
fn glue_vm() -> Option<UiScript> {
    wrathplus_formats::wow_data()?;
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.register_cvars(crate::shipped_cvars::pairs());
    script.set_screen_size(1600.0, 1000.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![ScriptValue::Str("login".into())]);
    script.tick(0.1);
    Some(script)
}

/// Options → Sound: a slider move and Okay queue the CVar write `sync_cvars` drains; Video opens
/// and closes through its Okay with no script error. Skips without an install.
#[test]
fn the_stock_glue_options_write_through_to_the_cvars_on_the_installed_data() {
    let Some(mut script) = glue_vm() else {
        eprintln!("skipped: no installed client");
        return;
    };
    let _ = script.take_cvar_changes();
    let before = script.warnings().len();
    script
        .run(
            "OptionsSelectFrame:Show() OptionsSelectFrameBackgroundContainerAudioOptionsButton:Click() \
             AudioOptionsSoundPanelMasterVolume:SetValue(0.3) AudioOptionsFrameOkay:Click()",
        )
        .unwrap();
    let changes = script.take_cvar_changes();
    assert!(
        changes
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("Sound_MasterVolume") && v.starts_with("0.3")),
        "{changes:?}"
    );
    script
        .run(
            "OptionsSelectFrameBackgroundContainerVideoOptionsButton:Click() VideoOptionsFrameOkay:Click() \
             OptionsSelectFrameBackgroundOkayButton:Click()",
        )
        .unwrap();
    script.tick(0.1);
    let shown = script
        .eval::<String>(
            "return tostring(OptionsSelectFrame:IsShown())..' '..tostring(VideoOptionsFrame:IsShown())",
        )
        .unwrap();
    assert_eq!(shown, "nil nil", "both closed");
    let warnings = script.warnings();
    let errors: Vec<_> = warnings[before..]
        .iter()
        .filter(|w| w.contains("error") || w.contains("attempt"))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}
