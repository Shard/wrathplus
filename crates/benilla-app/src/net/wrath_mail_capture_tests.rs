use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn fifo_retains_duplicate_mailboxes_rejects_partial_packets_and_skips_results() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let result = [255; 12]; // A send-mail result: now the shared event's, never captured here.
    let mailbox = [255; 8];
    for (opcode, body) in [
        (0x297, mailbox.as_slice()),
        (0x239, result.as_slice()),
        (0x297, [0; 8].as_slice()),
        (0x297, mailbox.as_slice()),
    ] {
        dispatch(&mut app, &mut adapter, opcode, body);
    }
    for end in 0..mailbox.len() {
        dispatch(&mut app, &mut adapter, 0x297, &mailbox[..end]);
    }
    dispatch(&mut app, &mut adapter, 0x297, &[0; 9]);
    assert_eq!(
        app.world()
            .resource::<NativeMailMutationPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            Packet::ShowMailbox { mailbox: u64::MAX },
            Packet::ShowMailbox { mailbox: 0 },
            Packet::ShowMailbox { mailbox: u64::MAX },
        ]
    );
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app
        .world()
        .resource::<NativeMailMutationPackets>()
        .0
        .is_empty());
}
