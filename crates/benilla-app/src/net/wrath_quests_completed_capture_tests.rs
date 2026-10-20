use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

pub(crate) fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x501, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn ordered_replies_keep_empty_duplicates_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let body = [2, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255];
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
            .resource::<NativeQuestsCompletedPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            Packet {
                quest_ids: vec![u32::MAX, u32::MAX]
            },
            Packet { quest_ids: vec![] },
            Packet {
                quest_ids: vec![u32::MAX, u32::MAX]
            },
        ]
    );
}
