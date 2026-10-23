//! The stock GM help window (`HelpFrame.xml`): the two faces of `UPDATE_TICKET`, the ticket
//! toast and its repoll, and the status ask on show.

use wrathplus_ui::script::{
    EditAction, EditBoxAdvanceRequest, EditUnit, GmTicketIntent, MeasureRequest, ScriptValue,
    TextMeasure, UiScript,
};

use super::test_ui::load_ui as load_xml;

/// The window and its dependencies, with the ten `GMTicketCategory.dbc` rows the app pushes.
fn setup() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    for file in [
        "Interface\\FrameXML\\GlobalStrings.lua",
        "Interface\\FrameXML\\Fonts.xml",
        "Interface\\FrameXML\\BasicControls.xml",
        r"Interface\FrameXML\UIParent.xml",
        r"Interface\FrameXML\MoneyFrame.lua",
        // Before StaticPopup.xml, whose coin row inherits `SmallMoneyFrameTemplate`.
        r"Interface\FrameXML\MoneyFrame.lua",
        r"Interface\FrameXML\MoneyFrame.xml",
        "Interface\\FrameXML\\GameTooltip.xml",
        r"Interface\FrameXML\UIPanelTemplates.lua",
        r"Interface\FrameXML\UIPanelTemplates.xml",
        "Interface\\FrameXML\\StaticPopup.xml",
        "Interface\\FrameXML\\TextStatusBar.lua",
        "Interface\\FrameXML\\TextStatusBar.xml",
        "Interface\\FrameXML\\MainMenuBar.xml",
        r"Interface\FrameXML\MainMenuBarMicroButtons.xml",
        // `HelpFrame_OnShow` calls `UpdateMicroButtons()` before `GetGMStatus()`
        // (`HelpFrame.lua:59-61`), so the micro row and the bar's button kit load too.
        "Interface\\FrameXML\\Cooldown.xml",
        "Interface\\FrameXML\\ActionButtonTemplate.xml",
        "Interface\\FrameXML\\ActionBarFrame.xml",
        // `TicketStatusFrame_OnShow` re-anchors `ConsolidatedBuffs` (`HelpFrame.lua:408-410`).
        "Interface\\FrameXML\\BuffFrame.xml",
        "Interface\\FrameXML\\BonusActionBarFrame.xml",
        "Interface\\FrameXML\\HelpFrame.xml",
        // The start page `ToggleHelpFrame` opens, and whose buttons `UPDATE_TICKET` swaps
        // (`HelpFrame.lua:155-172`).
        "Interface\\FrameXML\\KnowledgeBaseFrame.xml",
    ] {
        load_xml(&mut s, file);
    }
    s.set_gm_ticket_categories(vec![
        (1, "Stuck".into()),
        (2, "Behavior/Harassment".into()),
        (3, "Guild".into()),
        (4, "Item".into()),
        (5, "Environmental".into()),
        (6, "Non-Quest/Creep".into()),
        (7, "Quest/Quest NPC".into()),
        (8, "Technical".into()),
        (9, "Account/Billing".into()),
        (10, "Character".into()),
    ]);
    s
}

/// `UPDATE_TICKET`'s args for an open ticket; must match `ui_gm_ticket::update_ticket_args`.
fn open_ticket_args(
    category: i64,
    text: &str,
    age: f64,
    oldest: f64,
    update: f64,
) -> Vec<ScriptValue> {
    vec![
        ScriptValue::Int(category),
        ScriptValue::Str(text.into()),
        ScriptValue::Number(age),
        ScriptValue::Number(oldest),
        ScriptValue::Number(update),
        ScriptValue::Int(0),
        ScriptValue::Int(0),
    ]
}

