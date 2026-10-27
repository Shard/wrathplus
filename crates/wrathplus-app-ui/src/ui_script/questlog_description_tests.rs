//! The quest log's description (playtest, quest 806): a template that lands after the log row
//! fills `GetQuestLogQuestText()`'s first return, and the stock detail pane shows it. Stock
//! `QuestInfo_ShowDescriptionText` sets the text, then `SetAlphaGradient(0, 0)`
//! (`QuestInfo.lua:106-107`); 12340 fades nothing at length 0 (`0x6c78f0`), which the text
//! layout's gradient test covers, so here the pane carries the text and that pair. The
//! completion text a zero-objective quest's tracker line reads falls back to the objectives.

use wrathplus_protocol::types::{QuestObjective, QuestTemplate, WrathQuestInfo};
use wrathplus_ui::script::{QuadContent, QuestLogEntryView, QuestLogState, UiScript};

use super::test_ui::load_ui;

const DETAILS: &str = "We cannot allow the Burning Blade a foothold in Durotar, $N! Go, $c.";
const SHOWN: &str =
    "We cannot allow the Burning Blade a foothold in Durotar, Locktest! Go, warlock.";
const OBJECTIVES: &str = "Bring Fizzle's Claw to Orgnil Soulscar in Razor Hill.";

/// Quest 806's text, as `SMSG_QUEST_QUERY_RESPONSE` carries it: macros unexpanded.
fn template() -> QuestTemplate {
    QuestTemplate {
        wrath: Some(WrathQuestInfo::default()),
        quest_id: 806,
        method: 2,
        level: 12,
        zone_or_sort: 14,
        quest_type: 0,
        rep_objective_faction: 0,
        rep_objective_value: 0,
        next_quest_in_chain: 0,
        money: 0,
        money_max_level: 0,
        reward_spell: 0,
        src_item_id: 0,
        flags: 0,
        rewards: [(0, 0); 4],
        choices: [(0, 0); 6],
        point_map_id: 0,
        point_x: 0.0,
        point_y: 0.0,
        point_opt: 0,
        title: "Dark Storms".into(),
        objectives_text: OBJECTIVES.into(),
        details: DETAILS.into(),
        end_text: String::new(),
        objectives: std::array::from_fn(|_| QuestObjective {
            creature_or_go: 0,
            required_count: 0,
            item_id: 0,
            item_count: 0,
            text: String::new(),
        }),
    }
}

/// The log row the template builds, as `ui_quest_log`'s publish does for a cached quest.
fn landed_log() -> QuestLogState {
    let subject = crate::npc_text::Subject {
        name: "Locktest".into(),
        race: 2,
        class: 9,
        gender: 1,
    };
    let states = crate::app_core::state::WorldStates::default();
    let macros = crate::npc_text::MacroContext {
        subject: Some(&subject),
        states: &states,
    };
    let (tx, _rx) = crossbeam_channel::unbounded();
    let commands = crate::net::NetCommands(tx);
    let items = crate::items::Items::default();
    let detail =
        crate::ui_quest_log::build_detail(&template(), &items, None, &commands, &macros, None);
    QuestLogState {
        num_quests: 1,
        entries: vec![QuestLogEntryView {
            quest_id: 806,
            title: "Dark Storms".into(),
            level: 12,
            detail: Some(detail),
            ..Default::default()
        }],
        hidden_quest_ids: Vec::new(),
    }
}

/// The description's drawn text quad and its `SetAlphaGradient` pair.
fn drawn_description(s: &mut UiScript) -> Option<(String, Option<(f32, f32)>)> {
    s.resolve();
    s.extract().iter().find_map(|q| match &q.content {
        QuadContent::Text {
            text: Some(t),
            alpha_gradient,
            ..
        } if t == SHOWN => Some((t.clone(), *alpha_gradient)),
        _ => None,
    })
}

#[test]
fn a_template_landing_after_the_row_fills_the_log_and_map_descriptions() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_ui(&mut s, "Interface\\FrameXML\\QuestLogFrame.xml");
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    // The row's template is in flight: the log lists nothing yet (`0x4de510`).
    s.set_quest_log(QuestLogState::default());
    s.fire_event("QUEST_LOG_UPDATE", vec![]);
    s.run("ToggleFrame(QuestLogFrame)").unwrap();
    assert_eq!(s.eval::<i64>("return GetNumQuestLogEntries()").unwrap(), 0);

    // The template lands: the publish lists the quest and announces it.
    s.set_quest_log(landed_log());
    s.fire_event("QUEST_LOG_UPDATE", vec![]);
    s.run("QuestLog_SetSelection(1)").unwrap();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    let (desc, obj) = s
        .eval::<(String, String)>("return GetQuestLogQuestText()")
        .unwrap();
    assert_eq!((desc.as_str(), obj.as_str()), (SHOWN, OBJECTIVES));
    // No end text: `0x5e06d0` answers the objectives text, the WatchFrame line a zero-objective
    // quest shows (`WatchFrame.lua:834`, by row), not a bare dash.
    for call in [
        "GetQuestLogCompletionText()",
        "GetQuestLogCompletionText(1)",
    ] {
        assert_eq!(
            s.eval::<String>(&format!("return {call}")).unwrap(),
            OBJECTIVES,
            "{call}"
        );
    }
    let log_pane = drawn_description(&mut s);
    assert_eq!(
        log_pane,
        Some((SHOWN.to_string(), Some((0.0, 0.0)))),
        "the log pane"
    );

    // The enlarged map's detail pane (`WorldMapFrame.lua:1674-1675`) reparents the same text.
    s.run("SelectQuestLogEntry(1); QuestInfo_Display(QUEST_TEMPLATE_MAP1, WorldMapQuestDetailScrollChildFrame)")
        .unwrap();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert_eq!(
        s.eval::<String>("return QuestInfoDescriptionText:GetParent():GetName()")
            .unwrap(),
        "WorldMapQuestDetailScrollChildFrame"
    );
    assert_eq!(
        s.eval::<String>("return QuestInfoDescriptionText:GetText()")
            .unwrap(),
        SHOWN,
        "the map pane"
    );
}

/// `0x5e06d0`'s choice: the end text when it has one, else the objectives text, and the
/// objectives text whenever the template sets `QUEST_FLAGS_OBJ_TEXT` (0x40000).
#[test]
fn the_completion_text_falls_back_to_the_objectives() {
    let states = crate::app_core::state::WorldStates::default();
    let ctx = crate::npc_text::MacroContext {
        subject: None,
        states: &states,
    };
    let mut t = template();
    assert_eq!(crate::npc_text::completion_text(&t, &ctx), OBJECTIVES);
    t.end_text = "Return to Orgnil Soulscar in Razor Hill.".into();
    assert_eq!(
        crate::npc_text::completion_text(&t, &ctx),
        "Return to Orgnil Soulscar in Razor Hill."
    );
    t.flags |= 0x0004_0000;
    assert_eq!(crate::npc_text::completion_text(&t, &ctx), OBJECTIVES);
}
