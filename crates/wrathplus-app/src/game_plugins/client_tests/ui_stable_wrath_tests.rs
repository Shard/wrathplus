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

#[test]
fn built_client_native_list_reaches_stable_and_closes_gossip() {
    use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let npc = 17379391129526469420u64;
    app.world_mut()
        .resource_mut::<crate::app_core::interaction::GossipState>()
        .npc = Some(npc);
    let mut body = npc.to_le_bytes().to_vec();
    body.extend([1, 0]);
    for word in [8492u32, 1190, 13] {
        body.extend(word.to_le_bytes());
    }
    body.extend(b"Porkchop\0");
    body.push(1);
    let WrathDispatch::Handled { session_events, .. } =
        WrathEventAdapter::new().dispatch(0x26f, &body, 0).unwrap()
    else {
        panic!()
    };
    crate::net::handlers::dispatch(app.world_mut(), session_events);
    assert!(app
        .world()
        .resource::<crate::app_core::interaction::GossipState>()
        .npc
        .is_none());
    // StableOpen intentionally keeps its rows crate-private in app-ui. The public NpcSession
    // contract observes the actual registered native owner's session, not a capture queue.
    use crate::ui_session::NpcSession;
    assert_eq!(
        app.world().resource::<crate::ui_stable::StableOpen>().npc(),
        Some(npc)
    );
}
