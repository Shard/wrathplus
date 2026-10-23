//! Whole-client tests of `SMSG_CHAT_PLAYER_AMBIGUOUS` (`0x32d`) through `crate::ui_chat`'s
//! whisper-refusal handler. They build the client with `headless_client`, so they live in the root
//! layer.

use crate::app_core::errors::{FillArg, UiErrorKeys};
use bevy::prelude::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::SessionEventKind;

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

/// 12340's `0x32d` case shows message `0x1fa`, `ERR_CHAT_PLAYER_AMBIGUOUS_S`, with the name; a
/// malformed frame (no terminator, a trailing byte) shows nothing.
#[test]
fn an_ambiguous_whisper_name_shows_the_references_line_and_has_one_owner() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathChatNotice];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("on_whisper_refusal"), "{owners:?}");

    let mut adapter = WrathEventAdapter::new();
    dispatch(&mut app, &mut adapter, 0x32d, b"Bo");
    dispatch(&mut app, &mut adapter, 0x32d, b"Bo\0\0");
    dispatch(&mut app, &mut adapter, 0x32d, b"Bo\0");
    let errors = &app.world().resource::<UiErrorKeys>().0;
    let ambiguous: Vec<_> = errors
        .iter()
        .filter(|e| e.key == "ERR_CHAT_PLAYER_AMBIGUOUS_S")
        .collect();
    assert_eq!(ambiguous.len(), 1, "one line for the one well-formed reply");
    assert!(
        matches!(ambiguous[0].args.as_slice(), [FillArg::S(name)] if name == "Bo"),
        "{:?}",
        ambiguous[0].args
    );
}
