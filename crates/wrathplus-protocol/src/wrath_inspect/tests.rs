use super::*;

fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn h(v: u16) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

#[test]
fn inspect_talents_glyphs_and_enchants_decode_strictly() {
    let body = [
        packed(9),
        w(3),
        vec![1, 0],
        // One group: two talents, six glyph slots.
        vec![2],
        w(1001),
        vec![4],
        w(1002),
        vec![0],
        vec![6],
        h(10),
        h(0),
        h(0),
        h(0),
        h(0),
        h(0),
        // Head (slot 0) and main hand (slot 15).
        w(1 | 1 << 15),
        w(100),
        h(1),
        h(2000),
        h(0),
        packed(0),
        w(0),
        w(200),
        h(0b1001),
        h(3000),
        h(3001),
        h((-5i16) as u16),
        packed(7),
        w(11),
    ]
    .concat();
    let expected = Packet::Inspect {
        player: 9,
        unspent: 3,
        active_group: 0,
        groups: vec![TalentGroup {
            talents: vec![(1001, 4), (1002, 0)],
            glyphs: vec![10, 0, 0, 0, 0, 0],
        }],
        items: vec![
            InspectedItem {
                slot: 0,
                entry: 100,
                enchants: vec![(0, 2000)],
                random_property: 0,
                creator: 0,
                suffix_factor: 0,
            },
            InspectedItem {
                slot: 15,
                entry: 200,
                enchants: vec![(0, 3000), (3, 3001)],
                random_property: -5,
                creator: 7,
                suffix_factor: 11,
            },
        ],
    };
    let event = decode(0x3f4, &body).unwrap();
    assert!(
        matches!(&event, SessionEvent::WrathInspect { packet } if *packet == expected),
        "{event:?}"
    );
    for end in 0..body.len() {
        assert!(decode(0x3f4, &body[..end]).is_err(), "truncated at {end}");
    }
    assert!(decode(0x3f4, &[body.as_slice(), &[0]].concat()).is_err());
    // Talent inspection off: no groups, no items.
    let off = [packed(9), w(0), vec![0, 0], w(0)].concat();
    assert!(matches!(
        decode(0x3f4, &off).unwrap(),
        SessionEvent::WrathInspect { packet: Packet::Inspect { groups, items, .. } } if groups.is_empty() && items.is_empty()
    ));
    assert!(
        decode(0x3f4, &[packed(9), w(0), vec![3, 0]].concat()).is_err(),
        "at most two specs"
    );
    assert!(decode(0x3f4, &[packed(9), w(0), vec![0, 0], w(1 << 19)].concat()).is_err());
}

#[test]
fn wipe_confirm_feeds_the_shared_event_and_dispatches() {
    let body = [9u64.to_le_bytes().to_vec(), w(10_000)].concat();
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    match adapter.dispatch(0x2aa, &body, 0).unwrap() {
        crate::wrath_events::WrathDispatch::Handled { session_events, .. } => assert!(matches!(
            session_events.as_slice(),
            [SessionEvent::TalentWipeConfirm {
                trainer: 9,
                cost: 10_000
            }]
        )),
        other => panic!("{other:?}"),
    }
    assert!(decode(0x2aa, &body[..11]).is_err());
    assert!(decode(0x2aa, &[body.as_slice(), &[0]].concat()).is_err());
}

#[test]
fn commands_encode_ac_read_order() {
    for (command, opcode, body) in [
        (
            Command::Inspect { target: 9 },
            0x114,
            9u64.to_le_bytes().to_vec(),
        ),
        (
            Command::LearnTalent {
                talent_id: 1001,
                rank: 2,
            },
            0x251,
            [w(1001), w(2)].concat(),
        ),
        (
            Command::TalentWipeConfirm { trainer: 9 },
            0x2aa,
            9u64.to_le_bytes().to_vec(),
        ),
        (
            Command::RemoveGlyph { slot: 2 },
            0x48a,
            2u32.to_le_bytes().to_vec(),
        ),
        (
            Command::LearnPreviewTalents {
                talents: vec![(1001, 0), (1002, 1)],
            },
            0x4c1,
            [w(2), w(1001), w(0), w(1002), w(1)].concat(),
        ),
    ] {
        assert_eq!(encode(&command), (opcode, body), "{command:?}");
    }
}
