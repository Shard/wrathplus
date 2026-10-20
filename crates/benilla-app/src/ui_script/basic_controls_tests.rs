//! The stock `BasicControls.xml`: `message`, `_ERRORMESSAGE`, `TEXT` and the `DialogBoxFrame`
//! kit, driven from Lua the way addons call them.

use benilla_ui::script::UiScript;

use super::test_ui::load_ui as load_xml;

/// Fonts.xml is BasicControls.xml's one dependency.
fn basic_controls() -> UiScript {
    let mut s = UiScript::new().unwrap();
    // 3.3.5's `_ERRORMESSAGE` shows `BasicScriptErrors` only with `scriptErrors` on (its default is
    // "0") and `Blizzard_DebugTools` not loaded, as here.
    s.run("SetCVar('scriptErrors', '1')").unwrap();
    load_xml(&s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&s, "Interface\\FrameXML\\BasicControls.xml");
    s.set_screen_size(1024.0, 768.0);
    s.resolve();
    s
}

#[test]
fn message_shows_the_script_errors_dialog_carrying_its_text() {
    benilla_formats::wow_data_or_skip!();
    let s = basic_controls();
    assert!(
        !s.eval::<bool>("return BasicScriptErrors:IsVisible()")
            .unwrap(),
        "the dialog starts hidden (DialogBoxFrame's `hidden=true`)"
    );

    s.run(r#"message("Validation error: [nil]")"#).unwrap();
    assert!(s.errors().is_empty(), "{:?}", s.errors());
    assert!(s
        .eval::<bool>("return BasicScriptErrors:IsVisible()")
        .unwrap());
    assert_eq!(
        s.eval::<String>("return BasicScriptErrorsText:GetText()")
            .unwrap(),
        "Validation error: [nil]"
    );

    // A second message while the dialog is up keeps the first (`BasicControls.xml:7`).
    s.run(r#"message("second")"#).unwrap();
    assert_eq!(
        s.eval::<String>("return BasicScriptErrorsText:GetText()")
            .unwrap(),
        "Validation error: [nil]",
        "`if not BasicScriptErrors:IsShown()` — the first error is the one you keep"
    );

    s.run("BasicScriptErrorsButton:Click()").unwrap();
    assert!(!s
        .eval::<bool>("return BasicScriptErrors:IsVisible()")
        .unwrap());
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn error_message_returns_its_argument_and_message_shows_the_dialog_itself() {
    benilla_formats::wow_data_or_skip!();
    let s = basic_controls();
    assert_eq!(
        s.eval::<String>(r#"return _ERRORMESSAGE("boom")"#).unwrap(),
        "boom",
        "the ref returns `message` — a caller may chain on it"
    );

    // 3.3.5's `message` fills `BasicScriptErrors` itself; only the handler `seterrorhandler`
    // installed reaches `_ERRORMESSAGE`, so a replacement takes effect once it is re-installed. The
    // "boom" dialog is up, and `message` keeps a shown one, so close it first.
    s.run(
        r#"BasicScriptErrors:Hide()
           Captured = nil
           _ERRORMESSAGE = function(m) Captured = m return m end
           message("routed")"#,
    )
    .unwrap();
    assert!(s.eval::<bool>("return Captured == nil").unwrap());
    assert_eq!(
        s.eval::<String>("return BasicScriptErrorsText:GetText()")
            .unwrap(),
        "routed"
    );
    s.run(r#"seterrorhandler(_ERRORMESSAGE) geterrorhandler()("handled")"#)
        .unwrap();
    assert_eq!(s.eval::<String>("return Captured").unwrap(), "handled");
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

/// `BasicControls.xml:16` installs `_ERRORMESSAGE` as the error handler.
#[test]
fn error_message_is_the_installed_handler_and_the_host_channel_stays_sighted() {
    benilla_formats::wow_data_or_skip!();
    let mut s = basic_controls();
    assert!(
        s.eval::<bool>("return geterrorhandler() == _ERRORMESSAGE")
            .unwrap(),
        "the reference's own default handler is installed"
    );
    // An addon's pcall wrapper reports through `geterrorhandler()`, which pops the dialog.
    s.run(r#"geterrorhandler()("reported")"#).unwrap();
    assert!(
        s.eval::<bool>("return BasicScriptErrors:IsVisible()")
            .unwrap(),
        "the dialog is the report now"
    );
    assert_eq!(
        s.eval::<String>("return BasicScriptErrorsText:GetText()")
            .unwrap(),
        "reported"
    );
    // An engine-caught error still lands in `errors()`; the dispatch adds the dialog on top.
    s.run("BrokenProbe = CreateFrame('Frame') BrokenProbe:RegisterEvent('ERROR_PROBE') BrokenProbe:SetScript('OnEvent', function() error('fault') end)")
        .unwrap();
    s.fire_event("ERROR_PROBE", vec![]);
    assert!(
        s.errors().iter().any(|e| e.contains("fault")),
        "engine-caught errors stay on the host channel: {:?}",
        s.errors()
    );
    s.dispatch_script_errors_to_handler();
    assert!(
        s.eval::<bool>("return BasicScriptErrors:IsVisible()")
            .unwrap(),
        "and the dispatch keeps the dialog up for the player"
    );
}

#[test]
fn text_is_the_identity_function_the_corpus_expects() {
    benilla_formats::wow_data_or_skip!();
    let s = basic_controls();
    assert_eq!(
        s.eval::<String>(r#"return TEXT("Level")"#).unwrap(),
        "Level"
    );
    assert_eq!(s.eval::<i64>("return TEXT(7)").unwrap(), 7);
}

#[test]
fn a_dialog_box_from_the_template_names_and_closes_the_callers_frame() {
    benilla_formats::wow_data_or_skip!();
    let s = basic_controls();
    s.run(
        r#"MyDialog = CreateFrame("Frame", "MyDialog", UIParent, "DialogBoxFrame")
           MyDialog:SetWidth(384) MyDialog:SetHeight(128)
           MyDialog:Show()"#,
    )
    .unwrap();
    assert!(s.errors().is_empty(), "{:?}", s.errors());
    assert!(
        s.eval::<bool>(r#"return getglobal("MyDialogButton") ~= nil"#)
            .unwrap(),
        "the caller's name won through $parent"
    );
    assert!(
        s.eval::<bool>(r#"return getglobal("DialogBoxFrameButton") == nil"#)
            .unwrap(),
        "and the template's own name published nothing"
    );
    assert_eq!(
        s.eval::<String>(r#"return getglobal("MyDialogButton"):GetText()"#)
            .unwrap(),
        "OKAY"
    );

    s.run("MyDialogButton:Click()").unwrap();
    assert!(
        !s.eval::<bool>("return MyDialog:IsVisible()").unwrap(),
        "`this:GetParent():Hide()` closed the caller's frame, not the template's"
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}
