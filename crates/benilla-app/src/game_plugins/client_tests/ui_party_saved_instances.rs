//! Whole-client tests of `ui_party`'s saved-instance receipts
//! (`app_core::group::NativeSavedInstances`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::handlers::dispatch;
use crate::ui_party::GroupState;
use benilla_protocol::wrath_instance::RaidInstanceInfo;
use benilla_protocol::{
    wrath_events::{WrathDispatch, WrathEventAdapter},
    wrath_instance::SavedInstance,
    wrath_world::WrathServerPacket,
    SessionEnd, SessionEvent,
};

fn receipt(app: &mut bevy::prelude::App, entries: &[SavedInstance]) {
    let mut body = (entries.len() as u32).to_le_bytes().to_vec();
    for row in entries {
        body.extend(row.map.to_le_bytes());
        body.extend(row.difficulty.to_le_bytes());
        body.extend(row.instance_guid.to_le_bytes());
        body.extend([row.raw_flag, row.extended]);
        body.extend(row.reset_seconds.to_le_bytes());
    }
    let WrathDispatch::Handled { session_events, .. } = WrathEventAdapter::new()
        .dispatch_packet(
            &WrathServerPacket {
                opcode: 0x2cc,
                body,
            },
            1,
        )
        .unwrap()
    else {
        panic!("saved-instance opcode not handled");
    };
    assert_eq!(session_events.len(), 1);
    dispatch(app.world_mut(), session_events);
}

#[test]
fn native_bytes_keep_raw_rows_duplicates_and_each_empty_receipt() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let row = SavedInstance {
        map: u32::MAX,
        difficulty: u32::MAX,
        instance_guid: u64::MAX,
        raw_flag: 254,
        extended: 128,
        reset_seconds: 0x8000_0001,
    };
    let other = SavedInstance {
        map: 1,
        ..row.clone()
    };
    receipt(&mut app, &[row.clone(), other.clone(), row.clone()]);
    receipt(&mut app, &[]);
    receipt(&mut app, &[]);
    let group = app.world().resource::<GroupState>();
    let native = &group.wrath_saved_instances;
    assert_eq!(native.receipts.len(), 3);
    assert_eq!(native.receipts[0].entries, [row.clone(), other, row]);
    assert!(native.receipts[1].entries.is_empty());
    assert!(native.receipts[2].entries.is_empty());
    assert_eq!(native.snapshot, Some(RaidInstanceInfo { entries: vec![] }));
}

#[test]
fn group_leave_keeps_native_lockouts_but_disconnect_clears_snapshot_and_receipts() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    assert!(app
        .world()
        .resource::<GroupState>()
        .wrath_saved_instances
        .snapshot
        .is_none());
    receipt(&mut app, &[]);
    app.world_mut().resource_mut::<GroupState>().leave_group();
    let group = app.world().resource::<GroupState>();
    assert!(group.wrath_saved_instances.snapshot.is_some());
    assert_eq!(group.wrath_saved_instances.receipts.len(), 1);
    dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "fixture".into(),
            end: SessionEnd::Lost,
        }],
    );
    let group = app.world().resource::<GroupState>();
    assert!(group.wrath_saved_instances.snapshot.is_none());
    assert!(group.wrath_saved_instances.receipts.is_empty());
    receipt(&mut app, &[]);
    assert_eq!(
        app.world()
            .resource::<GroupState>()
            .wrath_saved_instances
            .receipts
            .len(),
        1
    );
}
