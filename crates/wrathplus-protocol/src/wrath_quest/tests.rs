use super::*;

fn w(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn fixtures() -> Vec<(u16, Vec<u8>, Packet)> {
    let mut poi = w(&[2, 783, 1, 0, u32::MAX, 0, 12, 0, 1, 2, 2]);
    poi.extend(w(&[(-100i32) as u32, 200, 300, (-400i32) as u32]));
    poi.extend(w(&[784, 0]));
    vec![
        (
            0x1e4,
            poi,
            Packet::PoiResponse {
                quests: vec![
                    QuestPois {
                        quest: 783,
                        pois: vec![Poi {
                            id: 0,
                            objective: -1,
                            map: 0,
                            area: 12,
                            floor: 0,
                            raw_unknown: [1, 2],
                            points: vec![(-100, 200), (300, -400)],
                        }],
                    },
                    QuestPois {
                        quest: 784,
                        pois: vec![],
                    },
                ],
            },
        ),
        (0x1e4, w(&[0]), Packet::PoiResponse { quests: vec![] }),
        (
            0x46f,
            w(&[11, 2, 5]),
            Packet::PvpKill {
                quest: 11,
                current: 2,
                required: 5,
            },
        ),
    ]
}

#[test]
fn every_fixture_decodes_and_rejects_truncation_and_trailing_bytes() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    for (opcode, body, expected) in fixtures {
        assert_eq!(decode(opcode, &body).unwrap(), expected, "{opcode:#x}");
        for end in 0..body.len() {
            assert!(
                decode(opcode, &body[..end]).is_err(),
                "{opcode:#x} truncated at {end}"
            );
        }
        let mut trailing = body.clone();
        trailing.push(0);
        assert!(decode(opcode, &trailing).is_err(), "{opcode:#x} trailing");
    }
    assert!(decode(0x1e4, &w(&[u32::MAX])).is_err());
    assert!(
        decode(0x21e, &w(&[1])).is_err(),
        "force-remove has no AC sender"
    );
}

#[test]
fn dispatch_routes_every_opcode_and_reports_malformed_as_unhandled() {
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures() {
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => assert_eq!(
                format!("{session_events:?}"),
                format!(
                    "{:?}",
                    vec![crate::SessionEvent::WrathQuest { packet: expected }]
                )
            ),
            other => panic!("{opcode:#x}: {other:?}"),
        }
        assert!(matches!(
            adapter.dispatch(opcode, &[body.as_slice(), &[0]].concat(), 0).unwrap(),
            crate::wrath_events::WrathDispatch::Unhandled { opcode: o } if o == opcode
        ));
    }
}

#[test]
fn commands_encode_ac_read_order() {
    for (command, opcode, body) in [
        (Command::AutoLaunch, 0x187, vec![]),
        (Command::Cancel, 0x190, vec![]),
        (
            Command::SwapQuest {
                slot1: 3,
                slot2: 24,
            },
            0x193,
            vec![3, 24],
        ),
        (
            Command::PoiQuery {
                quests: vec![783, 784],
            },
            0x1e3,
            w(&[2, 783, 784]),
        ),
        (Command::StatusMultipleQuery, 0x417, vec![]),
    ] {
        assert_eq!(encode(&command).unwrap(), (opcode, body), "{command:?}");
    }
    assert!(encode(&Command::PoiQuery {
        quests: vec![1; 26]
    })
    .is_err());
    assert_eq!(
        encode_push_result(0x42, 783, 4),
        (
            0x276,
            [
                0x42u64.to_le_bytes().as_slice(),
                &783u32.to_le_bytes(),
                &[4]
            ]
            .concat()
        )
    );
}
