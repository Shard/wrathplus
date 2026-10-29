use std::time::Duration;

use super::*;

/// The tile under `Player::default()`: the `focus_tile` when focus and avatar agree.
fn own_tile() -> (i32, i32) {
    let wow = wrathplus_assets::coords::bevy_to_wow(Player::default().pos);
    let (tx, ty) = wrathplus_formats::world_to_tile(wow[0], wow[1]);
    (tx as i32, ty as i32)
}

/// The release system registered, so its `Local` baseline survives frames (`run_system_once`
/// would reset it), a hand-driven clock, and progress about the avatar's own tile.
fn app() -> App {
    let mut app = App::new();
    app.insert_resource(Time::<()>::default())
        .insert_resource(WorldLoadProgress {
            focus_tile: Some(own_tile()),
            ..WorldLoadProgress::default()
        })
        .insert_resource(Player {
            settling: true,
            settle_deadline: SETTLE_TIMEOUT,
            ..Player::default()
        })
        .add_systems(Update, release_post_snap_hold);
    app
}

/// One frame: advance the clock, mutate the streamer's published progress, run the release.
fn step(app: &mut App, dt: f32, tweak: impl FnOnce(&mut WorldLoadProgress)) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(dt));
    tweak(&mut app.world_mut().resource_mut::<WorldLoadProgress>());
    app.update();
}

fn settling(app: &mut App) -> bool {
    app.world().resource::<Player>().settling
}

#[test]
fn a_flying_cinematic_holds_the_body_and_marks_its_world_stale() {
    let mut app = App::new();
    app.insert_resource(ViewFocus::default())
        .insert_resource(Player {
            active: true,
            settling: false,
            world_stale: false,
            ..Player::default()
        })
        .insert_resource(crate::cinematic::Cinematic::playing_for_test())
        .add_systems(Update, publish_view_focus);
    app.update();

    let p = app.world().resource::<Player>();
    assert!(
        p.settling,
        "the body kept gravity while its ground unloaded"
    );
    assert!(
        p.world_stale,
        "the stall backstop was left free to expire mid-shot"
    );
}

/// A re-arm would flicker `settling` under the loading screen's clear gate and the zone-channel
/// walk, which both read it.
#[test]
fn the_hold_is_armed_on_the_edge_and_never_re_armed() {
    let mut app = App::new();
    app.insert_resource(ViewFocus::default())
        .insert_resource(Player {
            active: true,
            ..Player::default()
        })
        .insert_resource(crate::cinematic::Cinematic::playing_for_test())
        .add_systems(Update, publish_view_focus);
    app.update();
    assert!(app.world().resource::<Player>().settling);

    // The world ends the hold mid-cinematic, the camera's tile being the body's.
    app.world_mut().resource_mut::<Player>().settling = false;
    app.world_mut().resource_mut::<Player>().world_stale = false;
    app.update();
    let p = app.world().resource::<Player>();
    assert!(
        !p.settling,
        "the publish re-armed a hold the world had ended"
    );
    assert!(
        !p.world_stale,
        "and re-staled a world the streamer had cleared"
    );
}

#[test]
fn an_ordinary_frame_arms_nothing() {
    let mut app = App::new();
    app.insert_resource(ViewFocus::default())
        .insert_resource(Player {
            active: true,
            ..Player::default()
        })
        .insert_resource(crate::cinematic::Cinematic::default())
        .add_systems(Update, publish_view_focus);
    app.update();
    let p = app.world().resource::<Player>();
    assert!(!p.settling);
    assert!(!p.world_stale);
}

#[test]
fn a_slow_but_advancing_stream_never_times_out() {
    let mut app = app();
    app.world_mut().resource_mut::<Player>().world_stale = false;
    // Five budgets of wall clock, with a counter moving every frame.
    for i in 0..(5.0 * SETTLE_TIMEOUT) as usize {
        step(&mut app, 1.0, |p| {
            p.total = 2000;
            p.ready = i; // tiles/placements landing one at a time
            p.colliders_pending = 300 + i % 7;
            p.scene_ready = false;
        });
        assert!(
            settling(&mut app),
            "the hold gave up at ~{i}s with the stream still visibly advancing"
        );
    }
}

