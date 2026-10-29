//! The movement halves of the object packets: a create's joined path, the movement blocks, the
//! relayed `MSG_MOVE_*` stream, `SMSG_MONSTER_MOVE` and the observer modes. Each runs on the same
//! event right after `crate::net::objects`' handler, whose spawn and store it reads, so a later
//! handler in the same drain sees the pose this one applied.
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use wrathplus_assets::coords::bevy_to_wow;
use wrathplus_protocol::{MonsterMoveFacing, SessionEvent, SessionEventKind, SplineMode};

use super::{
    apply_move, arrival_snap, create_spline, monster_move_spline, resolve_facing,
    trace_create_spline, trace_move_snap, trace_relay, wire_yaw, write_pose, DisplayFacing,
    PendingMove, RelayMove, RelayOutcome, RemoteMotion, Spline, SplineStopped, UnitMoveModes,
    ROOT_APPLY_WIPE,
};
use crate::app_core::session::SelfGuid;
use crate::net::{GuidIndex, NetHandlerApp};
// The tests' fixtures, which they name through `super::*`.
#[cfg(test)]
use {crate::net::NetEntity, wrathplus_protocol::EntityKind};

/// Registers the movement handlers, after `NetPlugin`'s so each follows the object layer's.
pub(super) fn register(app: &mut App) {
    use SessionEventKind as K;
    app.net_handler(K::ObjectCreate, on_object_create)
        .net_handler(K::ObjectMove, on_object_move)
        .net_handler(K::MoveTimeSkipped, on_move_time_skipped)
        .net_handler(K::UnitMove, on_unit_move)
        .net_handler(K::MonsterMove, on_monster_move)
        .net_handler(K::SplineMoveMode, on_spline_move_mode);
}

/// The movers as one parameter: the poses, motion components and edges the movement packets
/// touch.
#[derive(SystemParam)]
struct Movers<'w, 's> {
    commands: Commands<'w, 's>,
    index: Res<'w, GuidIndex>,
    self_guid: Res<'w, SelfGuid>,
    real: Res<'w, Time<Real>>,
    transforms: Query<'w, 's, &'static mut Transform>,
    remote: Query<'w, 's, &'static mut RemoteMotion>,
    modes: Query<'w, 's, &'static mut UnitMoveModes>,
    riders: Query<'w, 's, &'static mut crate::app_core::movement::TransportRider>,
    hard_landings: MessageWriter<'w, crate::app_core::movement::HardLanding>,
}

/// A create block's motion: the walk the unit is already on, joined at the server's progress,
/// after the object layer has spawned or refreshed it.
fn on_object_create(In(ev): In<SessionEvent>, mut mv: Movers) {
    if let SessionEvent::ObjectCreate { guid, spline, .. } = ev {
        object_create(guid, spline, &mut mv.commands, &mv.index, &mv.transforms);
    }
}

fn on_object_move(In(ev): In<SessionEvent>, mut mv: Movers) {
    if let SessionEvent::ObjectMove {
        guid,
        position,
        orientation,
    } = ev
    {
        object_move(
            guid,
            position,
            orientation,
            &mut mv.commands,
            &mv.index,
            &mut mv.transforms,
        );
    }
}

/// An observed mover skipped time: only its relay chain moves, as the reference's handler does
/// (`0x603b40` → `0x61ab90`, `[CMovement+0xac] += lag`); an unknown guid is dropped, as there.
fn on_move_time_skipped(In(ev): In<SessionEvent>, mut mv: Movers) {
    if let SessionEvent::MoveTimeSkipped { guid, lag_ms } = ev {
        if let Some(mut m) = mv
            .index
            .0
            .get(&guid)
            .and_then(|&e| mv.remote.get_mut(e).ok())
        {
            m.relay.skip_time(lag_ms);
        }
    }
}

/// A relayed move for a unit we observe; our own guid's goes to `player::wire_in` from the
/// object layer, and is only traced here.
fn on_unit_move(In(ev): In<SessionEvent>, mut mv: Movers) {
    if let SessionEvent::UnitMove {
        guid,
        position,
        orientation,
        flags,
        pitch,
        time,
        verb,
        fall_time,
        jump,
        transport,
    } = ev
    {
        let now_ms = mv.real.elapsed_secs_f64() * 1000.0;
        unit_move(
            guid,
            RelayMove {
                wire_ms: time,
                position,
                orientation,
                flags,
                pitch,
                fall_time,
                jump,
                transport,
                verb,
            },
            now_ms,
            &mut mv.commands,
            &mv.index,
            &mv.self_guid,
            &mut mv.remote,
            &mut mv.transforms,
            &mut mv.hard_landings,
        );
    }
}

