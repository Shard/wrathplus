//! The quest text's write-on runs at stock's pace: `QUEST_DESCRIPTION_GRADIENT_CPS` (70) drawn
//! glyphs a second (`QuestFrame.lua:4-5`), ending when `SetAlphaGradient` reports the reveal past
//! the last drawn glyph (12340 `0x6c78f0`), whereupon Accept enables (`QuestInfo.lua:5-15`).

use wrathplus_ui::script::{QuestPanel, QuestState, UiScript};

use super::test_ui::load_ui as load_xml;

/// A 60-word body: its spaces and the full stop's trailing space draw no glyph.
const BODY: &str = "The kobolds in the Fargodeep Mine have grown bold. Slay ten of the vermin and \
                    return to me before nightfall, and the town will remember your name.";

/// The stock questgiver window, loaded and settled, with the write-on fade on.
fn questgiver() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    for file in [
        r"Interface\FrameXML\GlobalStrings.lua",
        r"Interface\FrameXML\Fonts.xml",
        r"Interface\FrameXML\BasicControls.xml",
        r"Interface\FrameXML\UIParent.xml",
        r"Interface\FrameXML\MoneyFrame.lua",
        r"Interface\FrameXML\MoneyFrame.xml",
        r"Interface\FrameXML\GameTooltip.xml",
        r"Interface\FrameXML\UIPanelTemplates.lua",
        r"Interface\FrameXML\UIPanelTemplates.xml",
        r"Interface\FrameXML\StaticPopup.xml",
        r"Interface\FrameXML\ItemButtonTemplate.xml",
        r"Interface\FrameXML\QuestFrame.xml",
        r"Interface\FrameXML\QuestLogFrame.xml",
    ] {
        load_xml(&mut s, file);
    }
    s.eval::<()>(r#"QUEST_FADING_DISABLE = "0""#).unwrap();
    for _ in 0..3 {
        s.tick(1.0);
    }
    s
}

#[test]
fn the_write_on_completes_at_seventy_drawn_glyphs_a_second_and_accept_wakes() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = questgiver();
    s.set_quest(Some(QuestState {
        panel: QuestPanel::Detail,
        title: "Bold Kobolds".into(),
        body: BODY.into(),
        ..QuestState::default()
    }));
    s.fire_event("QUEST_DETAIL", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());

    let glyphs = BODY.chars().filter(|c| !c.is_whitespace()).count() as f32;
    let chars = BODY.chars().count() as f32;
    let enabled = |s: &UiScript| {
        s.eval::<bool>("return QuestFrameAcceptButton:IsEnabled() ~= 0")
            .unwrap()
    };
    assert!(!enabled(&s), "Accept waits for the write-on");
    let dt = 1.0 / 60.0;
    let mut t = 0.0f32;
    while !enabled(&s) && t < 10.0 {
        s.tick(dt);
        t += dt;
    }
    assert!(s.errors().is_empty(), "tick errors: {:?}", s.errors());
    let expected = glyphs / 70.0;
    assert!(
        (t - expected).abs() <= 2.0 * dt,
        "the reveal ends at {glyphs} drawn glyphs / 70 = {expected:.3}s, not {t:.3}s"
    );
    assert!(
        t <= (chars + 30.0) / 70.0,
        "within (len + gradient length) / CPS: {t:.3}s"
    );
}

/// A quest with an empty description has no drawn glyph: the first advance answers false and
/// Accept wakes within a frame or two (`QuestInfo.lua:5-15`), never a locked button.
#[test]
fn an_empty_description_ends_the_write_on_at_once() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = questgiver();
    s.set_quest(Some(QuestState {
        panel: QuestPanel::Detail,
        title: "No Words".into(),
        body: String::new(),
        ..QuestState::default()
    }));
    s.fire_event("QUEST_DETAIL", vec![]);
    for _ in 0..2 {
        s.tick(1.0 / 60.0);
    }
    assert!(s.errors().is_empty(), "{:?}", s.errors());
    assert!(s
        .eval::<bool>("return QuestFrameAcceptButton:IsEnabled() ~= 0")
        .unwrap());
}
