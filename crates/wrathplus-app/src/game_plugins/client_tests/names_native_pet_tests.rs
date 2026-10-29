//! Whole-client tests of `crate::names`, from `names_native_pet_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::names::native_pet_tests::{drain, packet};
use crate::names::*;
use crate::ui_native_pet_name::NativePetNamePackets;
use bevy::prelude::*;
use wrathplus_protocol::SessionEvent;

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
        .init_resource::<Seen>();
    net::register(&mut app);
    app.add_systems(Update, observe.in_set(crate::app_core::schedule::UiFeed));
    app.update();
    assert_eq!(
        app.world().resource::<Seen>().0,
        vec![Some("fresh-session".into())]
    );
    assert!(app.world().resource::<NativePetNamePackets>().0.is_empty());
}
