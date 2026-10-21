use super::*;
use crate::net::test_support::mirror_image::dispatch;
use benilla_protocol::wrath_events::WrathEventAdapter;

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
