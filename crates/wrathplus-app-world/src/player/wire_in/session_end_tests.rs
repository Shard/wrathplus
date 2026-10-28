use super::*;
use crate::app_core::session::DisconnectedMessage;
use crate::net::LoggedOutMessage;
use wrathplus_protocol::SessionEnd;

fn harness() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Player>()
        .add_message::<LoggedOutMessage>()
        .add_message::<DisconnectedMessage>()
        .add_systems(Update, release_on_session_end);
    app
}

/// A session that ran: in world, moving, every mode granted, the reins in other hands.
fn a_session_that_ran(app: &mut App) {
    let mut p = app.world_mut().resource_mut::<Player>();
    p.active = true;
    p.modes = super::super::state::MoveModes {
        rooted: true,
        water_walking: true,
        feather_fall: true,
        hover: true,
        levitating: true,
        can_fly: true,
    };
    p.flying = true;
    p.autorun = true;
    p.control_lost = true;
    p.foreign_mover = Some(0xBB);
    p.server_riding = true;
    p.swimming = true;
    p.move_flags = crate::app_core::movement::move_flags::FORWARD;
    p.pos = Vec3::new(10.0, 20.0, 30.0);
    p.owes_worldport_ack = true;
    p.airborne_since = Some(4.0);
    p.wedged = true;
}

/// `/logout` roots us (`MiscHandler.cpp:329`) and the next login is never unrooted:
/// `SendInitialPacketsAfterAddToMap` re-roots only for a stun (`Player.cpp:19297-19311`).
/// Checked against `Player::default()` whole, so a new field is covered.
#[test]
fn a_logout_takes_every_grant_the_ended_session_made() {
    let mut app = harness();
    a_session_that_ran(&mut app);
    app.world_mut().write_message(LoggedOutMessage);
    app.update();

    let p = app.world().resource::<Player>();
    assert!(
        p.modes == Default::default(),
        "the granted modes belonged to the mover that just ended — a root that survives \
             `/logout` means the character re-enters the world and WASD is dead"
    );
    assert!(
        *p == Player::default(),
        "and nothing else survives either: the session boundary returns the resource to the \
             state `player::setup` inserts at boot"
    );
}

/// A lost session is a different message on a different reader; the `|` draining both matters.
#[test]
fn a_lost_session_takes_them_too() {
    let mut app = harness();
    a_session_that_ran(&mut app);
    app.world_mut().write_message(DisconnectedMessage {
        reason: "world stream closed".into(),
        end: SessionEnd::Lost,
        session_over: true,
    });
    app.update();

    assert!(*app.world().resource::<Player>() == Player::default());
}

/// `session_over: false` (the logout's own teardown, a seamless reconnect) keeps the body ours.
#[test]
fn a_teardown_that_is_not_the_end_keeps_the_avatar() {
    let mut app = harness();
    a_session_that_ran(&mut app);
    app.world_mut().write_message(DisconnectedMessage {
        reason: "logged out".into(),
        end: SessionEnd::LoggedOut,
        session_over: false,
    });
    app.update();

    let p = app.world().resource::<Player>();
    assert!(
        p.active,
        "the session is not over — the avatar is still ours"
    );
    assert!(p.modes.rooted, "and so is everything granted to its mover");
}
