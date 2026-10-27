//! Whole-client tests of `crate::ui_stable::wrath`, from `ui_stable/wrath_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use wrathplus_protocol::SessionEventKind;

#[test]
fn built_client_has_one_native_owner_separate_from_vanilla() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathStable];
    assert_eq!(owners.len(), 1);
    assert!(owners[0].contains("ui_stable::wrath::receive"));
    // The list also ends an open gossip menu (`ui_gossip::net::on_service_window`).
    assert!(census[&SessionEventKind::ListStabledPets]
        .iter()
        .any(|h| h.contains("ui_stable::net::on_list")));
    assert!(census[&SessionEventKind::StableResult][0].contains("ui_stable::net::on_result"));
}
