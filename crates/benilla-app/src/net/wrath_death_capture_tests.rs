use super::*;
use crate::net::test_support::death::{captured, dispatch};
use benilla_protocol::wrath_events::WrathEventAdapter;

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