/// With a ticket the form is an editor (Save Changes, its description in the box); `UPDATE_TICKET`
/// with no arguments makes it a form again (`HelpFrame.lua:96-174`).
#[test]
fn an_open_ticket_turns_the_form_into_an_editor_and_no_ticket_turns_it_back() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = setup();
    s.run("HelpFrame_ShowFrame(\"OpenTicket\")").unwrap();

    s.fire_event(
        "UPDATE_TICKET",
        open_ticket_args(7, "Where is this NPC?", 0.25, 2.5, 0.01),
    );
    assert_eq!(
        s.eval::<String>("return HelpFrameOpenTicketEditBox:GetText()")
            .unwrap(),
        "Where is this NPC?",
        "arg2 is the description"
    );
    assert_eq!(
        s.eval::<(String, String)>(
            "return HelpFrameOpenTicketSubmit:GetText(), HelpFrameOpenTicketLabel:GetText()"
        )
        .unwrap(),
        ("Save Changes".into(), "Your Current Issue:".into())
    );
    assert!(s
        .eval::<bool>("return KnowledgeBaseFrameEditTicket:IsShown()")
        .unwrap());

    s.fire_event("UPDATE_TICKET", vec![]);
    assert_eq!(
        s.eval::<String>("return HelpFrameOpenTicketEditBox:GetText()")
            .unwrap(),
        "",
        "the editor empties"
    );
    assert_eq!(
        s.eval::<String>("return HelpFrameOpenTicketSubmit:GetText()")
            .unwrap(),
        "Submit"
    );
    assert!(!s
        .eval::<bool>("return KnowledgeBaseFrameEditTicket:IsShown()")
        .unwrap());
}

#[test]
fn the_ticket_toast_follows_the_ticket() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = setup();
    s.fire_event(
        "UPDATE_TICKET",
        open_ticket_args(1, "Stuck.", 0.1, 0.2, 0.01),
    );
    assert!(
        s.eval::<bool>("return TicketStatusFrame:IsVisible()")
            .unwrap(),
        "a ticket raises the toast"
    );
    s.fire_event("UPDATE_TICKET", vec![]);
    assert!(
        !s.eval::<bool>("return TicketStatusFrame:IsVisible()")
            .unwrap(),
        "and abandoning it takes the toast away"
    );
}

/// `TicketStatusFrame_OnUpdate` re-asks every `GMTICKET_CHECK_INTERVAL`, 600 s, while a ticket is
/// open (`HelpFrame.lua:4`, `:391-402`).
#[test]
fn the_toast_repolls_the_server_only_after_the_full_interval() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = setup();
    s.fire_event(
        "UPDATE_TICKET",
        open_ticket_args(1, "Stuck.", 0.1, 0.2, 0.01),
    );
    let _ = s.take_gm_ticket_intents();

    s.run("TicketStatusFrame_OnUpdate(TicketStatusFrame, 599)")
        .unwrap();
    assert!(s.take_gm_ticket_intents().is_empty(), "not yet");
    s.run("TicketStatusFrame_OnUpdate(TicketStatusFrame, 2)")
        .unwrap();
    assert_eq!(
        s.take_gm_ticket_intents(),
        vec![GmTicketIntent::Ask],
        "600s elapsed — re-ask"
    );
    s.run("TicketStatusFrame_OnUpdate(TicketStatusFrame, 1)")
        .unwrap();
    assert!(
        s.take_gm_ticket_intents().is_empty(),
        "and the clock restarts"
    );
}

/// `ToggleHelpFrame` opens the knowledge base page (`UIParent.lua:431-442`); the window's
/// `OnShow` asks the queue status and the page's asks it again with the ticket
/// (`HelpFrame.lua:61`, `KnowledgeBaseFrame.lua:51-52`).
#[test]
fn toggling_the_window_opens_it_and_asks_for_the_queue_status() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = setup();
    s.run("ToggleHelpFrame()").unwrap();
    assert!(s.eval::<bool>("return HelpFrame:IsVisible()").unwrap());
    assert_eq!(
        s.take_gm_ticket_intents(),
        vec![
            GmTicketIntent::AskStatus,
            GmTicketIntent::AskStatus,
            GmTicketIntent::Ask
        ],
        "OnShow calls GetGMStatus — the gate must not run on an assumption"
    );
    assert!(
        s.eval::<bool>("return KnowledgeBaseFrame:IsVisible()")
            .unwrap(),
        "and it opens on the knowledge base"
    );

    s.run("ToggleHelpFrame()").unwrap();
    assert!(!s.eval::<bool>("return HelpFrame:IsVisible()").unwrap());
}

