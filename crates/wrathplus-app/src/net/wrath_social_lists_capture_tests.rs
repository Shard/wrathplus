use super::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn receipts_queue_in_order_reject_malformed_and_clear_on_both_boundaries() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let mut count = b"Trade\0".to_vec();
    count.push(0x1c);
    count.extend(77u32.to_le_bytes());
    for event in [
        SessionEvent::Connected {
            self_guid: 1,
            name: "t".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        },
        SessionEvent::Disconnected {
            reason: "t".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, 0x3d5, &count);
        dispatch(&mut app, &mut adapter, 0x3d5, &count[..3]);
        dispatch(&mut app, &mut adapter, 0x369, &[1]);
        dispatch(&mut app, &mut adapter, 0x369, &[1, 0]);
        assert_eq!(app.world().resource::<NativeChannelMembers>().0.len(), 1);
        assert_eq!(
            app.world().resource::<NativeRaidBrowser>().0,
            [wrath_raid_browser::Packet::Searching { searching: true }]
        );
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(app.world().resource::<NativeChannelMembers>().0.is_empty());
        assert!(app.world().resource::<NativeRaidBrowser>().0.is_empty());
    }
}
