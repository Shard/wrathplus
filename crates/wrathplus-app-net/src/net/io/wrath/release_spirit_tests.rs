use super::*;

// Inbound authority records from 093549/packets.cap, 22.737468s–22.739315s:
// ghost speed/modes, their removal, graveyard teleport, hover removal, root.
const ORDERS: &[(u16, &[u8])] = &[
    (0xe2, &[3, 0x21, 4, 3, 0, 0, 0, 0, 0, 0, 0x28, 0x41]),
    (0xe6, &[3, 0x21, 4, 4, 0, 0, 0, 0xaa, 0xaa, 0xe2, 0x40]),
    (0xde, &[3, 0x21, 4, 5, 0, 0, 0]),
    (0xea, &[3, 0x21, 4, 6, 0, 0, 0]),
    (0xe2, &[3, 0x21, 4, 7, 0, 0, 0, 0, 0, 0, 0xe0, 0x40]),
    (0xe6, &[3, 0x21, 4, 8, 0, 0, 0, 0x71, 0x1c, 0x97, 0x40]),
    (0xea, &[3, 0x21, 4, 9, 0, 0, 0]),
    (0xdf, &[3, 0x21, 4, 10, 0, 0, 0]),
    (
        0xc7,
        &[
            3, 0x21, 4, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xf9, 0x45, 0xfd, 0x4f, 0xd7, 0xed, 0x11,
            0xc6, 0x73, 0x68, 0x2b, 0x43, 0x48, 0x3f, 0x76, 0x42, 0xa2, 0x84, 0xbc, 0x40, 0, 0, 0,
            0,
        ],
    ),
    (0xf5, &[3, 0x21, 4, 12, 0, 0, 0]),
    (0xe8, &[3, 0x21, 4, 13, 0, 0, 0]),
];

fn send_order(
    authority: &MovementAuthorityHandle,
    order: PendingAuthorityOrder,
    sent: &mut Vec<(u32, Vec<u8>)>,
) -> Result<UnsetAckResult> {
    let send = |opcode, body: &[u8]| {
        sent.push((opcode, body.to_vec()));
        Ok(())
    };
    match order {
        PendingAuthorityOrder::ForceRoot { order, rooted } => {
            authority.send_root_ack(order, rooted, 99, send)
        }
        PendingAuthorityOrder::Mover(order) => authority.send_mover_ack(order, 99, send),
        PendingAuthorityOrder::UnsetCanFly(_) => unreachable!(),
    }
}

#[test]
fn release_spirit_defers_post_teleport_orders_until_controller_snap() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(90, 0x421);
    authority
        .observe_self_create(0x421, &super::tests::test_self_position())
        .unwrap();
    let mut orders = Vec::new();
    for &(opcode, body) in ORDERS {
        let pending = if matches!(opcode, 0xe8 | 0xea) {
            PendingAuthorityOrder::ForceRoot {
                order: wrath_authority::decode_force_root(body).unwrap(),
                rooted: opcode == 0xe8,
            }
        } else {
            PendingAuthorityOrder::Mover(wrath_authority::decode_mover_order(opcode, body).unwrap())
        };
        authority.raise_authority_order(pending).unwrap();
        orders.push(pending);
    }
    let mut sent = Vec::new();
    for &order in &orders[..8] {
        assert_eq!(
            send_order(&authority, order, &mut sent).unwrap(),
            UnsetAckResult::Sent
        );
    }
    // The controller has not snapped yet, while the reader/writer have reached counter 12.
    assert_eq!(
        send_order(&authority, orders[9], &mut sent).unwrap(),
        UnsetAckResult::PendingTeleport
    );
    assert_eq!(sent.len(), 8);
    assert!(authority.with_movement_send(|| ()).is_none());
    let mut queue = orders[9..].iter().copied().collect();
    let mut timeout = crossbeam_channel::after(PRE_SELF_ORDER_TIMEOUT);
    drain_authority_queue(&mut queue, &mut timeout, |order| {
        send_order(&authority, order, &mut sent)
    })
    .unwrap();
    assert_eq!(queue.len(), 2);
    assert!(!authority
        .send_teleport_ack(0x421, 12, 99, |_| panic!("wrong counter"))
        .unwrap());
    assert!(authority
        .send_teleport_ack(0x421, 11, 99, |body| {
            sent.push((
                u32::from(wrath_authority::MSG_MOVE_TELEPORT_ACK),
                body.to_vec(),
            ));
            Ok(())
        })
        .unwrap());
    drain_authority_queue(&mut queue, &mut timeout, |order| {
        send_order(&authority, order, &mut sent)
    })
    .unwrap();
    assert!(queue.is_empty());
    assert_eq!(
        sent.iter().map(|(op, _)| *op).collect::<Vec<_>>(),
        [0xe3, 0xe7, 0x2d0, 0xeb, 0xe3, 0xe7, 0xeb, 0x2d0, 0xc7, 0xf6, 0xe9]
    );
    for (_, body) in &sent[9..] {
        assert_eq!(
            &body[17..29],
            &ORDERS[8].1[17..29],
            "post-teleport ACK reports destination"
        );
    }
    let state = shared.state.lock().unwrap();
    assert!(state.pending_orders.is_empty());
    assert!(state.rooted);
    drop(state);
    assert!(!authority
        .send_teleport_ack(0x421, 11, 99, |_| panic!("duplicate"))
        .unwrap());
    assert!(
        send_order(&authority, orders[9], &mut sent).is_err(),
        "duplicate mode ACK remains fatal"
    );
}

#[test]
fn teleport_deferral_does_not_admit_unqueued_or_out_of_order_replies() {
    let shared = Arc::new(MovementAuthority::default());
    let authority = shared.install(90, 0x421);
    let teleport = wrath_authority::decode_mover_order(ORDERS[8].0, ORDERS[8].1).unwrap();
    let mode = wrath_authority::decode_mover_order(ORDERS[9].0, ORDERS[9].1).unwrap();
    authority.raise_mover_barrier(teleport).unwrap();
    assert!(authority
        .send_mover_ack(mode, 99, |_, _| panic!("unqueued"))
        .is_err());
    authority.raise_mover_barrier(mode).unwrap();
    let root = wrath_authority::decode_force_root(ORDERS[10].1).unwrap();
    authority.raise_root_barrier(root, true).unwrap();
    assert!(authority
        .send_root_ack(root, true, 99, |_, _| panic!("overtook mode"))
        .is_err());
}