#[test]
fn a_genuinely_stalled_stream_still_times_out() {
    let mut app = app();
    app.world_mut().resource_mut::<Player>().world_stale = false;
    // Frozen counters: the first frame baselines the `Local` (a change), then the budget runs.
    for _ in 0..=(SETTLE_TIMEOUT + 2.0) as usize {
        step(&mut app, 1.0, |p| {
            p.total = 2000;
            p.ready = 500;
            p.colliders_pending = 300;
            p.scene_ready = false;
        });
    }
    assert!(
        !settling(&mut app),
        "a dead stream must not hold the body (and the screen) forever"
    );
}

#[test]
fn a_stale_world_pushes_the_deadline_before_the_stall_budget_starts() {
    let mut app = app();
    app.world_mut().resource_mut::<Player>().world_stale = true;
    // Two budgets of stale, frozen frames; `focus_resident` stays false, so stale stays set.
    for _ in 0..(2.0 * SETTLE_TIMEOUT) as usize {
        step(&mut app, 1.0, |p| {
            p.total = 0;
            p.scene_ready = false;
            p.focus_resident = false;
        });
        assert!(settling(&mut app), "released over the departed map's floor");
    }
    // The destination becomes resident, then stalls: the budget starts here.
    app.world_mut().resource_mut::<Player>().world_stale = false;
    for _ in 0..=(SETTLE_TIMEOUT + 2.0) as usize {
        step(&mut app, 1.0, |p| {
            p.total = 2000;
            p.ready = 500;
            p.colliders_pending = 300;
            p.scene_ready = false;
            p.focus_resident = true;
        });
    }
    assert!(
        !settling(&mut app),
        "the stall budget never started counting"
    );
}

#[test]
fn residency_releases_on_the_frame_it_lands() {
    let mut app = app();
    app.world_mut().resource_mut::<Player>().world_stale = false;
    step(&mut app, 1.0, |p| {
        p.total = 2000;
        p.ready = 1999;
        p.colliders_pending = 3;
        p.scene_ready = false;
    });
    assert!(settling(&mut app));
    step(&mut app, 0.1, |p| {
        p.ready = 2000;
        p.colliders_pending = 0;
        p.scene_ready = true;
    });
    assert!(!settling(&mut app), "presentable world, hold still on");
}

/// Spawned but unbaked geometry is a hole like an unattached collider, so it holds the settle.
#[test]
fn an_unbaked_region_holds_the_settle() {
    let mut app = app();
    app.world_mut().resource_mut::<Player>().world_stale = false;
    step(&mut app, 1.0, |p| {
        p.total = 2000;
        p.ready = 2000;
        p.colliders_pending = 0;
        p.scene_ready = true;
        p.gx_pending = 4;
    });
    assert!(settling(&mut app), "spawned but undrawn is not presentable");
    step(&mut app, 0.1, |p| p.gx_pending = 0);
    assert!(!settling(&mut app), "baked — the world is really there");
}

/// On a teleport's snap frame the focus can still describe the departure tile as resident.
#[test]
fn residency_about_another_tile_neither_releases_nor_clears_stale() {
    let mut app = app();
    app.world_mut().resource_mut::<Player>().world_stale = true;
    // A resident, quiet world described for a tile the avatar is not on.
    let elsewhere = {
        let (tx, ty) = own_tile();
        Some((tx + 8, ty))
    };
    for _ in 0..3 {
        step(&mut app, 0.05, |p| {
            p.focus_tile = elsewhere;
            p.total = 2000;
            p.ready = 2000;
            p.colliders_pending = 0;
            p.scene_ready = true;
            p.focus_resident = true;
        });
        assert!(settling(&mut app), "released on another tile's residency");
        assert!(
            app.world().resource::<Player>().world_stale,
            "the stale flag cleared on another tile's residency"
        );
    }
    // The same facts, now about the right tile: stale clears and the hold ends at once.
    step(&mut app, 0.05, |p| p.focus_tile = Some(own_tile()));
    assert!(!settling(&mut app), "matching residency must still release");
}

