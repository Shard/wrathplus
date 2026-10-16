use super::*;
use benilla_protocol::wrath_char_services::Reply;

#[test]
fn one_owner_keeps_ordered_replies_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathCharServiceResult];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_char_service_capture::receive"));
    let refused = Reply::Rename { result: 0x31, renamed: None };
    for end in [
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
        for _ in 0..2 {
            crate::net::handlers::dispatch(
                app.world_mut(),
                vec![SessionEvent::WrathCharServiceResult { reply: refused.clone() }],
            );
        }
        assert_eq!(
            app.world().resource::<NativeCharServiceReplies>().0.iter().cloned().collect::<Vec<_>>(),
            vec![refused.clone(), refused.clone()]
        );
        crate::net::handlers::dispatch(app.world_mut(), vec![end]);
        assert!(app.world().resource::<NativeCharServiceReplies>().0.is_empty());
    }
}
