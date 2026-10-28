use super::*;

pub(super) fn begin_transport_worldport(player: &mut Player) {
    player.transport_worldport = true;
    player.reseat = true;
    player.ride = None;
    player.vel_y = 0.0;
    player.horiz_vel = Vec3::ZERO;
    player.move_flags = 0;
    player.autorun = false;
    player.airborne_since = None;
    player.native_worldport_ack = None;
    player.owes_worldport_ack = false;
}

pub(super) fn restore_transport_worldport(
    player: &mut Player,
    cam: &mut FlyCam,
    rider: &crate::app_core::movement::TransportRider,
    entity: Entity,
    guid: u64,
    deck: &Transform,
) {
    let local_pos = wow_to_bevy(rider.local_pos);
    let boat_yaw = deck.rotation.to_euler(EulerRot::YXZ).0;
    let yaw = rider.local_orientation + boat_yaw;
    cam.yaw += wrap_pi(yaw - player.face_yaw);
    player.pos = deck.translation + deck.rotation * local_pos;
    player.face_yaw = yaw;
    player.model_yaw = yaw;
    player.last_facing = yaw.rem_euclid(std::f32::consts::TAU);
    player.last_pos = bevy_to_wow(player.pos);
    player.ride = Some(crate::player::state::PlayerRide {
        entity,
        guid,
        local_pos,
        boat_yaw,
    });
    player.active = true;
    player.reseat = false;
    player.transport_worldport = false;
    player.settling = false;
    player.world_stale = true;
}

#[cfg(test)]
mod transport_worldport_tests {
    use super::*;
    use crate::app_core::movement::TransportRider;
    use crate::net::GuidIndex;
    use bevy::ecs::system::RunSystemOnce;

    fn tick(world: &mut World) {
        world
            .run_system_once(
                |time: Res<Time>,
                 mut commands: Commands,
                 mut player: ResMut<Player>,
                 mut cam: ResMut<TestCamera>,
                 net: Res<NetCommands>,
                 mut teleports: MessageReader<TeleportMessage>,
                 mut worldports: MessageReader<WorldportMessage>,
                 mut speeds: MessageReader<SpeedChangeMessage>,
                 mut modes: MessageReader<MoveModeMessage>,
                 mut knocks: MessageReader<KnockBackMessage>,
                 mut moves: MessageReader<SelfMoveMessage>,
                 mut controls: MessageReader<ClientControlMessage>,
                 transports: crate::player::TransportQuery,
                 body: Query<(&Transform, Option<&TransportRider>), With<Embodied>>,
                 index: Res<GuidIndex>| {
                    let pose = body.single().ok();
                    apply_server_moves(
                        &time,
                        &mut commands,
                        &mut player,
                        &mut cam.0,
                        &net,
                        &mut teleports,
                        &mut worldports,
                        &mut speeds,
                        &mut modes,
                        &mut knocks,
                        &mut moves,
                        &mut controls,
                        Some(7),
                        &transports,
                        pose.map(|(t, _)| (t.translation, 0.0)),
                        pose.and_then(|(_, r)| r),
                        &index,
                    );
                },
            )
            .unwrap();
        world.resource_mut::<Messages<WorldportMessage>>().clear();
    }

    #[derive(Resource)]
    struct TestCamera(FlyCam);