fn on_monster_move(In(ev): In<SessionEvent>, mut mv: Movers) {
    if let SessionEvent::MonsterMove {
        guid,
        transport,
        start,
        spline_id,
        path,
        facing,
        stop,
        duration_ms,
        flying,
        interpolation,
        cyclic,
        effect,
        run_mode,
    } = ev
    {
        let rooted = modes_of(guid, &mv.index, &mv.modes).rooted();
        monster_move(
            guid,
            transport,
            start,
            spline_id,
            path,
            facing,
            stop,
            duration_ms,
            flying,
            interpolation,
            cyclic,
            effect,
            run_mode,
            rooted,
            &mut mv.commands,
            &mv.index,
            &mut mv.transforms,
            &mut mv.riders,
        );
    }
}

/// The observer movement-mode family: no ack.
fn on_spline_move_mode(In(ev): In<SessionEvent>, mut mv: Movers) {
    if let SessionEvent::SplineMoveMode { guid, mode, apply } = ev {
        spline_move_mode(
            guid,
            mode,
            apply,
            &mut mv.commands,
            &mv.index,
            &mut mv.modes,
            &mut mv.remote,
        );
    }
}

/// A create block's path. The object layer has already spawned the entity or refreshed it in place;
/// a re-create is a fresh snapshot, so its extrapolation goes, and the refreshed pose drops the
/// display smoother as [`write_pose`] does. Traced first, so a `WOW_CREATE_SPLINE=off` run still
/// records what the wire offered.
fn object_create(
    guid: u64,
    spline: Option<wrathplus_protocol::CreateSpline>,
    commands: &mut Commands,
    index: &GuidIndex,
    transforms: &Query<&mut Transform>,
) {
    trace_create_spline(guid, spline.as_ref());
    let walk = spline.and_then(create_spline);
    let Some(&e) = index.0.get(&guid) else {
        return;
    };
    commands.entity(e).remove::<RemoteMotion>();
    // A create without a path says the unit stands still, so a stale path goes.
    match walk {
        Some(s) => {
            commands.entity(e).insert(s);
        }
        None => {
            commands.entity(e).remove::<Spline>();
        }
    }
    if transforms.contains(e) {
        commands.entity(e).remove::<DisplayFacing>();
    }
}

/// An `SMSG_UPDATE_OBJECT` movement block: a new pose that supersedes any active path.
fn object_move(
    guid: u64,
    position: [f32; 3],
    orientation: f32,
    commands: &mut Commands,
    index: &GuidIndex,
    transforms: &mut Query<&mut Transform>,
) {
    if let Some(&e) = index.0.get(&guid) {
        commands.entity(e).remove::<Spline>();
        // A yaw only: the GameObjects that get one are transports, which their tick then owns.
        write_pose(commands, transforms, e, position, wire_yaw(orientation));
    }
}