#[test]
fn native_ack_waits_for_release_and_preserves_transfer_identity_once() {
    let mut app = app();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(crate::net::NetCommands(tx));
    let token = wrathplus_protocol::wrath_transfer::Token {
        epoch: 8,
        sequence: 19,
    };
    {
        let mut player = app.world_mut().resource_mut::<Player>();
        player.world_stale = false;
        player.native_worldport_ack = Some(token);
    }
    step(&mut app, 0.1, |p| {
        p.total = 2000;
        p.ready = 1500;
        p.colliders_pending = 5;
        p.scene_ready = false;
    });
    assert!(rx.try_recv().is_err());
    step(&mut app, 0.1, |p| {
        p.ready = 2000;
        p.colliders_pending = 0;
        p.scene_ready = true;
    });
    let sent: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(sent.as_slice(), [crate::net::ClientCommand::NativeWorldportAck(t)] if *t == token)
    );
    step(&mut app, 0.1, |_| {});
    assert!(rx.try_recv().is_err());
}

/// The ack is the reference's post-load `0xDC`, so it rides the release, never the snap.
#[test]
fn the_resident_release_pays_the_worldport_ack_once() {
    let mut app = app();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(crate::net::NetCommands(tx));
    {
        let mut player = app.world_mut().resource_mut::<Player>();
        player.world_stale = false;
        player.owes_worldport_ack = true;
    }
    // Still streaming: settling holds, nothing is sent.
    step(&mut app, 0.1, |p| {
        p.total = 2000;
        p.ready = 1500;
        p.colliders_pending = 5;
        p.scene_ready = false;
    });
    assert!(settling(&mut app));
    assert!(
        rx.try_recv().is_err(),
        "the ack went out before the release"
    );
    // Residency lands: the hold ends and the ack goes out.
    step(&mut app, 0.1, |p| {
        p.ready = 2000;
        p.colliders_pending = 0;
        p.scene_ready = true;
    });
    assert!(!settling(&mut app));
    let sent: Vec<crate::net::ClientCommand> = rx.try_iter().collect();
    assert!(
        matches!(sent[..], [crate::net::ClientCommand::WorldportAck]),
        "exactly one worldport ack at the release, got {sent:?}"
    );
    // Paid once: further quiet frames send nothing more.
    step(&mut app, 0.1, |_| {});
    assert!(rx.try_recv().is_err(), "the ack was paid twice");
}

/// vmangos keeps an unacked far teleport out of world, dropping every packet, until logout.
#[test]
fn a_timeout_release_still_pays_the_ack() {
    let mut app = app();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(crate::net::NetCommands(tx));
    {
        let mut player = app.world_mut().resource_mut::<Player>();
        player.world_stale = false;
        player.owes_worldport_ack = true;
    }
    // Frozen counters, scene never presentable: first frame baselines, then the budget runs.
    for _ in 0..=(SETTLE_TIMEOUT + 2.0) as usize {
        step(&mut app, 1.0, |p| {
            p.total = 2000;
            p.ready = 500;
            p.colliders_pending = 300;
            p.scene_ready = false;
        });
    }
    assert!(!settling(&mut app), "the stall backstop never fired");
    let sent: Vec<crate::net::ClientCommand> = rx.try_iter().collect();
    assert!(
        matches!(sent[..], [crate::net::ClientCommand::WorldportAck]),
        "the timeout release must still ack the transfer, got {sent:?}"
    );
}
