use super::*;

fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn f(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn fixtures() -> Vec<(u16, Vec<u8>, SessionEvent)> {
    let c = |packet| SessionEvent::WrathDeath { packet };
    vec![
        (0x216, vec![0], SessionEvent::CorpseQuery { found: false, display_map: 0, position: [0.0; 3], corpse_map: 0 }),
        (
            0x216,
            [vec![1], w(0), f(1.0), f(2.0), f(3.0), w(36), w(0)].concat(),
            SessionEvent::CorpseQuery { found: true, display_map: 0, position: [1.0, 2.0, 3.0], corpse_map: 36 },
        ),
        (0x269, w(30_000), SessionEvent::CorpseReclaimDelay { delay_ms: 30_000 }),
        (
            0x2ab,
            [9u64.to_le_bytes().to_vec(), w(12), w(120_000)].concat(),
            SessionEvent::SummonRequest { summoner: 9, zone: 12, delay_ms: 120_000 },
        ),
        (
            0x378,
            [w(u32::MAX), f(0.0), f(0.0), f(0.0)].concat(),
            c(Packet::ReleaseLocation { map: -1, position: [0.0; 3] }),
        ),
        (0x494, vec![0x01, 0x09], c(Packet::PreResurrect { guid: 9 })),
        (0x4b7, vec![0; 16], c(Packet::CorpseMapPosition { raw: [0.0; 4] })),
    ]
}

#[test]
fn every_fixture_decodes_and_rejects_truncation_and_trailing_bytes() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures {
        // `SessionEvent` has no `PartialEq`; its `Debug` form is total over the fields.
        assert_eq!(format!("{:?}", decode(opcode, &body).unwrap()), format!("{expected:?}"), "{opcode:#x}");
        // A corpse found byte of 0 is a whole reply.
        if !(opcode == 0x216 && body.len() > 1) {
            for end in 0..body.len() {
                assert!(decode(opcode, &body[..end]).is_err(), "{opcode:#x} truncated at {end}");
            }
        }
        assert!(decode(opcode, &[body.as_slice(), &[0]].concat()).is_err(), "{opcode:#x} trailing");
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(format!("{session_events:?}"), format!("{:?}", vec![expected]))
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
    }
    assert!(decode(0x216, &[2]).is_err());
    assert!(decode(0x216, &[vec![1], vec![0; 20]].concat()).is_err(), "the trailing word is required");
}

#[test]
fn commands_encode_ac_read_order() {
    for (command, opcode, body) in [
        (Command::Repop, 0x15a, vec![0]),
        (Command::CorpseQuery, 0x216, vec![]),
        (Command::ReclaimCorpse { corpse: 9 }, 0x1d2, 9u64.to_le_bytes().to_vec()),
        (Command::SummonResponse { summoner: 9, agree: true }, 0x2ac, [9u64.to_le_bytes().to_vec(), vec![1]].concat()),
        (Command::SelfRes, 0x2b3, vec![]),
        (Command::HearthAndResurrect, 0x49c, vec![]),
        (Command::CorpseMapPositionQuery { raw: 0 }, 0x4b6, w(0)),
    ] {
        assert_eq!(encode(&command), (opcode, body), "{command:?}");
    }
}