    #[test]
    fn native_crossing_waits_for_fresh_self_and_recreated_deck_then_carries() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Player {
            active: true,
            pos: Vec3::splat(900.0),
            ..Default::default()
        });
        world.insert_resource(TestCamera(FlyCam {
            yaw: 0.0,
            pitch: -0.45,
            speed: 1.0,
        }));
        world.init_resource::<GuidIndex>();
        let (tx, rx) = crossbeam_channel::unbounded();
        world.insert_resource(crate::net::NetCommands(tx));
        world.init_resource::<Messages<TeleportMessage>>();
        world.init_resource::<Messages<WorldportMessage>>();
        world.init_resource::<Messages<SpeedChangeMessage>>();
        world.init_resource::<Messages<MoveModeMessage>>();
        world.init_resource::<Messages<KnockBackMessage>>();
        world.init_resource::<Messages<SelfMoveMessage>>();
        world.init_resource::<Messages<ClientControlMessage>>();
        // Even if a stale source-map body is still visible this frame, it cannot pay the handoff.
        let old = world
            .spawn((Embodied, Transform::from_translation(Vec3::splat(800.0))))
            .id();
        let token = wrathplus_protocol::wrath_transfer::Token {
            epoch: 4,
            sequence: 2,
        };
        world
            .resource_mut::<Messages<WorldportMessage>>()
            .write(WorldportMessage {
                map_id: 0,
                position: [0.35, -2.36, -10.46],
                orientation: 2.3,
                needs_ack: true,
                native_token: Some(token),
                transport_entry: Some(176081),
            });
        tick(&mut world);
        assert!(
            matches!(rx.try_recv().unwrap(), ClientCommand::NativeWorldportAck(t) if t == token)
        );
        assert_eq!(world.resource::<CurrentMap>().0, 0);
        assert!(world.resource::<Player>().transport_worldport);
        assert!(world.resource::<Player>().reseat);
        assert_eq!(world.resource::<Player>().pos, Vec3::splat(900.0));
        world.despawn(old);
        tick(&mut world); // Neither self nor deck has arrived.
        let rider = TransportRider {
            transport_guid: 42,
            local_pos: [1.0, 2.0, 3.0],
            local_orientation: 0.4,
        };
        world.spawn((
            Embodied,
            Transform::from_translation(Vec3::splat(700.0)),
            rider,
        ));
        tick(&mut world); // Self before transport: no on-foot simulation.
        assert!(world.resource::<Player>().reseat);
        let deck_pose = Transform::from_translation(Vec3::new(100.0, 200.0, 300.0))
            .with_rotation(Quat::from_rotation_y(0.7));
        let deck = world
            .spawn((Transport::test_deck(), Guid(42), deck_pose))
            .id();
        world.resource_mut::<GuidIndex>().0.insert(42, deck);
        tick(&mut world);
        let p = world.resource::<Player>();
        assert!(!p.transport_worldport && !p.reseat && p.active && !p.settling);
        assert_eq!(p.ride.as_ref().unwrap().entity, deck);
        let expected = deck_pose.translation + deck_pose.rotation * wow_to_bevy([1.0, 2.0, 3.0]);
        assert!(p.pos.distance(expected) < 0.001);
        assert!((p.face_yaw - 1.1).abs() < 0.001);
        let tail = crate::player::movement_net::wire_transport(p).unwrap();
        assert_eq!(tail.guid, 42);
        assert_eq!([tail.pos.x, tail.pos.y, tail.pos.z], [1.0, 2.0, 3.0]);
        assert!((tail.orientation - 0.4).abs() < 0.001);
        assert!(
            rx.try_recv().is_err(),
            "no duplicate ACK or on-foot movement"
        );
        // Ordinary carry now follows the recreated entity, not the discarded source deck.
        world.get_mut::<Transform>(deck).unwrap().translation.x += 5.0;
        world
            .run_system_once(
                |mut p: ResMut<Player>,
                 mut cam: ResMut<TestCamera>,
                 decks: crate::player::TransportQuery| {
                    crate::player::ride::carry(&mut p, &mut cam.0, &decks);
                },
            )
            .unwrap();
        assert!(
            world
                .resource::<Player>()
                .pos
                .distance(expected + Vec3::X * 5.0)
                < 0.001
        );
        // AC's exit-trigger seam: fresh self carries a stale tail, then OUT_OF_RANGE
        // removes the phantom deck. Net restores the living world pose and drops the tail.
        world.resource_mut::<Player>().transport_worldport = true;
        world.resource_mut::<Player>().reseat = true;
        let body = world
            .query_filtered::<Entity, With<Embodied>>()
            .single(&world)
            .unwrap();
        let destination = wow_to_bevy([-4838.95, -1318.46, 501.868]);
        world
            .entity_mut(body)
            .remove::<TransportRider>()
            .insert(Transform::from_translation(destination));
        world.despawn(deck);
        world.resource_mut::<GuidIndex>().0.remove(&42);
        tick(&mut world);
        let p = world.resource::<Player>();
        assert_eq!(p.pos, destination);
        assert!(!p.transport_worldport && !p.reseat);
        assert!(p.ride.is_none() && p.settling && p.world_stale);
        assert!(crate::player::movement_net::wire_transport(p).is_none());
        assert!(rx.try_recv().is_err(), "exit never duplicates the paid ACK");
        // An ordinary on-foot worldport supersedes a pending transport handoff, using a
        // world pose and the existing deferred-ACK settle path, not the passenger path.
        world.resource_mut::<Player>().transport_worldport = true;
        world
            .resource_mut::<Messages<WorldportMessage>>()
            .write(WorldportMessage {
                map_id: 1,
                position: [10.0, 20.0, 30.0],
                orientation: 0.2,
                needs_ack: true,
                native_token: Some(token),
                transport_entry: None,
            });
        tick(&mut world);
        let p = world.resource::<Player>();
        assert!(!p.transport_worldport);
        assert!(p.ride.is_none() && p.settling);
        assert_eq!(p.pos, wow_to_bevy([10.0, 20.0, 30.0]));
        assert_eq!(p.native_worldport_ack, Some(token));
        assert!(rx.try_recv().is_err(), "on-foot ACK waits for settle");
    }
}
