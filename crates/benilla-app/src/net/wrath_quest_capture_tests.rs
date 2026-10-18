use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeQuestPackets>()
        .0
        .iter()
        .cloned()
        .collect()
}

#[test]
fn one_owner_keeps_ordered_duplicates_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathQuest];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_quest_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    let empty = Packet::PoiResponse { quests: Vec::new() };
    for event in [
        SessionEvent::Connected {
            self_guid: 1,
            name: "test".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        },
        SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, 0x1e4, &[0, 0, 0, 0]);
        dispatch(&mut app, &mut adapter, 0x1e4, &[0, 0]);
        dispatch(&mut app, &mut adapter, 0x1e4, &[0, 0, 0, 0, 0]);
        dispatch(&mut app, &mut adapter, 0x1e4, &[0, 0, 0, 0]);
        assert_eq!(captured(&app), vec![empty.clone(), empty.clone()]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}

#[test]
fn a_pvp_kill_tick_reasks_the_givers_and_is_not_captured() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let mut adapter = WrathEventAdapter::new();
    let before = app.world().resource::<crate::ui_quest::QuestGiver>().reask_epoch();
    let mut body = 6u32.to_le_bytes().to_vec();
    body.extend(3u32.to_le_bytes());
    body.extend(10u32.to_le_bytes());
    dispatch(&mut app, &mut adapter, 0x46f, &body);
    let after = app.world().resource::<crate::ui_quest::QuestGiver>().reask_epoch();
    assert_eq!(after, before.wrapping_add(1));
    assert!(captured(&app).is_empty());
}
