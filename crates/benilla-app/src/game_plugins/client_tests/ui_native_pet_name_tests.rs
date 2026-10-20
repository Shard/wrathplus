//! Whole-client tests of `crate::net::wrath_pet_name_capture`, from
//! `net/wrath_pet_name_capture_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use benilla_protocol::SessionEventKind;

#[test]
fn full_client_has_exactly_one_native_pet_name_owner_separate_from_shared() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathPetName];
    assert_eq!(owners.len(), 1);
    assert!(owners[0].contains("wrath_pet_name_capture::receive"));
    assert!(!census[&SessionEventKind::PetName]
        .iter()
        .any(|owner| owner.contains("wrath_pet_name_capture")));
}
