//! Whole-client tests of `crate::ui_native_chat`, from `ui_native_chat_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use benilla_protocol::SessionEventKind;

#[test]
fn full_client_has_exactly_one_native_chat_owner_separate_from_shared() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathChat];
    assert_eq!(owners.len(), 1);
    assert!(owners[0].contains("ui_native_chat::receive"));
    assert!(!census[&SessionEventKind::Chat]
        .iter()
        .any(|owner| owner.contains("ui_native_chat")));
}
