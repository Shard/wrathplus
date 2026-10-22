//! Whole-client tests of `crate::ui_native::wrath_quest_pois`, from `ui_native/wrath_quest_pois_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::ui_native::wrath_quest_pois::tests::{dispatch, one_quest_body};
use crate::ui_native::wrath_quest_pois::*;
use wrathplus_protocol::wrath_events::WrathEventAdapter;
use wrathplus_protocol::SessionEvent;
use wrathplus_protocol::SessionEventKind;

#[test]
fn one_owner_and_both_boundaries_forget_the_pois() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathQuest];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_quest_pois::receive"));
    let mut adapter = WrathEventAdapter::new();
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
            end: wrathplus_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, 0x1e4, &one_quest_body(6));
        dispatch(&mut app, &mut adapter, 0x1e4, &[1, 0]); // short: refused
        let pois = app.world().resource::<QuestPois>();
        assert_eq!(pois.view(&[6]).len(), 1);
        assert!(pois.fresh);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        let pois = app.world().resource::<QuestPois>();
        assert!(pois.view(&[6]).is_empty() && !pois.fresh);
    }
}

#[test]
fn a_pvp_kill_tick_reasks_the_givers_and_leaves_the_pois_alone() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let mut adapter = WrathEventAdapter::new();
    let before = app
        .world()
        .resource::<crate::ui_quest::QuestGiver>()
        .reask_epoch();
    let mut body = 6u32.to_le_bytes().to_vec();
    body.extend(3u32.to_le_bytes());
    body.extend(10u32.to_le_bytes());
    dispatch(&mut app, &mut adapter, 0x46f, &body);
    let after = app
        .world()
        .resource::<crate::ui_quest::QuestGiver>()
        .reask_epoch();
    assert_eq!(after, before.wrapping_add(1));
    assert!(!app.world().resource::<QuestPois>().fresh);
}
