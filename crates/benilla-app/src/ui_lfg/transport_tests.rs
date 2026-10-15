//! Decrypted native packets reach the sole lossless FIFO without a proposal reducer.
use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use benilla_protocol::wrath_world::WrathServerPacket;

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
            end: benilla_protocol::SessionEnd::Lost,
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
