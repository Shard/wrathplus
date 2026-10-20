//! Whole-client tests of `crate::net`, from `net/wrath_stream_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.
//!
//! Ordered decrypted packet replay through the real registered app handlers.
//! Synthetic bodies are built independently of the protocol update-object encoder.

use crate::net::{GuidIndex, ObjectStore};
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter, SMSG_UPDATE_OBJECT};
use benilla_protocol::wrath_world::WrathServerPacket;
use benilla_protocol::{ObjectFields, SessionEvent};
use bevy::prelude::*;

struct Replay {
    app: App,
    adapter: WrathEventAdapter,
    sequence: u32,
}

impl Replay {
    fn new() -> Self {
        Self {
            app: crate::game_plugins::schedule_tests::headless_client(),
            adapter: WrathEventAdapter::new(),
            sequence: 0,
        }
    }
    // Each packet's commands are applied before the next packet, as in the live drain.
    fn packet(&mut self, opcode: u16, body: Vec<u8>) -> Vec<SessionEvent> {
        self.sequence += 1;
        let packet = WrathServerPacket { opcode, body };
        let WrathDispatch::Handled { session_events, .. } = self
            .adapter
            .dispatch_packet(&packet, self.sequence)
            .unwrap()
        else {
            panic!("unhandled replay packet {opcode:x}")
        };
        for event in &session_events {
            match event {
                SessionEvent::ObjectCreate { fields, .. }
                | SessionEvent::ItemCreate { fields, .. }
                | SessionEvent::ObjectValues { fields, .. } => {
                    assert_eq!(
                        fields.layout(),
                        benilla_protocol::messages::FieldLayout::Wrath12340
                    );
                }
                _ => {}
            }
        }
        if matches!(opcode, 0x495 | 0x496) {
            assert!(
                !session_events
                    .iter()
                    .any(|event| matches!(event, SessionEvent::ObjectValues { .. })),
                "packet auras must not synthesize descriptor values"
            );
        }
        crate::net::handlers::dispatch(self.app.world_mut(), session_events.clone());
        session_events
    }
    fn fields(&self, guid: u64) -> ObjectFields {
        let entity = self.app.world().resource::<GuidIndex>().0[&guid];
        let fields = self
            .app
            .world()
            .get::<ObjectStore>(entity)
            .unwrap()
            .0
            .clone();
        assert_eq!(
            fields.layout(),
            benilla_protocol::messages::FieldLayout::Wrath12340
        );
        fields
    }
}

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

