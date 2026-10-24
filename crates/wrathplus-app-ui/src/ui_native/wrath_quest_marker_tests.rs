use super::*;
use wrathplus_protocol::types::{QuestObjective, QuestTemplate};
use wrathplus_ui::script::{QuestLogEntryView, QuestLogState};

fn template(quest_id: u32, x: f32, y: f32, opt: u32, title: &str) -> QuestTemplate {
    QuestTemplate {
        wrath: None,
        quest_id,
        method: 0,
        level: 2,
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
        point_map_id: 1,
        point_x: x,
        point_y: y,
        point_opt: opt,
        title: title.into(),
        objectives_text: String::new(),
        details: String::new(),
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

/// Selecting a quest points slot 0 at its record; selecting its header clears the slot.
#[test]
fn the_selection_points_slot_zero_at_the_quests_record() {
    let mut app = App::new();
    let (tx, _rx) = crossbeam_channel::unbounded();
    app.insert_resource(crate::net::NetCommands(tx));
    app.init_resource::<PoiMarker>();
    app.insert_resource(wrathplus_world::world_map::CurrentMap(1));
    let mut log = QuestLog::default();
    log.insert_template(template(792, -600.0, -4200.0, 0, "Vile Familiars"));
    app.insert_resource(log);
    let mut script = UiScript::new().unwrap();
    script.set_quest_log(QuestLogState {
        entries: vec![
            QuestLogEntryView {
                title: "Durotar".into(),
                is_header: true,
                ..Default::default()
            },
            QuestLogEntryView {
                quest_id: 792,
                title: "Vile Familiars".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    script.set_quest_log_selection(2);
    app.insert_non_send_resource(script);
    app.add_systems(Update, sync_quest_marker);

    app.update();
    let marker = app.world().resource::<PoiMarker>();
    let quest = marker.markers_on_map(1).next().expect("slot 0");
    assert_eq!(quest.name, "Vile Familiars");
    assert_eq!((quest.pos[0], quest.pos[1]), (-600.0, -4200.0));

    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .set_quest_log_selection(1);
    app.update();
    assert!(
        app.world().resource::<PoiMarker>().quest.is_none(),
        "a header clears it"
    );
}
