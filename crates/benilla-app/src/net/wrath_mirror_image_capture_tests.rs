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

#[test]
fn a_mirror_image_unit_asks_for_its_look_once_per_object() {
    use crate::net::{ClientCommand, Guid, NetCommands, ObjectStore};
    use benilla_protocol::types::Field;
    use benilla_protocol::ObjectFields;
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.insert_resource(NetCommands(tx));
    register(&mut app);
    let unit = |flags2: u32| {
        ObjectStore(ObjectFields::from_fields(&[(Field::ObjectType, 0x09), (Field::UnitFlags2, flags2)],
        ))
    };
    let image = app.world_mut().spawn((Guid(7), unit(0x10))).id();
    let plain = app.world_mut().spawn((Guid(8), unit(0))).id();
    app.update();
    let sent: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(
            sent[..],
            [ClientCommand::NativeMirrorImageQuery { guid: 7 }]
        ),
        "{sent:?}"
    );
    // Another delta on the same object stays silent; the flag arriving later asks.
    app.world_mut().entity_mut(image).insert(unit(0x11));
    app.world_mut().entity_mut(plain).insert(unit(0x10));
    app.update();
    let sent: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(
            sent[..],
            [ClientCommand::NativeMirrorImageQuery { guid: 8 }]
        ),
        "{sent:?}"
    );
    // A new object asks again.
    app.world_mut().despawn(image);
    app.update();
    app.world_mut().spawn((Guid(7), unit(0x10)));
    app.update();
    assert_eq!(rx.try_iter().count(), 1);
}
