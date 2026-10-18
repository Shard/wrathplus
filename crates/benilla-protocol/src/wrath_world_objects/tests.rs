use super::*;
use crate::wire::write_packed_guid;

fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn q(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn packed(guid: u64) -> Vec<u8> {
    let mut b = Vec::new();
    write_packed_guid(guid, &mut b).unwrap();
    b
}

fn fixtures() -> Vec<(u16, Vec<u8>, SessionEvent)> {
    let c = |packet| SessionEvent::WrathWorldObjects { packet };
    let go = 0xf110_0000_0000_0007u64;
    vec![
        (
            0x0b3,
            [q(go), w(1)].concat(),
            SessionEvent::GameObjectCustomAnim {
                guid: go,
                anim_id: 1,
            },
        ),
        (
            0x215,
            q(go),
            SessionEvent::GameObjectDespawnAnim { guid: go },
        ),
        (0x1c8, vec![], SessionEvent::FishNotHooked),
        (0x1c9, vec![], SessionEvent::FishEscaped),
        (0x222, q(9), SessionEvent::SpiritHealerConfirm { npc: 9 }),
        (0x277, w(11), SessionEvent::PlayMusic { music_id: 11 }),
        (
            0x278,
            [w(12), q(go)].concat(),
            SessionEvent::PlayObjectSound {
                sound_id: 12,
                guid: go,
            },
        ),
        (
            0x032,
            [packed(go), packed(5), packed(6), w((-40i32) as u32), w(55)].concat(),
            c(Packet::BuildingDamage {
                building: go,
                attacker: 5,
                player: 6,
                change: -40,
                spell: 55,
            }),
        ),
        (0x1df, q(go), c(Packet::PageText { object: go })),
        (
            0x412,
            [w(1), w(2), w(3000)].concat(),
            c(Packet::OverrideLight {
                default_light: 1,
                override_light: 2,
                fade_in_ms: 3000,
            }),
        ),
        (
            0x455,
            [w(0), w(0)].concat(),
            c(Packet::LearnedDanceMoves { raw: [0, 0] }),
        ),
        (0x464, w(14), c(Packet::TriggerMovie { movie: 14 })),
        (0x47c, w(0x41), c(Packet::PhaseShift { mask: 0x41 })),
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
                assert_eq!(
                    format!("{session_events:?}"),
                    format!("{:?}", vec![expected])
                )
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
    }
}

#[test]
fn use_and_report_use_send_the_full_guid() {
    let go = 0xf110_0000_0000_0007u64;
    assert_eq!(encode(&Command::Use { object: go }), (0x0b1, q(go)));
    assert_eq!(encode(&Command::ReportUse { object: go }), (0x481, q(go)));
}