/// A relayed `MSG_MOVE_*`. The reference schedules a remote's apply (`0x618c30`): the mover's
/// replay chain gives the packet a fire-time, a due move applies now and a later one queues for
/// `drain_pending_moves`, with dead-reckoning in between. `WOW_REMOTE_SNAP=1` applies at arrival.
/// Our own guid's is only traced: the object layer hands it to `player::wire_in`.
fn unit_move(
    guid: u64,
    mv: RelayMove,
    now_ms: f64,
    commands: &mut Commands,
    index: &GuidIndex,
    self_guid: &SelfGuid,
    remote_motion: &mut Query<&mut RemoteMotion>,
    transforms: &mut Query<&mut Transform>,
    landings: &mut MessageWriter<crate::app_core::movement::HardLanding>,
) {
    // Our own guid: the server writing our pose (`SetAsServerSide`, `ctime = 0`), which the
    // reference applies with no mover gate (`0x603bb0`).
    if self_guid.0 == Some(guid) {
        trace_relay(
            guid,
            &mv,
            &Default::default(),
            now_ms,
            0,
            RelayOutcome::SelfMover,
        );
        return;
    }
    let Some(&e) = index.0.get(&guid) else {
        // No entity: traced, since a mover running on while these pile up is a streaming bug.
        trace_relay(
            guid,
            &mv,
            &Default::default(),
            now_ms,
            0,
            RelayOutcome::Unknown,
        );
        return;
    };
    {
        // The server's pose supersedes a stale spline.
        commands.entity(e).remove::<Spline>();
        if let Ok(mut rm) = remote_motion.get_mut(e) {
            // The chain reads the state before this move applies, as the reference times the
            // packet off the live `[esi+0x40]`/`[esi+0x150]`.
            let (flags, queue_empty) = (rm.flags, rm.pending.is_empty());
            let fire_ms = rm.relay.schedule(mv.wire_ms, now_ms, flags, queue_empty);
            let at_arrival = arrival_snap() || rm.fires_at_arrival(fire_ms, now_ms);
            trace_relay(
                guid,
                &mv,
                &rm.relay,
                now_ms,
                rm.pending.len(),
                if at_arrival {
                    RelayOutcome::Now
                } else {
                    RelayOutcome::Queued
                },
            );
            if at_arrival {
                apply_move(e, &mv, &mut rm, now_ms, commands, landings);
            } else {
                // Fire-times are monotone per unit, so the queue stays ordered.
                rm.pending.push_back(PendingMove { fire_ms, mv });
            }
        } else {
            // The first move seeds the chain and applies at arrival; the insert is deferred, so
            // the extrapolator sees it next frame.
            let mut rm = RemoteMotion {
                wow_pos: mv.position,
                orientation: mv.orientation,
                flags: 0,
                pitch: 0.0,
                speed: 0.0,
                vertical_velocity: 0.0,
                jump_xy_vel: [0.0; 2],
                fall_start_z: None,
                pending: std::collections::VecDeque::new(),
                relay: Default::default(),
                last_apply_ms: now_ms,
                last_apply_pos: mv.position,
            };
            let fire_ms = rm.relay.schedule(mv.wire_ms, now_ms, 0, true);
            debug_assert_eq!(fire_ms, now_ms, "a seeding packet fires at arrival");
            trace_relay(guid, &mv, &rm.relay, now_ms, 0, RelayOutcome::Seed);
            apply_move(e, &mv, &mut rm, now_ms, commands, landings);
            write_pose(
                commands,
                transforms,
                e,
                mv.position,
                wire_yaw(mv.orientation),
            );
            commands.entity(e).insert(rm);
        }
    }
}

