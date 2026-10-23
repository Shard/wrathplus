//! The stock 3.3.5 tutorial window (`TutorialFrame.xml`), driven by `TUTORIAL_TRIGGER`: one alert
//! button over a queue, and out of combat a new tutorial opens the window at once
//! (`TutorialFrame_NewTutorial`, TutorialFrame.lua:814-842). 1.12's numbered alert buttons, the
//! window's "Display Tips" box (moved to the Interface Options) and its `CINEMATIC_STOP` arm are gone.

use wrathplus_ui::script::{ScriptValue, UiScript};

use super::test_ui::load_ui as load_xml;

fn session() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\TutorialFrame.xml");
    // A fresh character's completed bank: nothing flagged yet.
    s.set_wrath_tutorial_banks(Some(vec![0; 8]), Some(vec![0; 8]));
    s.resolve();
    s
}

fn visible(s: &UiScript, frame: &str) -> bool {
    s.eval::<bool>(&format!("return {frame}:IsVisible()"))
        .unwrap()
}

fn title(s: &UiScript) -> String {
    s.eval::<String>("return TutorialFrameTitle:GetText()")
        .unwrap()
}

fn trigger(s: &mut UiScript, id: i64) {
    s.fire_event("TUTORIAL_TRIGGER", vec![ScriptValue::Int(id)]);
}

/// A trigger out of combat shows the alert and opens the window on it, which flags it
/// (`TutorialFrame_Update`, TutorialFrame.lua:649-686); a second waits in the queue until Okay
/// (`TutorialFrame_Hide`, :873-879) opens it.
#[test]
fn a_trigger_opens_the_window_and_a_second_waits_for_okay() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = session();
    assert!(!visible(&s, "TutorialFrameAlertButton"));
    assert!(!visible(&s, "TutorialFrame"));
    // `OnLoad` builds the key strings on a virtual `<Font>` (TutorialFrame.lua:615, FontStyles.xml:122).
    assert_eq!(
        s.eval::<(String, f64, f64, f64)>(
            "local k = TutorialFrameKeyString1 local r, g, b = k:GetTextColor() \
             return k:GetFontObject():GetName(), r, g, b"
        )
        .unwrap(),
        ("GameFontNormalHugeBlack".into(), 0.0, 0.0, 0.0)
    );

    trigger(&mut s, 41);
    assert!(visible(&s, "TutorialFrameAlertButton"), "the alert shows");
    assert!(
        visible(&s, "TutorialFrame"),
        "out of combat it opens at once"
    );
    assert_eq!(title(&s), "Elite Quests");
    assert!(s
        .eval::<String>("return TutorialFrameText:GetText()")
        .unwrap()
        .starts_with("You have accepted an elite quest"));
    assert!(
        s.eval::<bool>("return IsTutorialFlagged(41) == 1").unwrap(),
        "opening it flags it"
    );

    trigger(&mut s, 42);
    assert_eq!(
        title(&s),
        "Elite Quests",
        "the open window keeps its tutorial"
    );
    assert!(!s.eval::<bool>("return IsTutorialFlagged(42) == 1").unwrap());

    s.run("TutorialFrameOkayButton:Click()").unwrap();
    assert!(visible(&s, "TutorialFrame"), "Okay opens the queued one");
    assert_eq!(title(&s), "Welcome to World of Warcraft!");
    assert!(s.eval::<bool>("return IsTutorialFlagged(42) == 1").unwrap());

    // A flagged tutorial never re-queues.
    s.run("TutorialFrameOkayButton:Click()").unwrap();
    trigger(&mut s, 41);
    assert!(!visible(&s, "TutorialFrame"), "41 is already flagged");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}
