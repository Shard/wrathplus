use super::*;

#[test]
fn player_action_authority_requires_current_self_control_but_not_movement_permission() {
    let shared = Arc::new(MovementAuthority::default());
    let old = shared.install(60, 0x55);
    assert!(old.with_player_action(|| ()).is_none());
    old.observe_self_create(0x55, &test_self_position())
        .unwrap();
    assert_eq!(old.with_player_action(|| 1), Some(1));
    let root = ForceRootOrder {
        mover_guid: 0x55,
        counter: 1,
    };
    old.raise_root_barrier(root, true).unwrap();
    old.send_root_ack(root, true, 2, |_, _| Ok(())).unwrap();
    assert!(old.with_movement_send(|| ()).is_none());
    assert_eq!(
        old.with_player_action(|| 2),
        Some(2),
        "root cannot disable selection/melee"
    );
    old.revoke();
    assert!(old.with_player_action(|| ()).is_none());
    let current = shared.install(61, 0x55);
    current
        .observe_self_create(0x55, &test_self_position())
        .unwrap();
    assert!(old.with_player_action(|| ()).is_none());
    assert_eq!(current.with_player_action(|| 3), Some(3));
}

#[test]
fn movement_authority_requires_self_create_before_normal_initial_grant() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(1, 0x55);
    let sends = AtomicU64::new(0);

    assert_eq!(
        authority.with_movement_send(|| sends.fetch_add(1, Ordering::SeqCst)),
        None
    );
    let error = authority
        .observe_self_create(0x66, &test_self_position())
        .unwrap_err();
    assert!(error.to_string().contains("foreign mover"));
    assert_eq!(
        authority.with_movement_send(|| sends.fetch_add(1, Ordering::SeqCst)),
        None
    );
    let authority = shared.install(2, 0x55);
    authority
        .observe_self_create(0x55, &test_self_position())
        .unwrap();
    assert_eq!(
        authority.with_movement_send(|| sends.fetch_add(1, Ordering::SeqCst)),
        Some(0)
    );
    assert_eq!(sends.load(Ordering::SeqCst), 1);
}

#[test]
fn pre_self_order_timeout_revokes_and_queues_epoch_fatal() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(35, 0x351);
    authority.observe_control(0x351, true).unwrap();
    let (control_tx, control_rx) = crossbeam_channel::unbounded();

    expire_pre_self_orders(&authority, 35, 2, &control_tx);

    let WrathWriterControl::Fatal { epoch, reason } = control_rx.recv().unwrap() else {
        panic!("expected fatal control");
    };
    assert_eq!(epoch, 35);
    assert!(reason.contains("self pose did not arrive"));
    assert!(reason.contains("2 pending movement order(s)"));
    authority
        .observe_self_create(0x351, &test_self_position())
        .unwrap();
    assert!(authority.with_movement_send(|| ()).is_none());
}

#[test]
fn self_control_grant_and_deny_gate_movement() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(2, 0x77);
    authority
        .observe_self_create(0x77, &test_self_position())
        .unwrap();
    authority.observe_control(0x77, false).unwrap();
    assert!(authority.with_movement_send(|| ()).is_none());
    authority.observe_control(0x77, true).unwrap();
    assert_eq!(authority.with_movement_send(|| 9), Some(9));
    authority.observe_control(0x77, false).unwrap();
    assert!(authority.with_movement_send(|| ()).is_none());
}

#[test]
fn deny_before_self_create_prevents_implicit_grant() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(8, 0xee);

    authority.observe_control(0xee, false).unwrap();
    authority
        .observe_self_create(0xee, &test_self_position())
        .unwrap();

    assert!(authority.with_movement_send(|| ()).is_none());
}

#[test]
fn repeated_self_create_does_not_restore_revoked_control() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(9, 0xff);

    authority
        .observe_self_create(0xff, &test_self_position())
        .unwrap();
    assert_eq!(authority.with_movement_send(|| 1), Some(1));
    authority.revoke();
    authority
        .observe_self_create(0xff, &test_self_position())
        .unwrap();

    assert!(authority.with_movement_send(|| ()).is_none());
}

#[test]
fn explicit_grant_before_self_create_still_requires_create() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(10, 0x110);

    authority.observe_control(0x110, true).unwrap();
    assert!(authority.with_movement_send(|| ()).is_none());
    authority
        .observe_self_create(0x110, &test_self_position())
        .unwrap();

    assert_eq!(authority.with_movement_send(|| 2), Some(2));
}