/// `SMSG_MONSTER_MOVE` or `SMSG_MONSTER_MOVE_TRANSPORT`: settles transport membership, snaps the
/// facing, then attaches or clears the spline. With a `transport`, `start`, the path and an
/// `Angle` facing are deck-local and the unit rides as a [`TransportRider`]; without one, a riding
/// unit leaves the deck, as vmangos drops it (`MoveSplineInit.cpp:156-159`).
fn monster_move(
    guid: u64,
    transport: Option<u64>,
    start: [f32; 3],
    spline_id: u32,
    path: Vec<[f32; 3]>,
    facing: MonsterMoveFacing,
    stop: bool,
    duration_ms: u32,
    flying: bool,
    interpolation: wrathplus_protocol::SplineInterpolation,
    cyclic: bool,
    effect: Option<wrathplus_protocol::SplineEffect>,
    run_mode: Option<bool>,
    rooted: bool,
    commands: &mut Commands,
    index: &GuidIndex,
    transforms: &mut Query<&mut Transform>,
    riders: &mut Query<&mut crate::app_core::movement::TransportRider>,
) {
    if let Some(&e) = index.0.get(&guid) {
        // The desync readout: the gap between where we draw the unit and where its path begins.
        trace_move_snap(
            guid,
            // Both sides in one frame: a deck packet's `start` is compared to the local pose.
            match transport {
                Some(_) => riders.get(e).ok().map(|r| r.local_pos),
                None => transforms.get(e).ok().map(|t| bevy_to_wow(t.translation)),
            },
            start,
            stop,
            duration_ms,
        );
        // Transport membership, settled before any pose is read, and above the root gate, which
        // refuses only the path. An `Angle` facing is deck-local (`MoveSplineInit.cpp:87`); a
        // `Spot` or `Target` is not converted server-side, so it is rebased by the deck's yaw.
        let deck_yaw = |t: u64| {
            index
                .0
                .get(&t)
                .and_then(|&te| transforms.get(te).ok())
                .map(|tf| tf.rotation.to_euler(EulerRot::YXZ).0)
        };
        if let Some(t) = transport {
            let local_facing = match facing {
                MonsterMoveFacing::None => None,
                MonsterMoveFacing::Angle(a) => Some(a),
                other => {
                    let target_pos = |g: u64| {
                        index
                            .0
                            .get(&g)
                            .and_then(|&te| transforms.get(te).ok())
                            .map(|tf| bevy_to_wow(tf.translation))
                    };
                    // The world position anchors the bearing; the wire's `start` is deck-local.
                    let world_pos = transforms.get(e).ok().map(|tf| bevy_to_wow(tf.translation));
                    world_pos
                        .and_then(|p| resolve_facing(other, p, target_pos))
                        .zip(deck_yaw(t))
                        .map(|(world, yaw)| world - yaw)
                }
            };
            match riders.get_mut(e) {
                Ok(mut rider) => {
                    rider.transport_guid = t;
                    rider.local_pos = start;
                    if let Some(o) = local_facing {
                        rider.local_orientation = o;
                    }
                }
                Err(_) => {
                    commands
                        .entity(e)
                        .insert(crate::app_core::movement::TransportRider {
                            transport_guid: t,
                            local_pos: start,
                            local_orientation: local_facing.unwrap_or_default(),
                        });
                }
            }
        } else if riders.get(e).is_ok() {
            commands
                .entity(e)
                .remove::<crate::app_core::movement::TransportRider>();
        }
        // The final facing (moveType 2/3/4) snaps the raw facing (`0x7c6f30`) and resets the
        // display smoother. A following path overrides it with the tangent each frame (`0x7c5490`),
        // so it sticks only on a stop. A deck packet's facing is the rider's `local_orientation`.
        if transport.is_none() && !matches!(facing, MonsterMoveFacing::None) {
            let target_pos = |g: u64| {
                index
                    .0
                    .get(&g)
                    .and_then(|&te| transforms.get(te).ok())
                    .map(|t| bevy_to_wow(t.translation))
            };
            if let Some(orientation) = resolve_facing(facing, start, target_pos) {
                if let Ok(mut t) = transforms.get_mut(e) {
                    t.rotation = Quat::from_rotation_y(orientation);
                    commands.entity(e).remove::<DisplayFacing>();
                }
            }
        }
        // Every spline move un-nocks and drops the weapon hold here. The handler `0x6018f0`
        // (opcodes 0xDD, 0x2AE) ends through `0x60d040` (clear `0x400`) and `0x60f530` (un-nock) on
        // every path but one: a stop within 3 yd of the stop point jumps past both (`0x6019e4`).
        commands.entity(e).remove::<(
            crate::app_core::anim::NockLatch,
            crate::app_core::anim::RangedHold,
        )>();
        // A rooted unit takes no path: `0x6187a0`, the only route into the spline installer
        // `0x7c6a50`, returns on ROOT (`0x6187c2`). Only the path is refused: the un-nock tail runs
        // on every path (`0x6020ef`), and the rest for a rooted unit is untraced.
        if rooted {
            return;
        }
        // The spline supersedes the relay stream: a splined player (charge, taxi) would otherwise
        // keep stale flags that outrank the spline in the anim selector.
        commands.entity(e).remove::<RemoteMotion>();
        let node_count = path.len();
        match monster_move_spline(
            path,
            spline_id,
            stop,
            duration_ms,
            flying,
            interpolation,
            cyclic,
            effect,
            run_mode,
            transport,
        ) {
            Some(spline) => {
                commands.entity(e).insert(spline).remove::<SplineStopped>();
            }
            // Only an actual stop arms the spline-done acknowledgement. Unsupported cyclic or
            // effect paths are diagnosed and refused without fabricating a server stop/ack.
            None if stop => {
                commands
                    .entity(e)
                    .remove::<Spline>()
                    .insert(SplineStopped(spline_id));
            }
            None => {
                warn!(
                    "net: refusing unsupported/invalid spline {spline_id} for {guid:#x} \
                     (nodes={}, duration_ms={duration_ms}, cyclic={cyclic}, effect={effect:?})",
                    node_count
                );
                commands.entity(e).remove::<(Spline, SplineStopped)>();
            }
        }
    }
}