/// The ticket text is a multi-line box and the scroll child itself (`HelpFrame.xml`, 541×357 in
/// the 378-tall `HelpFrameOpenTicketScrollFrame`): the box sizes itself to its wrapped text
/// (`0x77d4d0` @`0x77d8ad`), so `ScrollingEdit_OnTextChanged`'s recompute ranges over all of it.
#[test]
fn the_ticket_text_grows_with_its_lines_and_its_scroll_frame_ranges_over_them() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = setup();
    // Wide glyphs and the stand-in's 12-unit line: the box's 500-letter cap must still wrap
    // past the frame.
    const GLYPH: f64 = 60.0;
    s.set_text_measurer(Box::new(super::FixedWidthFont(GLYPH as f32)));
    s.run("HelpFrame_ShowFrame(\"OpenTicket\")").unwrap();
    s.resolve();
    s.tick(0.016);
    s.resolve();
    assert_eq!(
        s.eval::<f64>("return HelpFrameOpenTicketEditBox:GetHeight()")
            .unwrap(),
        12.0,
        "empty, the box is one line: the authored 357 lasts until the first flush"
    );

    s.run(r#"HelpFrameOpenTicketEditBox:SetText(string.rep("x", 400))"#)
        .unwrap();
    s.tick(0.016);
    s.resolve();
    // Wrapped at the box's own 541: no insets.
    let height = (400.0 * GLYPH / 541.0).ceil() * 12.0;
    assert_eq!(
        s.eval::<f64>("return HelpFrameOpenTicketEditBox:GetHeight()")
            .unwrap(),
        height
    );
    assert_eq!(
        s.eval::<f64>("return HelpFrameOpenTicketScrollFrame:GetVerticalScrollRange()")
            .unwrap(),
        height - 378.0
    );
    assert_eq!(
        s.eval::<f64>(
            "local _, max = HelpFrameOpenTicketScrollFrameScrollBar:GetMinMaxValues() return max"
        )
        .unwrap(),
        height - 378.0,
        "the range reached the scroll bar through OnScrollRangeChanged"
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

/// A stand-in font engine: each byte 7 wide, each line 14 tall, a multi-line box's rows starting
/// after each newline, a trailing one opening an empty row as the reference's break routine does
/// (`0x5c250b`–`0x5c2522`); the texts here never wrap.
struct Rows;

impl TextMeasure for Rows {
    fn measure(&mut self, req: &MeasureRequest) -> (f32, f32, f32) {
        let natural = req.text.split('\n').map(str::len).max().unwrap_or(0) as f32 * 7.0;
        (natural, req.text.split('\n').count() as f32 * 14.0, natural)
    }

    fn editbox_advances(
        &mut self,
        req: &EditBoxAdvanceRequest,
    ) -> Option<(Vec<f32>, Vec<usize>, f32)> {
        let cum = (0..=req.text.len()).map(|i| i as f32 * 7.0).collect();
        if req.wrap_width.is_none() {
            return Some((cum, vec![0], 0.0));
        }
        let breaks = req.text.match_indices('\n').map(|(i, _)| i + 1);
        let rows = std::iter::once(0).chain(breaks).collect();
        Some((cum, rows, 14.0))
    }
}

/// The ticket text is multi-line: UP moves the caret a row at its letter column (`0x77cb20`), and
/// Shift+HOME selects back to the line's start (`0x77c980`), not the ticket's.
#[test]
fn the_ticket_texts_caret_moves_by_rows_and_lines() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = setup();
    s.set_text_measurer(Box::new(Rows));
    s.run("HelpFrame_ShowFrame(\"OpenTicket\")").unwrap();
    s.resolve();
    s.run(
        "HelpFrameOpenTicketEditBox:SetText('Stuck in\\nthe mine') \
         HelpFrameOpenTicketEditBox:SetFocus()",
    )
    .unwrap();
    s.tick(0.016);
    s.resolve();
    let mv = |s: &mut UiScript, unit, back, extend| {
        assert!(s.editbox_action(EditAction::Move { unit, back, extend }));
    };
    // From the end, column 8: the end of "Stuck in".
    mv(&mut s, EditUnit::Row, true, false);
    s.char_input(",");
    mv(&mut s, EditUnit::Edge, false, false);
    mv(&mut s, EditUnit::Line, true, true);
    s.char_input("a cave");
    assert_eq!(
        s.eval::<String>("return HelpFrameOpenTicketEditBox:GetText()")
            .unwrap(),
        "Stuck in,\na cave"
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}
