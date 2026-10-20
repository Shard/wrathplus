//! Whole-client tests of `crate::net::wrath_social_lists_capture`, from `net/wrath_social_lists_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use benilla_protocol::SessionEventKind;

#[test]
fn built_client_has_one_owner_each() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    for kind in [
        SessionEventKind::WrathChannelMembers,
        SessionEventKind::WrathRaidBrowser,
    ] {
        assert_eq!(census[&kind].len(), 1, "{kind:?}");
        assert!(census[&kind][0].contains("wrath_social_lists_capture"));
    }
}
