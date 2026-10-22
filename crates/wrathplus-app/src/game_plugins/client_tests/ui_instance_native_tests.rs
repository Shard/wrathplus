//! Whole-client tests of `crate::ui_instance`'s native handler. They build the client with
//! `headless_client`, so they live in the root layer.

use crate::ui_instance::{InstanceState, Pending};
use wrathplus_protocol::wrath_instance_control::Packet;

#[test]
fn the_native_packets_reach_the_one_consumer() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let packets = [
        Packet::Reset { map: 36 },
        Packet::Ownership { raw_value: 1 },
        Packet::LastInstance { map: 36 },
        Packet::RaidMessage {
            kind: 4,
            map: 533,
            difficulty: 1,
            seconds: 60,
            welcome_flags: Some([0, 1]),
        },
    ];
    let events = packets
        .into_iter()
        .map(|packet| wrathplus_protocol::SessionEvent::WrathInstanceControl { packet })
        .collect();
    crate::net::handlers::dispatch(app.world_mut(), events);
    bevy::ecs::system::RunSystemOnce::run_system_once(
        app.world_mut(),
        crate::ui_instance::net::take_native,
    )
    .unwrap();
    let (owns, last, queued) = app.world().resource::<InstanceState>().snapshot();
    assert!(owns);
    assert_eq!(last, [36]);
    assert_eq!(queued.len(), 2, "the reset line, then the welcome");
    assert!(matches!(
        queued[1],
        Pending::Welcome {
            difficulty: 1,
            extended: true,
            ..
        }
    ));
}