#[test]
fn new_epoch_resets_explicit_control_decision() {
    let shared = Arc::new(MovementAuthority::default());
    let denied = shared.install(11, 0x120);
    denied.observe_control(0x120, false).unwrap();
    denied
        .observe_self_create(0x120, &test_self_position())
        .unwrap();
    assert!(denied.with_movement_send(|| ()).is_none());

    let replacement = shared.install(12, 0x130);
    replacement
        .observe_self_create(0x130, &test_self_position())
        .unwrap();

    assert_eq!(replacement.with_movement_send(|| 3), Some(3));
    assert!(denied.with_movement_send(|| ()).is_none());
}

#[test]
fn a_vehicle_handed_to_us_is_driven_and_acked_from_its_own_pose() {
    use wrathplus_protocol::wrath_gameplay::{MovementInfo, Position};
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(3, 0x88);
    authority
        .observe_self_create(0x88, &test_self_position())
        .unwrap();
    // The vehicle's create gives the pose its first orders ACK from.
    let car = MovementInfo {
        flags: 0,
        flags2: 0,
        timestamp: 0,
        position: Position {
            x: 50.0,
            y: 60.0,
            z: 70.0,
            orientation: 1.0,
        },
        transport: None,
        pitch: None,
        fall_time: 0,
        jump: None,
        spline_elevation: None,
    };
    authority.observe_unit_create(0x99, &MovementPosition::Living(car));
    // `SetClientControl(vehicle, true)`, then the root AzerothCore puts on a DISABLE_MOVE one.
    authority.observe_control(0x99, true).unwrap();
    let root = ForceRootOrder {
        mover_guid: 0x99,
        counter: 1,
    };
    assert!(authority.raise_root_barrier(root, true).unwrap());
    let mut sent = Vec::new();
    assert_eq!(
        authority
            .send_root_ack(root, true, 5, |opcode, body| {
                sent.push((opcode, body.to_vec()));
                Ok(())
            })
            .unwrap(),
        UnsetAckResult::Sent
    );
    assert_eq!(
        sent,
        vec![(
            wrath_authority::CMSG_FORCE_MOVE_ROOT_ACK,
            wrath_authority::encode_force_root_on_foot_ack(
                0x99,
                1,
                GroundPose::new(0, [50.0, 60.0, 70.0], 1.0).unwrap().into(),
                5,
                true,
            )
            .unwrap()
        )]
    );
    // Rooted, the vehicle may only turn: a turn goes out under its guid with ROOT, a step
    // does not, and our body's claim is not the mover.
    let turning = OnFootPose {
        flags: wrath_ground::MOVE_TURN_LEFT,
        ..GroundPose::new(0, [50.0, 60.0, 70.0], 1.2).unwrap().into()
    };
    let mut wrote = None;
    assert!(authority
        .with_on_foot_send(0x99, turning, |guid, pose| {
            wrote = Some((guid, pose.flags));
            Ok(())
        })
        .unwrap()
        .is_ok());
    assert_eq!(
        wrote,
        Some((0x99, wrath_ground::MOVE_TURN_LEFT | wrath_ground::MOVE_ROOT))
    );
    let stepping = OnFootPose {
        flags: wrath_ground::MOVE_FORWARD,
        ..turning
    };
    assert!(authority
        .with_on_foot_send(0x99, stepping, |_, _| panic!("rooted step"))
        .is_none());
    assert!(authority
        .with_on_foot_send(0x88, turning, |_, _| panic!("our body is not the mover"))
        .is_none());
    // An order for a unit we never saw is ignored, not fatal.
    assert!(!authority
        .raise_unset_barrier(UnsetCanFlyOrder {
            mover_guid: 0x777,
            counter: 2,
        })
        .unwrap());
    // Control returns to our body with its own pose and no root.
    authority.observe_control(0x88, true).unwrap();
    assert_eq!(authority.with_movement_send(|| 1), Some(1));
    let state = authority.shared.state.lock().unwrap();
    assert_eq!(state.mover, 0x88);
    assert_eq!(state.ground_pose.unwrap().position, [1.0, 2.0, 3.0]);
    assert!(state.parked[&0x99].rooted);
}

