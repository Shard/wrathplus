use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

pub(crate) fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x402, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

#[test]
fn ordered_replies_keep_zero_duplicates_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let body = [255; 68];
    dispatch(&mut app, &mut adapter, &body);
    dispatch(&mut app, &mut adapter, &[0; 68]);
    dispatch(&mut app, &mut adapter, &body);
    for end in 0..body.len() {
        dispatch(&mut app, &mut adapter, &body[..end]);
    }
    let mut trailing = body.to_vec();
    trailing.push(0);
    dispatch(&mut app, &mut adapter, &trailing);
    assert_eq!(
        app.world()
            .resource::<NativeMirrorImagePackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            benilla_protocol::wrath_mirror_image::decode(&[255; 68]).unwrap(),
            benilla_protocol::wrath_mirror_image::decode(&[0; 68]).unwrap(),
            benilla_protocol::wrath_mirror_image::decode(&[255; 68]).unwrap(),
        ]
    );
}
