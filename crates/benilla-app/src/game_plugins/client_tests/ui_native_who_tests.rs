//! Whole-client tests of `crate::ui_native_who`, from `ui_native_who_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use benilla_protocol::SessionEventKind;

#[test]
fn full_client_has_one_native_owner_separate_from_shared_who() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    assert_eq!(census[&SessionEventKind::WrathWho].len(), 1);
    assert!(census[&SessionEventKind::WrathWho][0].contains("ui_native_who::receive"));
    assert!(!census[&SessionEventKind::WhoResults]
        .iter()
        .any(|s| s.contains("ui_native_who")));
}
