//! Whole-client tests of `crate::ui_native::wrath_appearance`, from `ui_native/wrath_appearance_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use benilla_protocol::SessionEventKind;

#[test]
fn built_client_has_one_owner() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathAppearance];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_appearance::receive"));
}
