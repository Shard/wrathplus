//! Whole-client tests of `crate::net::wrath_mail_capture`, from `net/wrath_mail_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use benilla_protocol::SessionEventKind;

#[test]
fn built_client_has_one_capture_owner_and_connected_reset() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathMailMutation];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_mail_capture::receive"));
    assert!(census[&SessionEventKind::Connected]
        .iter()
        .any(|name| name.contains("wrath_mail_capture::clear")));
    assert!(census[&SessionEventKind::Disconnected]
        .iter()
        .any(|name| name.contains("wrath_mail_capture::clear")));
}
