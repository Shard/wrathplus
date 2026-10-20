use super::*;
use crate::ui_native_pet_name::{NativePetNamePackets, UiNativePetNamePlugin};
use benilla_protocol::{wrath_pet_name::Response, SessionEvent};
use bevy::ecs::system::RunSystemOnce;
fn response(name: &[u8]) -> Response {
    Response {
        pet_number: 7,
        name: name.into(),
        timestamp: 99,
        declined_flag: 0,
        declined_names: None,
    }
}
pub(crate) fn app() -> App {
    let mut app = App::new();
    app.init_resource::<NameCache>()
        .add_plugins(UiNativePetNamePlugin);
    app
}
pub(crate) fn packet(app: &mut App, name: &[u8]) {
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::WrathPetName {
            response: response(name),
        }],
    );
}
pub(crate) fn drain(app: &mut App) {
    app.world_mut()
        .run_system_once(net::drain_native_pet_names)
        .unwrap();
}
#[test]
fn fifo_named_duplicate_last_wins_and_uses_established_display_conversion() {
    let mut app = app();
    for name in [b"First".as_slice(), b"First", b"Last\xff"] {
        packet(&mut app, name);
    }
    drain(&mut app);
    let names = app.world().resource::<NameCache>();
    assert_eq!(names.pets.get(7).map(String::as_str), Some("Last\u{fffd}"));
    assert_eq!(names.generation(), 3);
    assert!(app.world().resource::<NativePetNamePackets>().0.is_empty());
}
#[test]
fn empty_evicts_answer_and_pending_allows_later_explicit_requery_without_auto_retry() {
    let mut app = app();
    let (tx, rx) = crossbeam_channel::unbounded();
    let commands = crate::net::NetCommands(tx);
    assert!(app
        .world()
        .resource::<NameCache>()
        .resolve_pet(7, 42, &commands)
        .is_none());
    assert_eq!(rx.try_iter().count(), 1);
    packet(&mut app, b"");
    drain(&mut app);
    assert!(!app.world().resource::<NameCache>().pets.is_pending(7));
    assert!(rx.try_recv().is_err());
    assert!(app
        .world()
        .resource::<NameCache>()
        .resolve_pet(7, 42, &commands)
        .is_none());
    assert_eq!(rx.try_iter().count(), 1);
    packet(&mut app, b"Rex");
    drain(&mut app);
    assert!(app.world().resource::<NameCache>().pets.answered(7));
    packet(&mut app, b"");
    drain(&mut app);
    let names = app.world().resource::<NameCache>();
    assert!(!names.pets.answered(7));
    assert!(!names.pets.is_pending(7));
}
#[test]
fn queued_response_disconnect_cannot_repopulate_cache() {
    let mut app = app();
    packet(&mut app, b"stale");
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "synthetic".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    app.world_mut().resource_mut::<NameCache>().clear_pending();
    drain(&mut app);
    assert!(!app.world().resource::<NameCache>().pets.answered(7));
}
