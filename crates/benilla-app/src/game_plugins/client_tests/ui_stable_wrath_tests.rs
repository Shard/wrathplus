//! Whole-client tests of `crate::ui_stable::wrath`, from `ui_stable/wrath_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use benilla_protocol::SessionEventKind;

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
    assert!(census[&SessionEventKind::ListStabledPets][0].contains("ui_stable::net::on_list"));
    assert!(census[&SessionEventKind::StableResult][0].contains("ui_stable::net::on_result"));
}
