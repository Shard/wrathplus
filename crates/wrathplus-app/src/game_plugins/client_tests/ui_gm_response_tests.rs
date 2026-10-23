//! Whole-client tests of `wrathplus_app_ui::ui_gm_response`, the sole `WrathHelp` owner. They build the client
//! with `headless_client`, so they live in the root layer.

use bevy::prelude::*;
use wrathplus_app_ui::ui_gm_response::GmResponseEvents;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::SessionEventKind;
use wrathplus_ui::script::ScriptValue;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn owed(app: &App) -> Vec<(&'static str, Vec<ScriptValue>)> {
    app.world().resource::<GmResponseEvents>().0.clone()
}

/// `0x4ef` fires `GMRESPONSE_RECEIVED(text, parts joined)`; `0x4f1` with 0 fires a bare
/// `UPDATE_TICKET` and with 1 takes the survey path (no queued event); `0x3c8` fires nothing; the
/// session end drops what is owed (`FUN_005ad240`).
#[test]
fn the_gm_response_and_its_status_owe_the_references_events() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathHelp];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("ui_gm_response::on_help"), "{owners:?}");

    let mut adapter = WrathEventAdapter::new();
    let mut body = Vec::new();
    body.extend(7u32.to_le_bytes());
    body.extend(9u32.to_le_bytes());
    body.extend(b"my ticket\0Hello \0there\0\0!\0");
    dispatch(&mut app, &mut adapter, 0x4ef, &body);
    dispatch(&mut app, &mut adapter, 0x4f1, &[0]);
    dispatch(&mut app, &mut adapter, 0x4f1, &[1]);
    dispatch(&mut app, &mut adapter, 0x3c8, &[0]);
    assert_eq!(
        owed(&app),
        vec![
            (
                "GMRESPONSE_RECEIVED",
                vec![
                    ScriptValue::Str("my ticket".into()),
                    ScriptValue::Str("Hello there!".into()),
                ],
            ),
            ("UPDATE_TICKET", vec![]),
        ]
    );

    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![wrathplus_protocol::SessionEvent::Disconnected {
            reason: "test".into(),
            end: wrathplus_protocol::SessionEnd::Lost,
        }],
    );
    assert!(owed(&app).is_empty());
}
