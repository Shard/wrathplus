use super::*;

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn criteria_bytes() -> Vec<u8> {
    let mut row = words(&[111]);
    row.extend([0x01, 0x05]); // counter 5, packed
    row.extend([0x81, 0x07, 0x40]); // player 0x4000000000000007
    row.extend(words(&[1, 0x0a1b_2c3d, 60, 3600]));
    row
}

fn criteria() -> Criteria {
    Criteria {
        id: 111,
        counter: 5,
        player: 0x4000_0000_0000_0007,
        raw_flags: 1,
        packed_time: 0x0a1b_2c3d,
        elapsed: 60,
        creation_elapsed: 3600,
    }
}

fn snapshot_bytes() -> Vec<u8> {
    let mut body = words(&[6, 0x0a1b_2c3d, 7, 0, END]);
    body.extend(criteria_bytes());
    body.extend(criteria_bytes());
    body.extend(words(&[END]));
    body
}

fn snapshot() -> Snapshot {
    Snapshot {
        earned: vec![(6, 0x0a1b_2c3d), (7, 0)],
        criteria: vec![criteria(), criteria()],
    }
}

fn fixtures() -> Vec<(u16, Vec<u8>, Packet)> {
    let mut first = b"Guild\0".to_vec();
    first.extend(9u64.to_le_bytes());
    first.extend(words(&[457, 1]));
    vec![
        (
            0x468,
            [vec![0x01, 0x09], words(&[6, 0x0a1b_2c3d, 0])].concat(),
            Packet::Earned {
                player: 9,
                achievement: 6,
                packed_time: 0x0a1b_2c3d,
                raw_unknown: 0,
            },
        ),
        (
            0x498,
            first,
            Packet::ServerFirst {
                name: "Guild".into(),
                player: 9,
                achievement: 457,
                raw_link: 1,
            },
        ),
        (0x47d, snapshot_bytes(), Packet::AllData(snapshot())),
        (
            0x47d,
            words(&[END, END]),
            Packet::AllData(Snapshot::default()),
        ),
        (
            0x46c,
            [vec![0x01, 0x09], snapshot_bytes()].concat(),
            Packet::Inspect {
                player: 9,
                data: snapshot(),
            },
        ),
        (
            0x49f,
            words(&[6]),
            Packet::AchievementDeleted { achievement: 6 },
        ),
        (
            0x49e,
            words(&[111]),
            Packet::CriteriaDeleted { criteria: 111 },
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
    assert!(
        decode(0x46a, &[0; 4]).is_err(),
        "criteria updates belong to wrath_progress"
    );
}

#[test]
fn dispatch_routes_every_opcode_and_reports_malformed_as_unhandled() {
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures() {
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(
                    format!("{session_events:?}"),
                    format!(
                        "{:?}",
                        vec![crate::SessionEvent::WrathAchievement { packet: expected }]
                    )
                )
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
        assert!(matches!(
            adapter.dispatch(opcode, &[body.as_slice(), &[0]].concat(), 0).unwrap(),
            crate::wrath_events::WrathDispatch::Unhandled { opcode: o } if o == opcode
        ));
    }
}

#[test]
fn inspect_query_is_a_packed_guid() {
    assert_eq!(
        encode_inspect_query(0x4000_0000_0000_0007),
        (0x46b, vec![0x81, 0x07, 0x40])
    );
    assert_eq!(encode_inspect_query(0), (0x46b, vec![0]));
}
