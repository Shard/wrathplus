//! Whole-client tests of `crate::ui_party::net`, from `ui_party/net.rs` (`wrath_raid_tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::SelfGuid;
use crate::ui_party::GroupState;
use benilla_protocol::SessionEvent;

#[test]
fn native_raid_bytes_reach_registered_owner_and_reset_on_uninvite() {
    use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
    use benilla_protocol::wrath_world::WrathServerPacket;
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    app.world_mut().resource_mut::<SelfGuid>().0 = Some(7);
    app.world_mut().resource_mut::<GroupState>().leader = 99;
    let mut adapter = WrathEventAdapter::new();
    for (opcode, body) in [
        (0x322, vec![7, 0, 0, 0, 0, 0, 0, 0]),
        (0x3ae, vec![8, 0, 0, 0, 0, 0, 0, 0, 2]),
        (
            0x321,
            vec![0, 7, 0, 0, 0, 0, 0, 0, 0, 3, 8, 0, 0, 0, 0, 0, 0, 0],
        ),
        (
            0x07f,
            vec![255, 255, 255, 255, b'A', 0, 255, 255, 255, 255, 1, 2, 3, 4],
        ),
        (0x3c6, vec![]),
    ] {
        let WrathDispatch::Handled { session_events, .. } = adapter
            .dispatch_packet(&WrathServerPacket { opcode, body }, 1)
            .unwrap()
        else {
            panic!("native raid opcode not handled");
        };
        assert_eq!(session_events.len(), 1);
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    let group = app.world().resource::<GroupState>();
    assert_eq!((group.ready_check_requests, group.ready_check), (1, 0));
    assert_eq!(group.wrath_raid.ready_initiator, Some(7));
    assert_eq!(group.wrath_raid.ready_answers, [(8, 2)]);
    assert_eq!(group.wrath_raid.target_setter, Some(7));
    assert_eq!(group.raid_targets[3], 8);
    assert_eq!(
        group.wrath_raid.command_result,
        Some((u32::MAX, "A".into(), u32::MAX, 0x04030201))
    );
    assert_eq!(group.wrath_raid.ready_finished, 1);
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::WrathGroup {
            packet: Box::new(benilla_protocol::wrath_group::Packet::Uninvite),
        }],
    );
    let group = app.world().resource::<GroupState>();
    assert!(group.wrath_raid.ready_initiator.is_none());
    assert!(group.wrath_raid.ready_answers.is_empty());
    assert!(group.wrath_raid.command_result.is_none());
    assert_eq!(group.raid_targets, [0; 8]);
}
