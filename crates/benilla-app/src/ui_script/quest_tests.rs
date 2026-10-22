//! The stock questgiver window (`QuestFrame.xml`), driven engine-only through the quest events.

use benilla_ui::script::{QuestPanel, QuestState, ScriptValue, SoundRequest, UiScript};

use super::test_ui::load_ui as load_xml;

/// Stock `QuestFrame_OnShow` and `QuestFrame_OnHide` play `igQuestListOpen` and `igQuestListClose`
/// (`QuestFrame.lua:293`, `:297`).
#[test]
fn questgiver_show_hide_plays_open_and_close_kits() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");

    assert!(
        s.take_sounds().is_empty(),
        "no sound at load (never transitions)"
    );

    s.set_quest(Some(QuestState {
        panel: QuestPanel::Detail,
        title: "A Threat Within".into(),
        body: "Speak with the captain.".into(),
        objectives: "Report to the captain.".into(),
        ..QuestState::default()
    }));
    s.fire_event("QUEST_DETAIL", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(
        s.eval::<bool>("return QuestFrame:IsVisible()").unwrap(),
        "the questgiver window opened onto the left slot"
    );
    assert_eq!(
        s.take_sounds(),
        vec![SoundRequest::KitName("igQuestListOpen".into())],
        "opening the questgiver window plays igQuestListOpen"
    );

    s.fire_event("QUEST_FINISHED", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(
        !s.eval::<bool>("return QuestFrame:IsVisible()").unwrap(),
        "QUEST_FINISHED hid the questgiver window"
    );
    assert_eq!(
        s.take_sounds(),
        vec![SoundRequest::KitName("igQuestListClose".into())],
        "closing the questgiver window plays igQuestListClose"
    );
}

/// Each stock `QuestFrame*Panel_OnShow` hides its three siblings; only the window's own show and
/// hide play a sound.
#[test]
fn panel_events_show_exactly_one_child_panel_and_hide_the_others() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");

    let panels = [
        "QuestFrameGreetingPanel",
        "QuestFrameDetailPanel",
        "QuestFrameProgressPanel",
        "QuestFrameRewardPanel",
    ];
    let is_shown =
        |s: &UiScript, name: &str| s.eval::<bool>(&format!("return {name}:IsShown()")).unwrap();

    s.set_quest(Some(QuestState {
        panel: QuestPanel::Greeting,
        greeting: "What can I do for you?".into(),
        active_titles: vec!["Report to Goldshire".into()],
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Deputy Willem".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_GREETING", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    for name in panels {
        assert_eq!(
            is_shown(&s, name),
            name == "QuestFrameGreetingPanel",
            "only the greeting panel is shown after QUEST_GREETING"
        );
    }
    assert_eq!(
        s.eval::<String>("return QuestFrameNpcNameText:GetText()")
            .unwrap(),
        "Deputy Willem"
    );
    // Drain the first show's open sound; the switch below must play none.
    s.take_sounds();

    s.set_quest(Some(QuestState {
        panel: QuestPanel::Detail,
        title: "A Threat Within".into(),
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Deputy Willem".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_DETAIL", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    for name in panels {
        assert_eq!(
            is_shown(&s, name),
            name == "QuestFrameDetailPanel",
            "only the detail panel is shown after QUEST_DETAIL"
        );
    }
    assert!(
        s.take_sounds().is_empty(),
        "a panel switch with the window already open plays no kit (only OnShow/OnHide of the window do)"
    );
}

/// Stock `QuestFrameItems_Update` fills one pool of 10 per panel: choices, the spell row, then
/// fixed rewards, so two choices and one reward are `Item1`, `Item2` and `Item3`.
#[test]
fn detail_panel_reward_grid_follows_the_refs_two_per_row_layout() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");

    let choice = |name: &str, quality: u32| benilla_ui::script::QuestItemView {
        item_id: 0,
        name: Some(name.into()),
        texture: None,
        count: 1,
        quality,
        usable: true,
        ..Default::default()
    };
    s.set_quest(Some(QuestState {
        panel: QuestPanel::Detail,
        title: "A Threat Within".into(),
        body: "Kill the kobolds infesting the mine.".into(),
        objectives: "Slay 10 Kobold Vermin.".into(),
        choices: vec![choice("Worn Sword", 1), choice("Worn Mace", 1)],
        rewards: vec![choice("Militia Hammer", 1)],
        reward_money: 150,
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Marshal McBride".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_DETAIL", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());

    assert!(s.eval::<bool>("return QuestInfoItem1:IsShown()").unwrap());
    assert!(s.eval::<bool>("return QuestInfoItem2:IsShown()").unwrap());
    assert!(!s.eval::<bool>("return QuestInfoItem4:IsShown()").unwrap());
    assert_eq!(
        s.eval::<String>("return QuestInfoItem1Name:GetText()")
            .unwrap(),
        "Worn Sword"
    );
    assert_eq!(
        s.eval::<String>("return QuestInfoItemChooseText:GetText()")
            .unwrap(),
        "You will be able to choose one of these rewards:"
    );
    assert_eq!(
        s.eval::<String>("return QuestInfoItemReceiveText:GetText()")
            .unwrap(),
        "You will also receive:",
        "choices present -> the ref's \"also\" wording (QuestInfo.lua:322)"
    );
    assert!(s.eval::<bool>("return QuestInfoItem3:IsShown()").unwrap());
    assert!(!s.eval::<bool>("return QuestInfoItem4:IsShown()").unwrap());
    assert_eq!(
        s.eval::<String>("return QuestInfoItem3Name:GetText()")
            .unwrap(),
        "Militia Hammer"
    );
    assert_eq!(
        s.eval::<String>("return QuestInfoRewardsHeader:GetText()")
            .unwrap(),
        "Rewards"
    );

    // A row selects only under `QuestInfoFrame.chooseItems` (`QuestInfo.xml:45`), which only the
    // reward template sets (`QuestInfo.lua:541`).
    s.run("QuestInfoItem1:Click()").ok();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert_eq!(
        s.eval::<i64>("return QuestInfoFrame.itemChoice or 0")
            .unwrap(),
        0,
        "a detail-panel row never selects a reward"
    );
}

/// A reward row click selects its choice (`QuestInfo.lua:26-32`).
#[test]
fn reward_panel_choice_click_selects_and_completes_with_zero_based_index() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");

    let choice = |name: &str| benilla_ui::script::QuestItemView {
        item_id: 0,
        name: Some(name.into()),
        texture: None,
        count: 1,
        quality: 1,
        usable: true,
        ..Default::default()
    };
    s.set_quest(Some(QuestState {
        panel: QuestPanel::Reward,
        title: "A Threat Within".into(),
        body: "Well done.".into(),
        choices: vec![choice("Worn Sword"), choice("Worn Mace")],
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Marshal McBride".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_COMPLETE", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());

    assert!(
        !s.eval::<bool>("return QuestInfoItemHighlight:IsShown()")
            .unwrap(),
        "no row picked yet"
    );
    // No choice picked: `QuestChooseRewardError()` instead (`QuestFrame.lua:92-93`).
    s.run("QuestFrameCompleteQuestButton:Click()").unwrap();
    assert!(
        s.take_quest_actions().is_empty(),
        "must pick a choice first"
    );

    s.run("QuestInfoItem2:Click()").unwrap();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(
        s.eval::<bool>("return QuestInfoItemHighlight:IsShown()")
            .unwrap(),
        "picking a choice row shows the highlight"
    );

    s.run("QuestFrameCompleteQuestButton:Click()").unwrap();
    assert_eq!(
        s.take_quest_actions(),
        vec![benilla_ui::script::QuestAction::Reward(1)],
        "row 2 (1-based) -> wire index 1 (0-based)"
    );
}

/// Goodbye only hides the window, with no `DeclineQuest()` (`QuestFrame.xml:748-752`).
#[test]
fn greeting_goodbye_button_closes_the_window() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");

    s.set_quest(Some(QuestState {
        panel: QuestPanel::Greeting,
        greeting: "Hello".into(),
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Deputy Willem".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_GREETING", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(s.eval::<bool>("return QuestFrame:IsVisible()").unwrap());

    s.run("QuestFrameGreetingGoodbyeButton:Click()").unwrap();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(
        !s.eval::<bool>("return QuestFrame:IsVisible()").unwrap(),
        "Goodbye closes the questgiver window"
    );
}

/// Accept and Decline keep real on-window rects through the instant-text arm's one frame of
/// disabled Accept; the test plants `QUEST_FADING_DISABLE = "1"`, the default being `"0"`.
#[test]
fn detail_panel_action_buttons_resolve_to_real_onscreen_rects() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");
    s.eval::<()>(r#"QUEST_FADING_DISABLE = "1""#).unwrap();
    s.set_quest(Some(QuestState {
        panel: QuestPanel::Detail,
        title: "A Threat Within".into(),
        body: "Kill kobolds.".into(),
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Deputy Willem".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_DETAIL", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(s
        .eval::<bool>("return QuestFrameAcceptButton:IsShown()")
        .unwrap());
    assert!(s
        .eval::<bool>("return QuestFrameDeclineButton:IsShown()")
        .unwrap());

    s.resolve();
    let button_art = |s: &mut UiScript| {
        let mut v = Vec::new();
        for q in s.extract() {
            if let benilla_ui::script::QuadContent::Texture { path: Some(p), .. } = &q.content {
                if p.starts_with("Interface\\Buttons\\UI-Panel-Button-") {
                    v.push((
                        p.clone(),
                        q.rect.expect("button quad carries a resolved rect"),
                    ));
                }
            }
        }
        v
    };

    // `OnShow` still disables Accept and zeroes the block; the reveal just starts at 1024
    // (`QuestInfo.lua:482-491`).
    let writing = button_art(&mut s);
    assert_eq!(
        writing.len(),
        2,
        "Accept + Decline both extract a real rect (not dropped for being unshown)"
    );
    assert!(
        writing
            .iter()
            .any(|(p, _)| p.ends_with("UI-Panel-Button-Disabled")),
        "Accept draws the Disabled art inside the one-frame arm window"
    );
    for (_, r) in &writing {
        assert!(
            r.top <= 512.0 && r.bottom >= 0.0 && r.left >= 0.0 && r.right <= 384.0,
            "button {r:?} must land inside the 384x512 window, not off-screen/at the origin"
        );
    }

    // The first `OnUpdate` scratches once, snaps the block opaque and enables Accept
    // (`QuestInfo.lua:3-16`).
    let scratches = |sounds: Vec<SoundRequest>| {
        sounds
            .iter()
            .filter(|r| **r == SoundRequest::KitName("WriteQuest".into()))
            .count()
    };
    s.tick(0.05);
    assert!(s.errors().is_empty(), "tick errors: {:?}", s.errors());
    assert_eq!(
        scratches(s.take_sounds()),
        1,
        "instant text keeps the writing sound: one scratch on the wake tick"
    );
    assert!(s
        .eval::<bool>("return QuestFrameAcceptButton:IsEnabled() ~= 0")
        .unwrap());
    assert_eq!(
        s.eval::<f32>("return QuestInfoFadingFrame:GetAlpha()")
            .unwrap(),
        1.0,
        "the block snaps straight to opaque — no QUESTINFO_FADE_IN ramp in instant mode"
    );
    s.tick(0.5);
    assert_eq!(
        scratches(s.take_sounds()),
        0,
        "the quill stops after the single instant-mode scratch"
    );
    s.resolve();
    let written = button_art(&mut s);
    assert!(
        written
            .iter()
            .all(|(p, _)| p.ends_with("UI-Panel-Button-Up")),
        "both buttons draw live art once the text has written on: {written:?}"
    );
}

/// With the default `QUEST_FADING_DISABLE = "0"` the text writes on at 70 chars/s, scratching each
/// tick with Accept disabled, then the block fades in (`QuestFrame.lua:5-6`, `QuestInfo.lua:3-16`).
#[test]
fn write_on_still_fades_when_instant_text_is_off() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");
    s.eval::<()>(r#"QUEST_FADING_DISABLE = "0""#).unwrap();
    // Let the tracking icon's load-time shine finish first: the stock walk skips the fade that
    // slides into a finished one's slot (`UIParent.lua:2456-2491`).
    for _ in 0..3 {
        s.tick(1.0);
    }
    s.set_quest(Some(QuestState {
        panel: QuestPanel::Detail,
        title: "A Threat Within".into(),
        body: "Kill kobolds.".into(),
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Deputy Willem".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_DETAIL", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());

    // Mid-write: the reveal is at char 7 of 13.
    s.tick(0.1);
    assert!(
        s.take_sounds()
            .iter()
            .any(|r| *r == SoundRequest::KitName("WriteQuest".into())),
        "the write-on scratches the WriteQuest quill each tick"
    );
    assert!(!s
        .eval::<bool>("return QuestFrameAcceptButton:IsEnabled() ~= 0")
        .unwrap());

    // Half a second more ends the 13-char gradient: Accept wakes and the block is mid-fade.
    s.tick(0.5);
    assert!(s.errors().is_empty(), "tick errors: {:?}", s.errors());
    assert!(s
        .eval::<bool>("return QuestFrameAcceptButton:IsEnabled() ~= 0")
        .unwrap());
    let alpha = s
        .eval::<f32>("return QuestInfoFadingFrame:GetAlpha()")
        .unwrap();
    assert!(
        alpha < 1.0,
        "fading mode ramps the block in (got alpha {alpha})"
    );
    s.tick(1.1);
    assert_eq!(
        s.eval::<f32>("return QuestInfoFadingFrame:GetAlpha()")
            .unwrap(),
        1.0,
        "the fade completes at opaque"
    );
}

/// Every quest event but `QUEST_FINISHED`, `QUEST_ITEM_UPDATE` over a shown window included,
/// re-reads `UnitName("questnpc")` into the title bar (`QuestFrame.lua:26`, `:62-63`).
#[test]
fn npc_name_reaches_the_title_bar_on_open_and_on_refresh() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");

    s.set_quest(Some(QuestState {
        panel: QuestPanel::Detail,
        title: "A Threat Within".into(),
        body: "Kill things.".into(),
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Marshal McBride".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_DETAIL", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert_eq!(
        s.eval::<String>("return getglobal('QuestFrameNpcNameText'):GetText() or ''")
            .unwrap(),
        "Marshal McBride",
        "the panel-open event puts the quest NPC's name in the title bar"
    );
    s.resolve();
    let name_quad = s
        .extract()
        .into_iter()
        .find(|q| {
            matches!(&q.content,
                benilla_ui::script::QuadContent::Text { text: Some(t), .. } if t == "Marshal McBride")
        })
        .expect("the NPC-name FontString extracts a quad");
    assert!(
        name_quad.rect.is_some(),
        "the NPC-name quad carries a resolved rect (got None — under-constrained)"
    );

    // A cleared name comes back on the refresh.
    s.run("getglobal('QuestFrameNpcNameText'):SetText('')")
        .unwrap();
    s.fire_event(
        "QUEST_ITEM_UPDATE",
        vec![ScriptValue::Str("Marshal McBride".into())],
    );
    assert_eq!(
        s.eval::<String>("return getglobal('QuestFrameNpcNameText'):GetText() or ''")
            .unwrap(),
        "Marshal McBride",
        "the QUEST_ITEM_UPDATE refresh also carries the name"
    );
}

/// A title that wraps at the row label's 275px (`QuestFrameTemplates.xml:202`) grows its row, or
/// the rows below overlap it.
#[test]
fn greeting_panel_title_rows_grow_to_their_wrapped_titles() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // Stock sizes each row right after `SetText`, `GetTextHeight() + 2` (`QuestFrame.lua:239`), so
    // the measure must be synchronous, as the app's is.
    s.set_text_measurer(Box::new(super::FixedWidthFont(7.0)));
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIParent.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, r"Interface\FrameXML\StaticPopup.xml");
    load_xml(&mut s, "Interface\\FrameXML\\ItemButtonTemplate.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");

    s.set_quest(Some(QuestState {
        panel: QuestPanel::Greeting,
        greeting: "What can I do for you?".into(),
        active_titles: vec![
            "Deliver Thomas' Report to Marshal Dughan in Goldshire before the Defias move again"
                .into(),
            "Investigate the Echo Ridge Mine and report back to Marshal McBride at once".into(),
        ],
        ..QuestState::default()
    }));
    s.set_unit(
        "npc",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Deputy Willem".into()),
            ..Default::default()
        }),
    );
    s.fire_event("QUEST_GREETING", vec![]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());

    let answer_measures = |s: &mut UiScript| {
        let answers: Vec<(u32, f32, f32, u64)> = s
            .fontstrings_needing_measure()
            .into_iter()
            .map(|r| {
                let ink = r.text.chars().count() as f32 * 6.0;
                match r.wrap_width {
                    Some(w) => (r.id, ink.min(w), (ink / w).ceil().max(1.0) * 14.0, r.key),
                    None => (r.id, ink, 14.0, r.key),
                }
            })
            .collect();
        s.set_measured_text_unwrapped(&answers);
    };
    answer_measures(&mut s);
    s.resolve();
    s.tick(0.016);
    answer_measures(&mut s);
    s.resolve();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());

    let row = |i: u32| -> (f32, f32, f32) {
        s.eval::<(f32, f32, f32)>(&format!(
            "local b = getglobal('QuestTitleButton{i}')\n\
             return b:GetTop(), b:GetBottom(), b:GetFontString():GetHeight()"
        ))
        .unwrap()
    };
    let (t1, b1, h1) = row(1);
    let (t2, b2, h2) = row(2);
    assert!(h1 >= 24.0 && h2 >= 24.0, "both titles wrap: {h1}, {h2}");
    assert!(
        (t1 - b1 - (h1 + 2.0)).abs() < 0.5,
        "row 1 is its wrapped title + 2: got {}, title {h1}",
        t1 - b1
    );
    assert!(
        (t2 - b2 - (h2 + 2.0)).abs() < 0.5,
        "row 2 is its wrapped title + 2: got {}, title {h2}",
        t2 - b2
    );
    assert!(
        t2 <= b1 + 0.5,
        "row 2 starts at/below row 1's bottom: row1 bottom {b1}, row2 top {t2}"
    );
}