#[test]
fn a_rooted_passenger_keeps_its_deck_and_reports_from_it() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(4, 0x89);
    authority
        .observe_self_create(0x89, &test_self_position())
        .unwrap();
    let deck = wrath_ground::TransportTail {
        guid: 0x1FC0_0000_0000_0042,
        position: [1.0, 2.0, 3.0],
        orientation: 0.5,
        time: 7,
    };
    let riding = OnFootPose {
        flags: wrath_ground::MOVE_ON_TRANSPORT | wrath_ground::MOVE_FORWARD,
        position: [10.0, 20.0, 30.0],
        orientation: 0.25,
        fall_time: 0,
        fall: None,
        pitch: None,
        transport: Some(deck),
    };
    assert!(authority.with_self_send(riding, || Ok(())).unwrap().is_ok());
    let root = ForceRootOrder {
        mover_guid: 0x89,
        counter: 30,
    };
    authority.raise_root_barrier(root, true).unwrap();
    let mut sent = Vec::new();
    authority
        .send_root_ack(root, true, 9, |opcode, body| {
            sent.push((opcode, body.to_vec()));
            Ok(())
        })
        .unwrap();
    // A STOP (still on the deck), then the ACK with ROOT | ON_TRANSPORT.
    let [(stop, stop_body), (ack, ack_body)] = &sent[..] else {
        panic!("{sent:?}")
    };
    assert_eq!(
        (*stop, *ack),
        (
            wrath_ground::MSG_MOVE_STOP,
            wrath_authority::CMSG_FORCE_MOVE_ROOT_ACK
        )
    );
    let stop_movement = wrathplus_protocol::wrath_gameplay::OutboundMovement::from_bytes(stop_body)
        .unwrap()
        .movement;
    assert_eq!(stop_movement.flags, wrath_ground::MOVE_ON_TRANSPORT);
    assert_eq!(stop_movement.transport.unwrap().guid, deck.guid);
    // 0x89 packs to two bytes, so the flags follow at byte 6.
    let flags = u32::from_le_bytes(ack_body[6..10].try_into().unwrap());
    assert_eq!(
        flags,
        wrath_authority::MOVEMENT_FLAG_ROOT | wrath_ground::MOVE_ON_TRANSPORT
    );
}

#[test]
fn unsupported_mandatory_orders_are_classified_fail_closed() {
    // Only the never-sent swim/fly transition pair remains undecodable; it is logged and ignored.
    for opcode in [0x033e, 0x033f] {
        assert!(mandatory_movement_order(opcode), "{opcode:#05x}");
        assert!(!wrath_authority::is_mover_order(opcode), "{opcode:#05x}");
    }
    // Every other self order has a native ACK path.
    for opcode in [
        0x00c7, 0x00de, 0x00df, 0x00e2, 0x00e4, 0x00e6, 0x00ef, 0x00f2, 0x00f3, 0x00f4, 0x00f5,
        0x02da, 0x02dc, 0x02de, 0x0343, 0x0381, 0x0383, 0x045c, 0x04ce, 0x04d0, 0x0516,
    ] {
        assert!(wrath_authority::is_mover_order(opcode), "{opcode:#05x}");
        assert!(!mandatory_movement_order(opcode), "{opcode:#05x}");
    }
    // An observed teleport is a relay, not an order.
    assert!(wrathplus_protocol::wrath_relay::handles(0x00c5));
    assert!(!mandatory_movement_order(0x00c5));
    for opcode in [
        SMSG_PONG,
        SMSG_CLIENT_CONTROL_UPDATE,
        wrath_authority::SMSG_MOVE_UNSET_CAN_FLY,
        wrath_authority::SMSG_FORCE_MOVE_ROOT,
        wrath_authority::SMSG_FORCE_MOVE_UNROOT,
        0x00dd,
    ] {
        assert!(!mandatory_movement_order(opcode), "{opcode:#05x}");
    }

    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(4, 0xaa);
    authority
        .observe_self_create(0xaa, &test_self_position())
        .unwrap();
    assert!(mandatory_movement_order(0x033e));
    authority.revoke();
    assert!(authority.with_movement_send(|| ()).is_none());
}

#[test]
fn stale_authority_handles_cannot_send_or_change_replacement_session() {
    let shared = Arc::new(MovementAuthority::default());
    let stale = shared.install(5, 0xbb);
    stale
        .observe_self_create(0xbb, &test_self_position())
        .unwrap();
    assert_eq!(stale.with_movement_send(|| 1), Some(1));

    let current = shared.install(6, 0xcc);
    stale
        .observe_self_create(0xbb, &test_self_position())
        .unwrap();
    stale.observe_control(0xbb, true).unwrap();
    assert!(stale.with_movement_send(|| ()).is_none());
    assert!(current.with_movement_send(|| ()).is_none());
    current
        .observe_self_create(0xcc, &test_self_position())
        .unwrap();
    assert_eq!(current.with_movement_send(|| 2), Some(2));
}
