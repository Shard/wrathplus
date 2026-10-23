use super::*;

#[test]
fn lifecycle_events_reuse_existing_sole_owner_and_strict_decode() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(1, 7);
    for reason in [0u32, 1, 2, 3, u32::MAX] {
        for instant in 0..=255 {
            let mut body = reason.to_le_bytes().to_vec();
            body.push(instant);
            assert!(matches!(authority.logout_event(0x4c, &body).unwrap(),
                Some(SessionEvent::LogoutResponse { reason: r, instant: i })
                if r == reason && i == (instant != 0)));
        }
    }
    authority.send_logout(|| Ok(())).unwrap();
    assert!(matches!(
        authority.logout_event(0x4f, &[]).unwrap(),
        Some(SessionEvent::LogoutCancelled)
    ));
    assert!(matches!(
        authority.logout_event(0x4d, &[]).unwrap(),
        Some(SessionEvent::LoggedOut)
    ));
    assert!(authority.logout_event(0x4f, &[0]).is_err());
    assert!(authority.logout_event(0x4d, &[0]).is_err());
    assert!(authority.logout_event(0x4c, &[0; 4]).is_err());
    assert_eq!(
        route(&ClientCommand::LogoutCancel),
        WrathRoute::LogoutCancel
    );
    assert_eq!(
        wrathplus_protocol::wrath_logout::encode_cancel(),
        (0x4e, vec![])
    );
}

fn ready() -> (Arc<MovementAuthority>, MovementAuthorityHandle) {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(1, 7);
    authority
        .observe_self_create(7, &super::tests::test_self_position())
        .unwrap();
    (shared, authority)
}

#[test]
fn cancel_never_revokes_movement_and_server_root_unroot_owns_freezing() {
    let (_shared, authority) = ready();
    authority.send_logout(|| Ok(())).unwrap();
    assert!(authority.with_movement_send(|| ()).is_some());
    let root = ForceRootOrder {
        mover_guid: 7,
        counter: 1,
    };
    authority.raise_root_barrier(root, true).unwrap();
    authority
        .send_root_ack(root, true, 1, |_, _| Ok(()))
        .unwrap();
    authority.send_logout_cancel(|| Ok(())).unwrap();
    assert!(authority.logout_event(0x4f, &[]).unwrap().is_none());
    assert!(authority.with_movement_send(|| ()).is_none());
    let unroot = ForceRootOrder {
        mover_guid: 7,
        counter: 2,
    };
    authority.raise_root_barrier(unroot, false).unwrap();
    authority
        .send_root_ack(unroot, false, 2, |_, _| Ok(()))
        .unwrap();
    assert!(authority.with_movement_send(|| ()).is_some());
    authority.send_logout(|| Ok(())).unwrap();
    authority.send_logout_cancel(|| Ok(())).unwrap();
    assert!(authority.with_movement_send(|| ()).is_some());
}

#[test]
fn refusal_preserves_movement_complete_revokes_and_ack_never_grants() {
    let (shared, authority) = ready();
    authority.send_logout(|| Ok(())).unwrap();
    authority.logout_event(0x4c, &[1, 0, 0, 0, 0]).unwrap();
    assert!(authority.with_movement_send(|| ()).is_some());
    assert!(authority.logout_event(0x4f, &[]).unwrap().is_none());
    authority.send_logout(|| Ok(())).unwrap();
    assert!(matches!(
        authority.logout_event(0x4f, &[]).unwrap(),
        Some(SessionEvent::LogoutCancelled)
    ));
    assert!(authority.logout_event(0x4f, &[]).unwrap().is_none());
    authority.logout_event(0x4d, &[]).unwrap();
    assert!(authority.with_movement_send(|| ()).is_none());
    assert!(authority.logout_event(0x4f, &[]).unwrap().is_none());
    let next = shared.install(2, 7);
    assert!(authority.logout_event(0x4f, &[]).unwrap().is_none());
    assert!(next.with_movement_send(|| ()).is_none());
}
