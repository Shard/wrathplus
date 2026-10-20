use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

pub(crate) fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

pub(crate) fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeDeathPackets>()
        .0
        .iter()
        .cloned()
        .collect()
}

#[test]
fn the_release_location_feeds_the_map_and_map_minus_one_clears_it() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let mut body = 1u32.to_le_bytes().to_vec();
    for f in [10.0f32, 20.0, 30.0] {
        body.extend(f.to_le_bytes());
    }
    dispatch(&mut app, &mut adapter, 0x378, &body);
    assert_eq!(app.world().resource::<DeathRelease>().0, Some((1, 10.0, 20.0)));
    assert!(captured(&app).is_empty(), "the location does not enter the capture");
    let mut clear = (-1i32).to_le_bytes().to_vec();
    clear.extend([0u8; 12]);
    dispatch(&mut app, &mut adapter, 0x378, &clear);
    assert_eq!(app.world().resource::<DeathRelease>().0, None);
}