/// This unit's granted modes; no component means all bits clear.
fn modes_of(guid: u64, index: &GuidIndex, modes: &Query<&mut UnitMoveModes>) -> UnitMoveModes {
    index
        .0
        .get(&guid)
        .and_then(|&e| modes.get(e).ok().copied())
        .unwrap_or_default()
}

/// `SMSG_SPLINE_MOVE_*`: a mode on a unit we do not control, with no ack. An unknown guid is
/// dropped, as the reference's handler (`0x603c80`) does; our own guid is not special-cased, as
/// vmangos never sends it for a player's mover.
fn spline_move_mode(
    guid: u64,
    mode: SplineMode,
    apply: bool,
    commands: &mut Commands,
    index: &GuidIndex,
    modes: &mut Query<&mut UnitMoveModes>,
    remote: &mut Query<&mut RemoteMotion>,
) {
    let Some(&e) = index.0.get(&guid) else {
        return; // the reference drops it too
    };
    let mut word = modes_of(guid, index, modes);
    word.set(mode, apply);
    if let Ok(mut live) = modes.get_mut(e) {
        *live = word;
    } else {
        commands.entity(e).insert(word);
    }

    // `SetRoot`'s one-shot wipe (`0x7c7340`): it clears `0xffe07f00`'s complement and stops the
    // fall (`0x7c6290`), so the unit fails the integration gate and a relayed player stops dead.
    // A creature has no direction bits; `monster_move` refuses its path instead.
    if mode == SplineMode::Root && apply {
        if let Ok(mut rm) = remote.get_mut(e) {
            use crate::app_core::movement::move_flags;
            rm.flags &= ROOT_APPLY_WIPE & !(move_flags::FALLING | move_flags::FALLING_FAR);
            rm.speed = 0.0;
            rm.vertical_velocity = 0.0;
            rm.jump_xy_vel = [0.0; 2];
            rm.fall_start_z = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The observer movement-mode family, on a bare `World` with one indexed unit.
    mod spline_modes {
        use super::*;
        use bevy::ecs::system::RunSystemOnce;
        use wrathplus_protocol::SplineMode;

        const MOB: u64 = 0x0000_0000_0000_00F0;

        /// A world holding one indexed unit, optionally already dead-reckoning as a relayed player.
        fn world_with_unit(remote: Option<RemoteMotion>) -> (World, Entity) {
            let mut w = World::new();
            let mut e = w.spawn_empty();
            if let Some(rm) = remote {
                e.insert(rm);
            }
            let entity = e.id();
            let mut index = GuidIndex::default();
            index.0.insert(MOB, entity);
            w.insert_resource(index);
            (w, entity)
        }

        fn remote_walking_forward() -> RemoteMotion {
            RemoteMotion {
                wow_pos: [0.0; 3],
                pending: std::collections::VecDeque::new(),
                orientation: 0.0,
                flags: crate::app_core::movement::move_flags::FORWARD,
                pitch: 0.0,
                speed: 7.0,
                vertical_velocity: 0.0,
                jump_xy_vel: [0.0; 2],
                fall_start_z: None,
                relay: Default::default(),
                last_apply_ms: 0.0,
                last_apply_pos: [0.0; 3],
            }
        }

        /// Runs one grant and flushes its commands.
        fn grant(w: &mut World, mode: SplineMode, apply: bool) {
            w.run_system_once(
                move |mut commands: Commands,
                      index: Res<GuidIndex>,
                      mut modes: Query<&mut UnitMoveModes>,
                      mut remote: Query<&mut RemoteMotion>| {
                    spline_move_mode(
                        MOB,
                        mode,
                        apply,
                        &mut commands,
                        &index,
                        &mut modes,
                        &mut remote,
                    );
                },
            )
            .unwrap();
        }

        /// Every one of the twelve opcodes sets or clears one bit.
        #[test]
        fn a_grant_sets_the_bit_and_its_pair_clears_it() {
            use crate::app_core::movement::move_flags;
            let (mut w, e) = world_with_unit(None);

            grant(&mut w, SplineMode::Hover, true);
            grant(&mut w, SplineMode::WaterWalk, true);
            let m = *w
                .entity(e)
                .get::<UnitMoveModes>()
                .expect("first grant inserts");
            assert_eq!(
                m.0,
                move_flags::HOVER | move_flags::WATER_WALKING,
                "two grants accumulate in one word — the reference has one CMovement+0x40"
            );

            grant(&mut w, SplineMode::Hover, false);
            let m = *w.entity(e).get::<UnitMoveModes>().unwrap();
            assert_eq!(
                m.0,
                move_flags::WATER_WALKING,
                "UNSET_HOVER takes back hover and nothing else"
            );
        }

        /// `SET_RUN_MODE` clears the walk bit: the pair is inverted against its names (`0x617e80`
        /// → `SetRunMode` `0x7c71c0`), folded at the parse.
        #[test]
        fn the_walk_bit_round_trips_with_the_run_opcodes_sense() {
            use crate::app_core::movement::move_flags;
            let (mut w, e) = world_with_unit(None);
            grant(&mut w, SplineMode::WalkMode, true); // SMSG_SPLINE_MOVE_SET_WALK_MODE
            assert_eq!(
                w.entity(e).get::<UnitMoveModes>().unwrap().0,
                move_flags::WALK_MODE
            );
            grant(&mut w, SplineMode::WalkMode, false); // SMSG_SPLINE_MOVE_SET_RUN_MODE
            assert_eq!(w.entity(e).get::<UnitMoveModes>().unwrap().0, 0);
        }

        /// `SetRoot` (`0x7c7340`) wipes the direction bits, so the unit fails the integration gate
        /// (`0x616e20`, `test [esi+0x40], 0x20ff`) and is not stepped.
        #[test]
        fn a_root_wipes_the_direction_bits_the_dead_reckon_runs_on() {
            use crate::app_core::movement::move_flags;
            let (mut w, e) = world_with_unit(Some(remote_walking_forward()));

            grant(&mut w, SplineMode::Root, true);

            let rm = w.entity(e).get::<RemoteMotion>().unwrap();
            assert_eq!(
                rm.flags & move_flags::INTEGRATED,
                0,
                "nothing the integration gate tests may survive the root — this is what stops it"
            );
            assert_eq!(rm.speed, 0.0, "and it is not still travelling at run speed");
            assert!(w.entity(e).get::<UnitMoveModes>().unwrap().rooted());
        }

        /// `ClearRoot` (`0x7c7370`) only clears the bit.
        #[test]
        fn an_unroot_clears_the_bit_and_invents_no_movement() {
            let (mut w, e) = world_with_unit(Some(remote_walking_forward()));
            grant(&mut w, SplineMode::Root, true);
            grant(&mut w, SplineMode::Root, false);
            assert!(!w.entity(e).get::<UnitMoveModes>().unwrap().rooted());
            assert_eq!(
                w.entity(e).get::<RemoteMotion>().unwrap().flags,
                0,
                "still no direction bits — the wipe is one-shot and the unroot does not undo it"
            );
        }

        /// As in the reference's handler (`0x603c80`); the create carries the modes itself.
        #[test]
        fn a_grant_for_a_unit_we_do_not_have_is_dropped() {
            let (mut w, e) = world_with_unit(None);
            w.run_system_once(
                |mut commands: Commands,
                 index: Res<GuidIndex>,
                 mut modes: Query<&mut UnitMoveModes>,
                 mut remote: Query<&mut RemoteMotion>| {
                    spline_move_mode(
                        0xDEAD_BEEF,
                        SplineMode::Root,
                        true,
                        &mut commands,
                        &index,
                        &mut modes,
                        &mut remote,
                    );
                },
            )
            .unwrap();
            assert!(w.entity(e).get::<UnitMoveModes>().is_none());
        }

        /// `0x6187a0` returns on ROOT (`0x6187c2`), so a queued path does not move a pinned body.
        #[test]
        fn a_rooted_unit_refuses_the_path_the_server_queued() {
            let mut w = World::new();
            let e = w
                .spawn((
                    Transform::default(),
                    NetEntity {
                        kind: EntityKind::Unit,
                        display_id: None,
                        scale: 1.0,
                    },
                ))
                .id();
            let mut index = GuidIndex::default();
            index.0.insert(MOB, e);
            w.insert_resource(index);

            let path = vec![[0.0, 0.0, 0.0], [30.0, 0.0, 0.0]];
            for (rooted, expect_spline) in [(true, false), (false, true)] {
                let path = path.clone();
                w.run_system_once(
                    move |mut commands: Commands,
                          index: Res<GuidIndex>,
                          mut transforms: Query<&mut Transform>,
                          mut riders: Query<&mut crate::app_core::movement::TransportRider>| {
                        monster_move(
                            MOB,
                            None,
                            [0.0, 0.0, 0.0],
                            7,
                            path.clone(),
                            MonsterMoveFacing::None,
                            false,
                            3000,
                            false,
                            wrathplus_protocol::SplineInterpolation::Linear,
                            false,
                            None,
                            Some(true),
                            rooted,
                            &mut commands,
                            &index,
                            &mut transforms,
                            &mut riders,
                        );
                    },
                )
                .unwrap();
                assert_eq!(
                    w.entity(e).get::<Spline>().is_some(),
                    expect_spline,
                    "rooted={rooted}: a pinned body takes no path, an unpinned one does"
                );
            }
        }

        /// A plain path takes the unit off the deck, where vmangos drops the passenger
        /// (`MoveSplineInit.cpp:156-159`).
        #[test]
        fn a_transport_path_attaches_the_rider_and_a_plain_one_lets_it_go() {
            const BOAT: u64 = 0x2000_0000_0000_0007;
            let mut w = World::new();
            let e = w
                .spawn((
                    Transform::default(),
                    NetEntity {
                        kind: EntityKind::Unit,
                        display_id: None,
                        scale: 1.0,
                    },
                ))
                .id();
            let boat = w.spawn(Transform::default()).id();
            let mut index = GuidIndex::default();
            index.0.insert(MOB, e);
            index.0.insert(BOAT, boat);
            w.insert_resource(index);

            let deck_path = vec![[1.5, -2.5, 0.75], [4.0, -2.5, 0.75]];
            w.run_system_once(
                move |mut commands: Commands,
                      index: Res<GuidIndex>,
                      mut transforms: Query<&mut Transform>,
                      mut riders: Query<&mut crate::app_core::movement::TransportRider>| {
                    monster_move(
                        MOB,
                        Some(BOAT),
                        [1.5, -2.5, 0.75],
                        7,
                        deck_path.clone(),
                        MonsterMoveFacing::Angle(1.25),
                        false,
                        3000,
                        false,
                        wrathplus_protocol::SplineInterpolation::Linear,
                        false,
                        None,
                        Some(true),
                        false,
                        &mut commands,
                        &index,
                        &mut transforms,
                        &mut riders,
                    );
                },
            )
            .unwrap();
            let rider = w
                .entity(e)
                .get::<crate::app_core::movement::TransportRider>()
                .expect("a transport path attaches its rider");
            assert_eq!(rider.transport_guid, BOAT);
            assert_eq!(rider.local_pos, [1.5, -2.5, 0.75], "seeded deck-local");
            assert!(
                (rider.local_orientation - 1.25).abs() < 1e-6,
                "Angle is deck-local"
            );
            assert_eq!(
                w.entity(e).get::<Spline>().expect("a path").deck,
                Some(BOAT),
                "the spline rides the deck frame, not the world"
            );
            // The deck-local start is never written as a world position.
            assert_eq!(
                w.entity(e).get::<Transform>().unwrap().translation,
                Vec3::ZERO
            );

            let world_path = vec![[100.0, 200.0, 30.0], [110.0, 200.0, 30.0]];
            w.run_system_once(
                move |mut commands: Commands,
                      index: Res<GuidIndex>,
                      mut transforms: Query<&mut Transform>,
                      mut riders: Query<&mut crate::app_core::movement::TransportRider>| {
                    monster_move(
                        MOB,
                        None,
                        [100.0, 200.0, 30.0],
                        8,
                        world_path.clone(),
                        MonsterMoveFacing::None,
                        false,
                        3000,
                        false,
                        wrathplus_protocol::SplineInterpolation::Linear,
                        false,
                        None,
                        Some(true),
                        false,
                        &mut commands,
                        &index,
                        &mut transforms,
                        &mut riders,
                    );
                },
            )
            .unwrap();
            assert!(
                w.entity(e)
                    .get::<crate::app_core::movement::TransportRider>()
                    .is_none(),
                "a plain path takes the unit off the deck"
            );
            assert_eq!(w.entity(e).get::<Spline>().expect("a path").deck, None);
        }
    }
}
