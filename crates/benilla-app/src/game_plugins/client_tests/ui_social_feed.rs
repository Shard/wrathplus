//! Whole-client tests of `crate::ui_social::feed`, from `ui_social/feed.rs` (`native_tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::names::NameCache;
use crate::net::NetCommands;
use crate::ui_native_who::NativeWhoPackets;
use crate::ui_social::feed::native_tests::entries;
use crate::ui_social::feed::*;
use crate::ui_social::SocialState;
use benilla_protocol::wrath_who::Packet;
use benilla_ui::script::UiScript;
use bevy::prelude::*;

fn connected() -> benilla_protocol::SessionEvent {
    benilla_protocol::SessionEvent::Connected {
        self_guid: 123,
        name: "Synthetic".into(),
        billing_time_rested: 0,
        tutorial_flags: None,
        addon_info: None,
    }
}

fn packet(app: &mut App, name: &str) {
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![benilla_protocol::SessionEvent::WrathWho {
            packet: Packet::Who {
                total: 88,
                entries: entries(name, 11),
            },
        }],
    );
}

#[test]
fn real_handlers_clear_capture_and_pending_receipts_before_fresh_feed() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    app.insert_resource(crate::net::SessionDialect::Wrath12340);
    packet(&mut app, "OldPending");
    app.world_mut()
        .run_system_once(crate::ui_social::native_who::drain)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<SocialState>()
            .pending_native_who
            .len(),
        1
    );
    packet(&mut app, "OldCapture");
    crate::net::handlers::dispatch(app.world_mut(), vec![connected()]);
    assert!(app
        .world()
        .resource::<SocialState>()
        .pending_native_who
        .is_empty());
    assert!(app.world().resource::<NativeWhoPackets>().0.is_empty());
    packet(&mut app, "Fresh");
    let social = app.world_mut().remove_resource::<SocialState>().unwrap();
    let packets = app
        .world_mut()
        .remove_resource::<NativeWhoPackets>()
        .unwrap();
    // Run only the real social systems in a CPU app, not renderer startup.
    let mut cpu = App::new();
    let (tx, _rx) = crossbeam_channel::unbounded();
    cpu.insert_resource(social)
        .insert_resource(packets)
        .init_resource::<NameCache>()
        .init_resource::<crate::ui_chat::ChatLog>()
        .init_resource::<crate::sound::MessageSounds>()
        .insert_resource(NetCommands(tx))
        .insert_non_send_resource(UiScript::new().unwrap())
        .add_systems(
            Update,
            (crate::ui_social::native_who::drain, feed_social).chain(),
        );
    cpu.update();
    let script = cpu.world().non_send_resource::<UiScript>();
    let name: String = script.lua().load("return GetWhoInfo(1)").eval().unwrap();
    assert_eq!(name, "Fresh");
    let social = cpu.world_mut().remove_resource::<SocialState>().unwrap();
    let packets = cpu
        .world_mut()
        .remove_resource::<NativeWhoPackets>()
        .unwrap();
    app.insert_resource(social).insert_resource(packets);
    packet(&mut app, "DisconnectPending");
    app.world_mut()
        .run_system_once(crate::ui_social::native_who::drain)
        .unwrap();
    packet(&mut app, "DisconnectCapture");
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![benilla_protocol::SessionEvent::Disconnected {
            reason: "synthetic".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    let social = app.world().resource::<SocialState>();
    assert!(social.pending_native_who.is_empty());
    assert!(social.who.is_empty());
    assert!(social.native_who_rows.is_empty());
    assert!(app.world().resource::<NativeWhoPackets>().0.is_empty());
}
