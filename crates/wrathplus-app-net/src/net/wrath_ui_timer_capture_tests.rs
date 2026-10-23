use super::*;
use crate::net::test_support::ui_timer::dispatch;
use wrathplus_protocol::wrath_events::WrathEventAdapter;

#[test]
fn ordered_replies_keep_zero_duplicates_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let body = [255; 4];
    dispatch(&mut app, &mut adapter, &body);
    dispatch(&mut app, &mut adapter, &[0; 4]);
    dispatch(&mut app, &mut adapter, &body);
    for end in 0..body.len() {
        dispatch(&mut app, &mut adapter, &body[..end]);
    }
    let mut trailing = body.to_vec();
    trailing.push(0);
    dispatch(&mut app, &mut adapter, &trailing);
    assert_eq!(
        app.world()
            .resource::<NativeUiTimerPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            Packet { time: u32::MAX },
            Packet { time: 0 },
            Packet { time: u32::MAX },
        ]
    );
}
