//! Decrypted native packets reach the sole lossless FIFO without a proposal reducer.
use super::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::wrath_world::WrathServerPacket;

fn proposal(count: u8) -> Vec<u8> {
    let mut body = vec![
        7, 0, 0, 254, 128, 120, 86, 52, 18, 255, 255, 255, 255, 129, count,
    ];
    for n in 0..count {
        body.extend([n, 0, 0, 128, 255, 128, 127, 2, n]);
    }
    body
}
fn deliver(app: &mut App, adapter: &mut WrathEventAdapter, body: Vec<u8>) -> usize {
    let session_events = match adapter
        .dispatch_packet(
            &WrathServerPacket {
                opcode: 0x0361,
                body,
            },
            1,
        )
        .unwrap()
    {
        WrathDispatch::Handled { session_events, .. } => session_events,
        WrathDispatch::Unhandled { opcode: 0x0361 } => return 0,
        _ => panic!("unexpected dispatch"),
    };
    let count = session_events.len();
    crate::net::handlers::dispatch(app.world_mut(), session_events);
    count
}

#[test]
fn adapter_preserves_all_proposal_rows_repeats_and_reset_in_sole_fifo() {
    let mut app = App::new();
    app.add_plugins(UiLfgPlugin);
    let mut adapter = WrathEventAdapter::new();
    for count in [6, 6, 0, 255] {
        assert_eq!(deliver(&mut app, &mut adapter, proposal(count)), 1);
    }
    let queue = &app.world().resource::<NativeLfgPackets>().0;
    assert_eq!(queue.len(), 4);
    for (packet, count) in queue.iter().zip([6, 6, 0, 255]) {
        let Packet::Proposal(p) = packet else {
            panic!("wrong packet")
        };
        assert_eq!(
            (p.entry, p.state, p.id, p.encounter_mask, p.silent),
            (0xfe000007, 128, 0x12345678, u32::MAX, 129)
        );
        assert_eq!(p.members.len(), count);
        for (n, row) in p.members.iter().enumerate() {
            assert_eq!(
                (
                    row.roles,
                    row.is_self,
                    row.in_dungeon,
                    row.same_group,
                    row.answered,
                    row.accepted
                ),
                (0x80000000 | n as u32, 255, 128, 127, 2, n as u8)
            );
        }
    }
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<NativeLfgPackets>().0.is_empty());
    assert_eq!(deliver(&mut app, &mut adapter, proposal(0)), 1);
    assert_eq!(app.world().resource::<NativeLfgPackets>().0.len(), 1);
}

#[test]
fn malformed_proposal_never_publishes_partial_fifo_entry() {
    let mut app = App::new();
    app.add_plugins(UiLfgPlugin);
    let mut adapter = WrathEventAdapter::new();
    let full = proposal(6);
    for end in 0..full.len() {
        assert_eq!(deliver(&mut app, &mut adapter, full[..end].to_vec()), 0);
    }
    let mut trailing = full;
    trailing.push(0);
    assert_eq!(deliver(&mut app, &mut adapter, trailing), 0);
    assert!(app.world().resource::<NativeLfgPackets>().0.is_empty());
}

#[test]
fn dungeon_notifications_preserve_order_repetition_and_full_words_in_sole_fifo() {
    use wrathplus_protocol::wrath_lfg::dungeon::Packet as Dungeon;
    let mut app = App::new();
    app.add_plugins(UiLfgPlugin);
    let mut adapter = WrathEventAdapter::new();
    let fixtures = [
        (
            0x0293,
            vec![0x56, 0x34, 0x12, 0xab],
            Dungeon::OfferContinue { entry: 0xab123456 },
        ),
        (
            0x0200,
            vec![0xef, 0xcd, 0xab, 0x80],
            Dungeon::TeleportDenied { reason: 0x80abcdef },
        ),
        (0x0398, vec![], Dungeon::Disabled),
        (0x0293, vec![0; 4], Dungeon::OfferContinue { entry: 0 }),
        (0x0200, vec![0; 4], Dungeon::TeleportDenied { reason: 0 }),
    ];
    // Repeated disabled/offer packets are receipts, not an instruction to clear
    // or deduplicate the capture queue. Only App's later state owner interprets them.
    for _ in 0..2 {
        for (opcode, body, _) in &fixtures {
            let packet = WrathServerPacket {
                opcode: *opcode,
                body: body.clone(),
            };
            let WrathDispatch::Handled { session_events, .. } =
                adapter.dispatch_packet(&packet, 1).unwrap()
            else {
                panic!("dungeon notification unhandled")
            };
            assert_eq!(session_events.len(), 1);
            crate::net::handlers::dispatch(app.world_mut(), session_events);
        }
    }
    let queue = &app.world().resource::<NativeLfgPackets>().0;
    assert_eq!(queue.len(), 10);
    for (actual, (_, _, expected)) in queue.iter().zip(fixtures.iter().cycle()) {
        assert_eq!(actual, &Packet::Dungeon(expected.clone()));
    }
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<NativeLfgPackets>().0.is_empty());
}

