//! Independent build-12340 bytes: no update-object encoder or projected descriptor fixtures.
use crate::events::SessionEvent;
use crate::types::{FieldLayout, ObjectFields};
use crate::wrath_events::{WrathDispatch, WrathEventAdapter, SMSG_UPDATE_OBJECT};

fn guid(out: &mut Vec<u8>, value: u8) {
    match value {
        8 => out.extend([0xc1, value, 0x30, 0xf1]),
        9 => out.extend([0x81, value, 0x40]),
        10 => out.extend([0xc1, value, 0x10, 0xf1]),
        _ => out.extend([1, value]),
    }
}
fn descriptors(out: &mut Vec<u8>, pairs: &[(usize, u32)]) {
    let count = pairs.last().map_or(0, |p| p.0 / 32 + 1);
    out.push(count as u8);
    for word in 0..count {
        let mask = pairs
            .iter()
            .filter(|p| p.0 / 32 == word)
            .fold(0u32, |m, p| m | (1 << (p.0 % 32)));
        out.extend(mask.to_le_bytes());
    }
    for (_, value) in pairs {
        out.extend(value.to_le_bytes());
    }
}
fn create(id: u8, kind: u8, pairs: &[(usize, u32)]) -> Vec<u8> {
    let mut out = 1u32.to_le_bytes().to_vec();
    out.push(2);
    guid(&mut out, id);
    out.push(kind);
    if kind == 1 || kind == 2 {
        out.extend(0u16.to_le_bytes());
    } else {
        out.extend(0x40u16.to_le_bytes());
        for value in [1.0f32, 2.0, 3.0, 4.0] {
            out.extend(value.to_le_bytes());
        }
    }
    descriptors(&mut out, pairs);
    out
}
fn delta(id: u8, pairs: &[(usize, u32)]) -> Vec<u8> {
    let mut out = 1u32.to_le_bytes().to_vec();
    out.push(0);
    guid(&mut out, id);
    descriptors(&mut out, pairs);
    out
}
fn dispatch(adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) -> Vec<SessionEvent> {
    match adapter.dispatch(opcode, body, 0).unwrap() {
        WrathDispatch::Handled { session_events, .. } => session_events,
        _ => panic!("expected handled opcode {opcode:x}"),
    }
}
fn created(events: &[SessionEvent]) -> ObjectFields {
    match &events[0] {
        SessionEvent::ObjectCreate { fields, .. } | SessionEvent::ItemCreate { fields, .. } => {
            fields.clone()
        }
        _ => panic!("expected descriptor create"),
    }
}

#[test]
fn byte_stream_native_create_values_aura_remove_recreate() {
    let mut adapter = WrathEventAdapter::new();
    let mut player = created(&dispatch(
        &mut adapter,
        SMSG_UPDATE_OBJECT,
        &create(
            7,
            4,
            &[
                (24, 100),
                (25, 721),
                (31, 659),
                (32, 200),
                (33, 1000),
                (39, 1250),
                (278, 783),
                (279, 1),
                (280, 3 | (65535 << 16)),
                (281, 70 << 16),
                (282, 12345),
                (1170, 123456),
            ],
        ),
    ));
    assert_eq!(player.layout(), FieldLayout::Wrath12340);
    assert_eq!(player.unit_health(), Some(100));
    assert_eq!(player.wrath_powers().unwrap()[6], (659, 1250));
    let quest = player.player_quest_log(24).unwrap();
    assert_eq!(quest.counters, [3, 65535, 0, 70]);
    assert_eq!(quest.timer, 12345);
    assert_eq!(player.player_money(), Some(123456));
    let unit = created(&dispatch(
        &mut adapter,
        SMSG_UPDATE_OBJECT,
        &create(8, 3, &[(24, 91), (32, 123), (82, 0x1081)]),
    ));
    assert_eq!(unit.layout(), FieldLayout::Wrath12340);
    assert_eq!(unit.unit_npc_flags(), 0x1081);
    let item = created(&dispatch(
        &mut adapter,
        SMSG_UPDATE_OBJECT,
        &create(9, 1, &[(14, 7), (60, 31), (61, 47)]),
    ));
    assert_eq!(item.layout(), FieldLayout::Wrath12340);
    assert_eq!(item.item_durability(), Some(31));
    let go = created(&dispatch(
        &mut adapter,
        SMSG_UPDATE_OBJECT,
        &create(10, 5, &[(14, 0x12340009), (17, 0x07060502)]),
    ));
    assert_eq!(go.layout(), FieldLayout::Wrath12340);
    assert_eq!(go.gameobject_state(), Some(2));
    let events = dispatch(
        &mut adapter,
        SMSG_UPDATE_OBJECT,
        &delta(7, &[(24, 0), (31, 0)]),
    );
    let SessionEvent::ObjectValues { fields, .. } = &events[0] else {
        panic!()
    };
    assert_eq!(fields.layout(), FieldLayout::Wrath12340);
    player.merge(fields.clone());
    assert_eq!(player.unit_health(), Some(0));
    assert_eq!(player.unit_max_health(), Some(200));
    assert_eq!(player.wrath_powers().unwrap()[6], (0, 1250));
    assert!(dispatch(
        &mut adapter,
        SMSG_UPDATE_OBJECT,
        &delta(7, &[(24, 0), (31, 0)])
    )
    .is_empty());
    // Aura slot 55 exceeds the legacy buff half; caster omitted, timed positive aura.
    let mut aura = vec![1, 7, 55];
    aura.extend(123u32.to_le_bytes());
    aura.extend([0x38, 10, 3]);
    aura.extend(30000u32.to_le_bytes());
    aura.extend(29000u32.to_le_bytes());
    let events = dispatch(&mut adapter, 0x495, &aura);
    assert_eq!(events.len(), 1);
    let SessionEvent::UnitAuras {
        auras,
        updated_slots,
        ..
    } = &events[0]
    else {
        panic!()
    };
    assert_eq!(auras[0].slot, 55);
    assert_eq!(auras[0].stacks, 3);
    assert_eq!(auras[0].remaining_ms, Some(29000));
    assert_eq!(updated_slots, &[55]);
    let events = dispatch(&mut adapter, 0x496, &[1, 7, 55, 0, 0, 0, 0]);
    assert!(matches!(&events[..],[SessionEvent::UnitAuras {auras,..}] if auras.is_empty()));
    dispatch(&mut adapter, 0x496, &aura);
    let mut remove = 1u32.to_le_bytes().to_vec();
    remove.push(4);
    remove.extend(1u32.to_le_bytes());
    guid(&mut remove, 7);
    dispatch(&mut adapter, SMSG_UPDATE_OBJECT, &remove);
    assert_eq!(adapter.object_type(7), None);
    let player = created(&dispatch(
        &mut adapter,
        SMSG_UPDATE_OBJECT,
        &create(7, 4, &[(24, 77)]),
    ));
    assert_eq!(player.layout(), FieldLayout::Wrath12340);
    assert_eq!(player.unit_health(), Some(77));
    assert_eq!(player.player_money(), Some(0));
    let events = dispatch(&mut adapter, 0x496, &[1, 7]);
    assert!(matches!(&events[..],[SessionEvent::UnitAuras {auras,..}] if auras.is_empty()));
}