#[test]
fn ordered_wrath_packets_reach_real_object_and_aura_handlers() {
    let mut replay = Replay::new();
    let mut login = 0u32.to_le_bytes().to_vec();
    for v in [1.0f32, 2.0, 3.0, 4.0] {
        login.extend(v.to_le_bytes());
    }
    assert!(matches!(
        &replay.packet(0x236, login)[..],
        [SessionEvent::Worldport { map_id: 0, .. }]
    ));
    replay.packet(
        SMSG_UPDATE_OBJECT,
        create(
            7,
            4,
            &[
                (24, 100),
                (25, 721),
                (31, 659),
                (32, 200),
                (33, 1000),
                (39, 1250),
                (1170, 123456),
            ],
        ),
    );
    let player = replay.fields(7);
    assert_eq!(player.unit_health(), Some(100));
    assert_eq!(player.wrath_powers().unwrap()[6], (659, 1250));
    assert_eq!(player.player_money(), Some(123456));
    replay.packet(SMSG_UPDATE_OBJECT, create(8, 3, &[(24, 91), (32, 123)]));
    let unit_guid = 0xf130_0000_0000_0008;
    assert_eq!(replay.fields(unit_guid).unit_health(), Some(91));
    replay.packet(
        SMSG_UPDATE_OBJECT,
        create(9, 1, &[(14, 7), (60, 31), (61, 47)]),
    );
    let item_guid = 0x4000_0000_0000_0009;
    assert_eq!(replay.fields(item_guid).item_durability(), Some(31));
    replay.packet(
        SMSG_UPDATE_OBJECT,
        create(10, 5, &[(14, 0x12340009), (17, 0x07060502)]),
    );
    let go_guid = 0xf110_0000_0000_000a;
    assert_eq!(replay.fields(go_guid).gameobject_state(), Some(2));
    replay.packet(SMSG_UPDATE_OBJECT, delta(8, &[(24, 0)]));
    assert_eq!(replay.fields(unit_guid).unit_health(), Some(0));
    assert_eq!(replay.fields(unit_guid).unit_max_health(), Some(123));
    replay.packet(SMSG_UPDATE_OBJECT, delta(9, &[(60, 0)]));
    assert_eq!(replay.fields(item_guid).item_durability(), Some(0));
    replay.packet(SMSG_UPDATE_OBJECT, delta(10, &[(17, 0x07060501)]));
    assert_eq!(replay.fields(go_guid).gameobject_state(), Some(1));
    replay.packet(SMSG_UPDATE_OBJECT, delta(7, &[(24, 0), (31, 0)]));
    let player = replay.fields(7);
    assert_eq!(player.unit_health(), Some(0));
    assert_eq!(player.unit_max_health(), Some(200));
    assert_eq!(player.wrath_powers().unwrap()[6], (0, 1250));
    assert!(replay
        .packet(SMSG_UPDATE_OBJECT, delta(7, &[(24, 0), (31, 0)]))
        .is_empty());
    let mut aura = vec![1, 7, 55];
    aura.extend(123u32.to_le_bytes());
    aura.extend([0x38, 10, 3]);
    aura.extend(30000u32.to_le_bytes());
    aura.extend(29000u32.to_le_bytes());
    let _aura_events = replay.packet(0x495, aura.clone());
    let state = replay
        .app
        .world()
        .resource::<crate::net::wrath_auras::PacketAuras>();
    assert_eq!(state.by_guid[&7][0].aura.slot, 55);
    assert_eq!(state.by_guid[&7][0].aura.stacks, 3);
    replay.packet(0x496, vec![1, 7, 55, 0, 0, 0, 0]);
    assert!(replay
        .app
        .world()
        .resource::<crate::net::wrath_auras::PacketAuras>()
        .by_guid[&7]
        .is_empty());
    replay.packet(0x496, aura);
    let mut remove = 1u32.to_le_bytes().to_vec();
    remove.push(4);
    remove.extend(1u32.to_le_bytes());
    guid(&mut remove, 7);
    replay.packet(SMSG_UPDATE_OBJECT, remove);
    assert!(!replay
        .app
        .world()
        .resource::<GuidIndex>()
        .0
        .contains_key(&7));
    assert!(!replay
        .app
        .world()
        .resource::<crate::net::wrath_auras::PacketAuras>()
        .by_guid
        .contains_key(&7));
    replay.packet(SMSG_UPDATE_OBJECT, create(7, 4, &[(24, 77)]));
    assert_eq!(replay.fields(7).unit_health(), Some(77));
    assert_eq!(replay.fields(7).player_money(), Some(0));
    replay.packet(0x496, vec![1, 7]);
    assert!(replay
        .app
        .world()
        .resource::<crate::net::wrath_auras::PacketAuras>()
        .by_guid[&7]
        .is_empty());
    replay.adapter.reset();
    crate::net::handlers::dispatch(
        replay.app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "synthetic replay reset".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert!(replay.app.world().resource::<GuidIndex>().0.is_empty());
    assert!(replay
        .app
        .world()
        .resource::<crate::net::wrath_auras::PacketAuras>()
        .by_guid
        .is_empty());
    replay.packet(SMSG_UPDATE_OBJECT, create(7, 4, &[(24, 88)]));
    assert_eq!(replay.fields(7).unit_health(), Some(88));
    assert_eq!(replay.fields(7).player_money(), Some(0));
}

/// Read only reviewed/sanitized decrypted captures. No capture file ships in this unit.
#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
fn recording(bytes: &[u8]) -> Result<Vec<(u64, u16, Vec<u8>)>, &'static str> {
    if bytes.len() < 12 || &bytes[..8] != b"AZBWRPK1" || bytes[8..12] != 12340u32.to_le_bytes() {
        return Err("not a build-12340 recording");
    }
    let mut rest = &bytes[12..];
    let mut packets = Vec::new();
    let mut previous = 0;
    while !rest.is_empty() {
        if rest.len() < 14 {
            return Err("truncated record header");
        }
        let elapsed = u64::from_le_bytes(rest[..8].try_into().unwrap());
        let opcode = u16::from_le_bytes(rest[8..10].try_into().unwrap());
        let len = u32::from_le_bytes(rest[10..14].try_into().unwrap()) as usize;
        rest = &rest[14..];
        if elapsed < previous {
            return Err("nonmonotonic receive clock");
        }
        if len > rest.len() {
            return Err("truncated record body");
        }
        packets.push((elapsed, opcode, rest[..len].to_vec()));
        rest = &rest[len..];
        previous = elapsed;
    }
    Ok(packets)
}

#[test]
fn sanitized_recording_replays_in_receive_order_and_rejects_truncation() {
    let mut bytes = b"AZBWRPK1".to_vec();
    bytes.extend(12340u32.to_le_bytes());
    for (time, body) in [
        (10u64, create(7, 4, &[(24, 100)])),
        (20, delta(7, &[(24, 0)])),
    ] {
        bytes.extend(time.to_le_bytes());
        bytes.extend(SMSG_UPDATE_OBJECT.to_le_bytes());
        bytes.extend((body.len() as u32).to_le_bytes());
        bytes.extend(body);
    }
    let mut replay = Replay::new();
    for (_, opcode, body) in recording(&bytes).unwrap() {
        replay.packet(opcode, body);
    }
    assert_eq!(replay.fields(7).unit_health(), Some(0));
    bytes.pop();
    assert_eq!(recording(&bytes).unwrap_err(), "truncated record body");
    assert!(recording(b"invalid").is_err());
}
