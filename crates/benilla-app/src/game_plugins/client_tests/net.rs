//! Whole-client tests of `crate::net`, from `net.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::*;
use benilla_protocol::SessionEvent;
use bevy::prelude::*;

fn connected() -> SessionEvent {
    SessionEvent::Connected {
        self_guid: 0x1234,
        name: "New".into(),
        billing_time_rested: 0,
        tutorial_flags: None,
        addon_info: None,
    }
}

fn old_pose_move(pos: Vec3) -> ClientCommand {
    ClientCommand::Move {
        kind: MoveKind::Heartbeat,
        flags: 0,
        pos: [pos.x, pos.y, pos.z],
        orientation: 0.0,
        pitch: 0.0,
        fall_time: 0,
        jump: None,
        transport: None,
    }
}

#[test]
fn wrath_connected_clears_old_pose_before_publishing_the_new_endpoint() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let (wire_tx, wire_rx) = crossbeam_channel::unbounded();
    let old = SessionCommandSender::new(wire_tx.clone(), 7);
    let old_clone = old.clone();
    let new = SessionCommandSender::new(wire_tx, 8);
    let (endpoint_tx, endpoint_rx) = crossbeam_channel::unbounded();
    endpoint_tx.send(new).unwrap();
    app.insert_resource(NetCommands::session_stamped(old))
        .insert_resource(CommandEndpoints(endpoint_rx))
        .init_resource::<crate::player::Player>();
    {
        let mut player = app.world_mut().resource_mut::<crate::player::Player>();
        player.active = true;
        player.pos = Vec3::new(11.0, 22.0, 33.0);
    }

    // The prior session edge and the new connection are drained in one ECS frame.
    handlers::dispatch(app.world_mut(), vec![SessionEvent::LoggedOut, connected()]);

    let player = app.world().resource::<crate::player::Player>();
    assert!(*player == crate::player::Player::default());
    // Even a producer retaining the old clone remains stamped old. The normal movement gate
    // sees inactive and therefore cannot copy the old pose through the newly active endpoint.
    old_clone
        .send(old_pose_move(Vec3::new(11.0, 22.0, 33.0)))
        .unwrap();
    if player.active {
        app.world()
            .resource::<NetCommands>()
            .0
            .send(old_pose_move(player.pos))
            .unwrap();
    }
    app.world()
        .resource::<NetCommands>()
        .0
        .send(ClientCommand::Logout)
        .unwrap();
    let queued: Vec<_> = wire_rx.try_iter().collect();
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[0].epoch, 7);
    assert!(matches!(queued[0].command, ClientCommand::Move { .. }));
    assert_eq!(queued[1].epoch, 8);
    assert!(matches!(queued[1].command, ClientCommand::Logout));
}

#[test]
fn raw_offline_connected_neither_waits_for_a_sidechannel_nor_resets_vanilla_player() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let (raw_tx, _raw_rx) = crossbeam_channel::unbounded();
    let (_endpoint_tx, endpoint_rx) = crossbeam_channel::unbounded();
    app.insert_resource(NetCommands(raw_tx))
        .insert_resource(CommandEndpoints(endpoint_rx))
        .init_resource::<crate::player::Player>();
    app.world_mut()
        .resource_mut::<crate::player::Player>()
        .active = true;

    handlers::dispatch(app.world_mut(), vec![connected()]);

    assert!(app.world().resource::<crate::player::Player>().active);
    assert!(matches!(
        app.world().resource::<NetCommands>().0,
        CommandSender::Raw(_)
    ));
}
