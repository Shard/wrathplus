//! The character window (C) on the installed 3.3.5 interface, loaded as a session loads it.

use wrathplus_ui::script::UiScript;

/// `ToggleCharacter("PaperDollFrame")`, the micro button's and the binding's route, opens the window
/// on its paper doll without a raise, and toggles it shut again. Stock `CharacterFrame_ShowSubFrame`
/// walks all five subframes, `TokenFrame` included, so this needs the startup `Blizzard_TokenUI`.
#[test]
fn the_character_window_opens_on_its_paper_doll_without_a_raise() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().expect("a VM");
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "load failures: {failures:#?}");
    for event in ["VARIABLES_LOADED", "PLAYER_LOGIN", "PLAYER_ENTERING_WORLD"] {
        s.fire_event(event, vec![]);
    }
    s.resolve();
    let before = s.errors();
    assert!(before.is_empty(), "raised at login: {before:#?}");
    assert!(
        s.eval::<bool>("return TokenFrame ~= nil").unwrap(),
        "Blizzard_TokenUI loaded at startup"
    );

    s.run("ToggleCharacter('PaperDollFrame')")
        .expect("ToggleCharacter raises nothing");
    s.resolve();
    assert!(
        s.eval::<bool>(
            "return (CharacterFrame:IsShown() and PaperDollFrame:IsShown()) and true or false"
        )
        .unwrap(),
        "the window is open on its paper doll"
    );
    s.run("ToggleCharacter('PaperDollFrame')")
        .expect("the second toggle raises nothing");
    s.resolve();
    assert!(
        !s.eval::<bool>("return CharacterFrame:IsShown() and true or false")
            .unwrap(),
        "the same tab toggles it shut"
    );
    let errors = s.errors();
    assert!(errors.is_empty(), "script errors: {errors:#?}");
}
