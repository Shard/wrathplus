use super::*;

fn fixtures() -> Vec<(u16, Vec<u8>, SessionEvent)> {
    let ping = [
        9u64.to_le_bytes().as_slice(),
        &1.5f32.to_le_bytes(),
        &(-2.0f32).to_le_bytes(),
    ]
    .concat();
    let roll = [
        1u32.to_le_bytes(),
        100u32.to_le_bytes(),
        42u32.to_le_bytes(),
    ]
    .concat();
    vec![
        (
            0x79,
            b"Bob\0".to_vec(),
            SessionEvent::GroupLeaderChanged { name: "Bob".into() },
        ),
        (0x7c, vec![], SessionEvent::GroupDestroyed),
        (
            0x1d5,
            ping,
            SessionEvent::MinimapPing {
                guid: 9,
                x: 1.5,
                y: -2.0,
            },
        ),
        (
            0x1fb,
            [roll, 9u64.to_le_bytes().to_vec()].concat(),
            SessionEvent::RandomRoll {
                min: 1,
                max: 100,
                roll: 42,
                guid: 9,
            },
        ),
        (
            0x286,
            vec![0; 8],
            SessionEvent::RaidGroupOnly {
                delay_ms: 0,
                reason: 0,
            },
        ),
    ]
}

#[test]
fn every_fixture_maps_to_its_shared_event_and_rejects_bad_extent() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures {
        // `SessionEvent` has no `PartialEq`; its `Debug` form is total over the fields.
        assert_eq!(
            format!("{:?}", decode(opcode, &body).unwrap()),
            format!("{expected:?}")
        );
        for end in 0..body.len() {
            assert!(
                decode(opcode, &body[..end]).is_err(),
                "{opcode:#x} truncated at {end}"
            );
        }
        let trailing = [body.as_slice(), &[0]].concat();
        assert!(decode(opcode, &trailing).is_err(), "{opcode:#x} trailing");
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(
                    format!("{session_events:?}"),
                    format!("{:?}", vec![expected])
                )
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
        assert!(matches!(
            adapter.dispatch(opcode, &trailing, 0).unwrap(),
            crate::wrath_events::WrathDispatch::Unhandled { opcode: o } if o == opcode
        ));
    }
}

#[test]
fn commands_encode_ac_read_order() {
    let g = 9u64.to_le_bytes().to_vec();
    for (command, opcode, body) in [
        (
            Command::Uninvite { name: "Bob".into() },
            0x75,
            b"Bob\0".to_vec(),
        ),
        (
            Command::UninviteGuid {
                guid: 9,
                reason: String::new(),
            },
            0x76,
            [g.clone(), vec![0]].concat(),
        ),
        (Command::SetLeader { guid: 9 }, 0x78, g.clone()),
        (
            Command::MinimapPing { x: 1.5, y: -2.0 },
            0x1d5,
            [1.5f32.to_le_bytes(), (-2.0f32).to_le_bytes()].concat(),
        ),
        (
            Command::RandomRoll { min: 1, max: 100 },
            0x1fb,
            [1u32.to_le_bytes(), 100u32.to_le_bytes()].concat(),
        ),
        (Command::ReadyCheckFinished, 0x3c6, vec![]),
    ] {
        assert_eq!(encode(&command), (opcode, body), "{command:?}");
    }
}
