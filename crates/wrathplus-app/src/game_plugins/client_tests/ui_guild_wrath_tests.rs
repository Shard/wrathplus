//! Whole-client tests of `crate::ui_guild::wrath`, from `ui_guild/wrath_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use wrathplus_protocol::SessionEventKind;

#[test]
fn built_client_has_one_native_owner() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathGuild];
    assert_eq!(owners.len(), 1);
    assert!(owners[0].contains("ui_guild::wrath::receive"));
    assert!(census[&SessionEventKind::GuildRoster][0].contains("guild::net::on_roster"));
}
