//! The stock login's password box masks what it draws: `AccountLoginPasswordEdit` is declared
//! `password="1"` (`AccountLogin.xml:239`), which 12340's XML boolean reads as on (`0x815400`).

use super::*;
use wrathplus_ui::script::QuadContent;

/// The stock login screen, up.
fn login() -> UiScript {
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.set_screen_size(1024.0, 768.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![ScriptValue::Str("login".into())]);
    script.tick(0.1);
    script
}

/// Every text the frame draws, from the extracted quads.
fn drawn_texts(script: &mut UiScript) -> Vec<String> {
    script.resolve();
    script
        .extract()
        .into_iter()
        .filter_map(|q| match q.content {
            QuadContent::Text { text: Some(t), .. } => Some(t),
            _ => None,
        })
        .collect()
}

#[test]
fn the_password_box_draws_stars_and_keeps_its_text_while_the_account_box_shows_it() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut script = login();
    script
        .run(
            "AccountLoginAccountEdit:SetText('Probeacct') \
             AccountLoginPasswordEdit:SetText('hunter2')",
        )
        .unwrap();
    script.tick(0.016);
    let (pass, flag, acct, acct_flag): (String, Option<i64>, String, Option<i64>) = script
        .eval(
            "return AccountLoginPasswordEdit:GetText(), AccountLoginPasswordEdit:IsPassword(), \
             AccountLoginAccountEdit:GetText(), AccountLoginAccountEdit:IsPassword()",
        )
        .unwrap();
    assert_eq!(pass, "hunter2", "GetText keeps the real password");
    assert_eq!(flag, Some(1), "password=\"1\" is on");
    assert_eq!((acct.as_str(), acct_flag), ("Probeacct", None));

    let texts = drawn_texts(&mut script);
    assert!(
        !texts.iter().any(|t| t.contains("hunter2")),
        "the password is never drawn: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "*******"),
        "one '*' per character: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "Probeacct"),
        "the account box is unmasked: {texts:?}"
    );

    // A runtime SetPassword re-masks text already present, and lifting it shows the text.
    script
        .run("AccountLoginAccountEdit:SetPassword(true)")
        .unwrap();
    let texts = drawn_texts(&mut script);
    assert!(!texts.iter().any(|t| t == "Probeacct") && texts.iter().any(|t| t == "*********"));
    script
        .run("AccountLoginAccountEdit:SetPassword(false)")
        .unwrap();
    assert!(drawn_texts(&mut script).iter().any(|t| t == "Probeacct"));
}
