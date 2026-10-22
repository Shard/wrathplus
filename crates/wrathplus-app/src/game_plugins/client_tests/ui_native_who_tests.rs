//! Whole-client tests of `crate::net::wrath_who_capture`, from `net/wrath_who_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use wrathplus_protocol::SessionEventKind;

#[test]
fn full_client_has_one_native_owner_separate_from_shared_who() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    assert_eq!(census[&SessionEventKind::WrathWho].len(), 1);
    assert!(census[&SessionEventKind::WrathWho][0].contains("wrath_who_capture::receive"));
    assert!(!census[&SessionEventKind::WhoResults]
        .iter()
        .any(|s| s.contains("wrath_who_capture")));
}
