//! Whole-client tests of `crate::net::wrath_mail_capture`, from `net/wrath_mail_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use wrathplus_protocol::SessionEventKind;

#[test]
fn built_client_has_one_capture_owner_and_connected_reset() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathMailMutation];
    assert_eq!(owners.len(), 2, "{owners:?}");
    assert_eq!(
        owners
            .iter()
            .filter(|name| name.contains("wrath_mail_capture::receive"))
            .count(),
        1
    );
    assert_eq!(
        owners
            .iter()
            .filter(|name| name.contains("ui_mail::net::on_show_mailbox"))
            .count(),
        1
    );
    assert!(census[&SessionEventKind::Connected]
        .iter()
        .any(|name| name.contains("wrath_mail_capture::clear")));
    assert!(census[&SessionEventKind::Disconnected]
        .iter()
        .any(|name| name.contains("wrath_mail_capture::clear")));
}