#[test]
fn malformed_dungeon_notifications_are_unhandled_without_fifo_publication() {
    let mut app = App::new();
    app.add_plugins(UiLfgPlugin);
    let mut adapter = WrathEventAdapter::new();
    for (opcode, full) in [
        (0x0293, vec![1, 2, 3, 255]),
        (0x0200, vec![255; 4]),
        (0x0398, vec![]),
    ] {
        let mut invalid: Vec<Vec<u8>> = (0..full.len()).map(|cut| full[..cut].to_vec()).collect();
        let mut trailing = full;
        trailing.push(0);
        invalid.push(trailing);
        for body in invalid {
            let packet = WrathServerPacket { opcode, body };
            assert!(
                matches!(adapter.dispatch_packet(&packet, 1).unwrap(), WrathDispatch::Unhandled { opcode: actual } if actual == opcode)
            );
        }
    }
    assert!(app.world().resource::<NativeLfgPackets>().0.is_empty());
}

fn boot_fixture(active: u8, reason: &str) -> (u16, Vec<u8>, Packet) {
    use wrathplus_protocol::wrath_lfg::boot::Boot;
    let mut body = vec![active, 128, 255];
    body.extend(0xf102030405060708u64.to_le_bytes());
    for word in [u32::MAX, 0x80000000, 0, 0x11223344] {
        body.extend(word.to_le_bytes());
    }
    body.extend(reason.as_bytes());
    body.push(0);
    (
        0x036d,
        body,
        Packet::Boot(Boot {
            in_progress: active,
            did_vote: 128,
            agree: 255,
            victim: 0xf102030405060708,
            votes: u32::MAX,
            agrees: 0x80000000,
            seconds_left: 0,
            votes_needed: 0x11223344,
            reason: reason.into(),
        }),
    )
}

fn completion_fixture(count: u8) -> (u16, Vec<u8>, Packet) {
    use wrathplus_protocol::wrath_lfg::{completion::Reward, RewardItem};
    let mut body = vec![0x56, 0x34, 0x12, 0xab, 0xef, 0xcd, 0xab, 0xfe, 255];
    for word in [0x80000000u32, u32::MAX, 0, 0x11223344, 0x55667788] {
        body.extend(word.to_le_bytes());
    }
    body.push(count);
    let mut items = Vec::new();
    for n in 0..u32::from(count) {
        for word in [0x80000000 | n, u32::MAX - n, n] {
            body.extend(word.to_le_bytes());
        }
        items.push(RewardItem {
            item: 0x80000000 | n,
            display: u32::MAX - n,
            count: n,
        });
    }
    (
        0x01ff,
        body,
        Packet::Completion(Reward {
            random_entry: 0xab123456,
            actual_entry: 0xfeabcdef,
            done: 255,
            extra: 0x80000000,
            money: u32::MAX,
            xp: 0,
            variable_money: 0x11223344,
            variable_xp: 0x55667788,
            items,
        }),
    )
}

#[test]
fn boot_and_completion_reach_sole_fifo_losslessly_without_lifecycle_inference() {
    let mut app = App::new();
    app.add_plugins(UiLfgPlugin);
    let mut adapter = WrathEventAdapter::new();
    let fixtures = [
        boot_fixture(128, "raison é"),
        completion_fixture(6),
        boot_fixture(0, &"x".repeat(1024)),
        completion_fixture(255),
        completion_fixture(0),
        boot_fixture(0, ""),
    ];
    for _ in 0..2 {
        for (opcode, body, _) in &fixtures {
            let packet = WrathServerPacket {
                opcode: *opcode,
                body: body.clone(),
            };
            let WrathDispatch::Handled { session_events, .. } =
                adapter.dispatch_packet(&packet, 1).unwrap()
            else {
                panic!("L4b receipt unhandled")
            };
            assert_eq!(session_events.len(), 1);
            crate::net::handlers::dispatch(app.world_mut(), session_events);
        }
    }
    let queue = &app.world().resource::<NativeLfgPackets>().0;
    assert_eq!(queue.len(), 12);
    for (actual, (_, _, expected)) in queue.iter().zip(fixtures.iter().cycle()) {
        assert_eq!(actual, expected);
    }
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<NativeLfgPackets>().0.is_empty());
}

#[test]
fn malformed_boot_and_completion_never_publish_partial_receipts() {
    let mut app = App::new();
    app.add_plugins(UiLfgPlugin);
    let mut adapter = WrathEventAdapter::new();
    for (opcode, body, _) in [boot_fixture(0, "raison é"), completion_fixture(6)] {
        let mut invalid: Vec<Vec<u8>> = (0..body.len()).map(|cut| body[..cut].to_vec()).collect();
        let mut trailing = body;
        trailing.push(0);
        invalid.push(trailing);
        for body in invalid {
            let packet = WrathServerPacket { opcode, body };
            assert!(
                matches!(adapter.dispatch_packet(&packet, 1).unwrap(), WrathDispatch::Unhandled { opcode: actual } if actual == opcode)
            );
        }
    }
    // Count says seven rows but only six complete rows are present.
    let (opcode, mut body, _) = completion_fixture(6);
    body[29] = 7;
    assert!(matches!(
        adapter
            .dispatch_packet(&WrathServerPacket { opcode, body }, 1)
            .unwrap(),
        WrathDispatch::Unhandled { .. }
    ));
    assert!(app.world().resource::<NativeLfgPackets>().0.is_empty());
}
