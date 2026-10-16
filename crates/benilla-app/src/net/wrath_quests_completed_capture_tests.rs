use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x501, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn ordered_replies_keep_empty_duplicates_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let body = [2, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255];
    dispatch(&mut app, &mut adapter, &body);
    dispatch(&mut app, &mut adapter, &[0; 4]);
    dispatch(&mut app, &mut adapter, &body);
    for end in 0..body.len() {
        dispatch(&mut app, &mut adapter, &body[..end]);
    }
    let mut trailing = body.to_vec();
    trailing.push(0);
    dispatch(&mut app, &mut adapter, &trailing);
    assert_eq!(
        app.world()
            .resource::<NativeQuestsCompletedPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            Packet {
                quest_ids: vec![u32::MAX, u32::MAX]
            },
            Packet { quest_ids: vec![] },
            Packet {
                quest_ids: vec![u32::MAX, u32::MAX]
            },
        ]
    );
}

#[test]
fn one_owner_and_both_session_boundaries_clear_captured_replies() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathQuestsCompleted];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_quests_completed_capture::receive"));
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
            end: benilla_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, &[0; 4]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(app
            .world()
            .resource::<NativeQuestsCompletedPackets>()
            .0
            .is_empty());
    }
}

/// The VM's queries go out one `CMSG 0x500` each; a reply replaces the list and fires
/// `QUEST_QUERY_COMPLETE` once; a new VM reads the stored list without an event.
#[test]
fn the_feed_sends_queries_and_applies_each_reply_in_order() {
    use benilla_ui::script::{ScriptDialect, UiScript};
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.add_plugins(bevy::state::app::StatesPlugin)
        .insert_state(crate::char_select::ClientState::InWorld)
        .insert_resource(NetCommands(tx));
    register(&mut app);
    let new_vm = || {
        let s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.run("N = 0 local f = CreateFrame('Frame') f:RegisterEvent('QUEST_QUERY_COMPLETE') f:SetScript('OnEvent', function() N = N + 1 end)").unwrap();
        s
    };
    app.insert_non_send_resource(new_vm());
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run("QueryQuestsCompleted() QueryQuestsCompleted()")
        .unwrap();
    app.update();
    let sent: Vec<_> = rx.try_iter().collect();
    assert_eq!(sent.len(), 2);
    assert!(sent
        .iter()
        .all(|c| matches!(c, ClientCommand::NativeQuestsCompletedQuery)));

    let mut adapter = WrathEventAdapter::new();
    dispatch(
        &mut app,
        &mut adapter,
        &[2, 0, 0, 0, 7, 0, 0, 0, 9, 0, 0, 0],
    );
    dispatch(&mut app, &mut adapter, &[1, 0, 0, 0, 4, 0, 0, 0]);
    app.update();
    let script = app.world().non_send_resource::<UiScript>();
    assert_eq!(
        script.eval::<i64>("return N").unwrap(),
        2,
        "one event per reply"
    );
    assert!(script
        .eval::<bool>("local t = GetQuestsCompleted() return t[4] == true and t[7] == nil")
        .unwrap());

    // A rebuilt VM: the stored list, no event.
    app.insert_non_send_resource(new_vm());
    app.update();
    let script = app.world().non_send_resource::<UiScript>();
    assert_eq!(script.eval::<i64>("return N").unwrap(), 0);
    assert!(script
        .eval::<bool>("return GetQuestsCompleted()[4] == true")
        .unwrap());

    // A session boundary empties the list in the live VM and for the next one.
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: String::new(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    app.update();
    let script = app.world().non_send_resource::<UiScript>();
    assert!(script
        .eval::<bool>("return next(GetQuestsCompleted()) == nil")
        .unwrap());
    app.insert_non_send_resource(new_vm());
    app.update();
    let script = app.world().non_send_resource::<UiScript>();
    assert!(script
        .eval::<bool>("return next(GetQuestsCompleted()) == nil")
        .unwrap());
}
