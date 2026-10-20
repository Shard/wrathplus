use super::*;
use crate::net::wrath_quests_completed_capture::tests::dispatch;
use benilla_protocol::wrath_events::WrathEventAdapter;
use benilla_protocol::SessionEvent;

/// The VM's queries go out one `CMSG 0x500` each; a reply replaces the list and fires
/// `QUEST_QUERY_COMPLETE` once; a new VM reads the stored list without an event.
#[test]
fn the_feed_sends_queries_and_applies_each_reply_in_order() {
    use benilla_ui::script::UiScript;
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.add_plugins(bevy::state::app::StatesPlugin)
        .insert_state(crate::char_select::ClientState::InWorld)
        .insert_resource(NetCommands(tx));
    crate::net::wrath_quests_completed_capture::register(&mut app);
    register(&mut app);
    let new_vm = || {
        let s = UiScript::new().unwrap();
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
