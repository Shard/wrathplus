use super::*;

fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn fixtures() -> Vec<(u16, Vec<u8>, Vec<SessionEvent>)> {
    let c = |packet| SessionEvent::WrathSessionMisc { packet };
    vec![
        (
            0x1cf,
            [w(1_700_000_000), w(3600)].concat(),
            vec![
                SessionEvent::ServerUnixTime {
                    unix_time: 1_700_000_000,
                },
                c(Packet::QueryTime {
                    server_time: 1_700_000_000,
                    daily_reset_in: 3600,
                }),
            ],
        ),
        (
            0x1d9,
            [
                w(1),
                w(60_000),
                w(180_000),
                w((-1i32) as u32),
                vec![0],
                w(5),
            ]
            .concat(),
            vec![SessionEvent::MirrorTimerStart(MirrorTimerStart {
                kind: 1,
                remaining_ms: 60_000,
                duration_ms: 180_000,
                scale: -1,
                paused: false,
                spell_id: 5,
            })],
        ),
        (
            0x1da,
            [w(1), vec![1]].concat(),
            vec![SessionEvent::MirrorTimerPause {
                kind: 1,
                paused: true,
            }],
        ),
        (0x1db, w(2), vec![SessionEvent::MirrorTimerStop { kind: 2 }]),
        (
            0x2f5,
            [w(2), w(600)].concat(),
            vec![c(Packet::PlayTimeWarning {
                flags: 2,
                remaining: 600,
            })],
        ),
        (
            0x3c1,
            [7u64.to_le_bytes().to_vec(), w(2), w(2593)].concat(),
            vec![c(Packet::Inebriation {
                player: 7,
                threshold: 2,
                item: 2593,
            })],
        ),
        (
            0x3c9,
            vec![2, 0],
            vec![c(Packet::FeatureSystemStatus {
                complaints: 2,
                voice_chat: 0,
            })],
        ),
    ]
}

#[test]
fn every_fixture_decodes_and_rejects_bad_extent() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures {
        assert_eq!(
            format!("{:?}", decode(opcode, &body).unwrap()),
            format!("{expected:?}"),
            "{opcode:#x}"
        );
        for end in 0..body.len() {
            assert!(
                decode(opcode, &body[..end]).is_err(),
                "{opcode:#x} truncated at {end}"
            );
        }
        assert!(
            decode(opcode, &[body.as_slice(), &[0]].concat()).is_err(),
            "{opcode:#x} trailing"
        );
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(format!("{session_events:?}"), format!("{expected:?}"))
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
    }
    assert!(
        decode(0x1da, &[w(1), vec![2]].concat()).is_err(),
        "paused is a boolean byte"
    );
}

#[test]
fn commands_encode_ac_read_order() {
    assert_eq!(encode(&Command::PlayerLogout), (0x04a, vec![]));
    assert_eq!(encode(&Command::Emote { emote: 3 }), (0x102, w(3)));
    assert_eq!(encode(&Command::QueryTime), (0x1ce, vec![]));
    assert_eq!(
        encode(&Command::UnlearnSkill { skill: 186 }),
        (0x202, w(186))
    );
    assert_eq!(encode(&Command::SetPvp { on: 1 }), (0x253, vec![1]));
}
