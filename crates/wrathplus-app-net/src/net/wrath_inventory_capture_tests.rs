use super::*;
use crate::net::test_support::inventory::{captured, dispatch};
use wrathplus_protocol::wrath_events::WrathEventAdapter;
use wrathplus_protocol::wrath_inventory::decode;

fn packet(opcode: u16, body: &[u8]) -> Packet {
    match decode(opcode, body).unwrap() {
        SessionEvent::WrathInventory { packet } => packet,
        other => panic!("{other:?}"),
    }
}

#[test]
fn ordered_receipts_keep_duplicates_across_opcodes_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let guid = 0x0400_0000_0000_0102u64.to_le_bytes();
    let rows: [(u16, &[u8]); 4] = [(0x4d6, &[4]), (0x0ae, &guid), (0x4d6, &[0]), (0x4d6, &[4])];
    for (opcode, body) in rows {
        dispatch(&mut app, &mut adapter, opcode, body);
        for end in 0..body.len() {
            dispatch(&mut app, &mut adapter, opcode, &body[..end]);
        }
        dispatch(&mut app, &mut adapter, opcode, &[body, &[0]].concat());
    }
    assert_eq!(
        captured(&app),
        rows.iter().map(|(o, b)| packet(*o, b)).collect::<Vec<_>>()
    );
}

/// A letter's text reaches the item-text reader's cache, not the capture.
#[test]
fn letter_texts_go_to_the_reader() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let mut body = vec![0u8];
    body.extend(0x4000_0000_0000_0042u64.to_le_bytes());
    body.extend(b"Dear friend\0");
    dispatch(&mut app, &mut adapter, 0x244, &body);
    dispatch(&mut app, &mut adapter, 0x244, &[1]);
    assert!(captured(&app).is_empty());
    let mut letters = app.world_mut().resource_mut::<crate::app_core::interaction::LetterTexts>();
    assert_eq!(letters.text(0x4000_0000_0000_0042).as_deref(), Some("Dear friend"));
}
