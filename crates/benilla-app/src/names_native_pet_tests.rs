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
fn app() -> App {
    let mut app = App::new();
    app.init_resource::<NameCache>()
        .add_plugins(UiNativePetNamePlugin);
    app
}
fn packet(app: &mut App, name: &[u8]) {
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::WrathPetName {
            response: response(name),
        }],
    );
}
fn drain(app: &mut App) {
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
    app.world_mut()
        .resource_mut::<NameCache>()
        .set_dialect(SessionDialect::Wrath12340);
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
#[test]
fn queued_response_world_entry_cannot_repopulate_previous_session() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    packet(&mut app, b"stale");
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Connected {
            self_guid: 123,
            name: "Synthetic".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        }],
    );
    drain(&mut app);
    assert!(!app.world().resource::<NameCache>().pets.answered(7));
    assert!(app.world().resource::<NativePetNamePackets>().0.is_empty());
}

/// Use the registered schedule, not run_system_once: consumers see this frame's
/// answer after Net and before UiFeed, including a reply after the login reset.
#[test]
fn scheduled_drain_exposes_fresh_post_connected_name_to_ui_feed_same_frame() {
    #[derive(Resource, Default)]
    struct Seen(Vec<Option<String>>);
    fn observe(names: Res<NameCache>, mut seen: ResMut<Seen>) {
        seen.0.push(names.pets.get(7).cloned());
    }
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    packet(&mut app, b"old-session");
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Connected {
            self_guid: 123,
            name: "Synthetic".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        }],
    );
    packet(&mut app, b"fresh-session");
    // Preserve the real Connected-handler result, but run the registered name
    // schedule in a CPU-only app: full client startup requires a RenderDevice.
    let names = app.world_mut().remove_resource::<NameCache>().unwrap();
    let packets = app
        .world_mut()
        .remove_resource::<NativePetNamePackets>()
        .unwrap();
    let mut app = App::new();
    app.insert_resource(names)
        .insert_resource(packets)
        .insert_resource(SessionDialect::Wrath12340)
        .init_resource::<Seen>();
    net::register(&mut app);
    app.add_systems(Update, observe.in_set(crate::ui_script::UiFeed));
    app.update();
    assert_eq!(
        app.world().resource::<Seen>().0,
        vec![Some("fresh-session".into())]
    );
    assert!(app.world().resource::<NativePetNamePackets>().0.is_empty());
}
