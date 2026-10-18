use super::*;
use benilla_protocol::wrath_achievement::Criteria;
use std::collections::VecDeque;

fn snapshot(earned: &[(u32, u32)], criteria: &[(u32, u64)]) -> Snapshot {
    Snapshot {
        earned: earned.to_vec(),
        criteria: criteria
            .iter()
            .map(|&(id, counter)| Criteria {
                id,
                counter,
                player: 1,
                raw_flags: 0,
                packed_time: 0,
                elapsed: 0,
                creation_elapsed: 0,
            })
            .collect(),
    }
}

#[test]
fn own_receipts_leave_the_capture_and_others_keep_their_order() {
    let me = 0x10;
    let mut state = Achievements::default();
    let other_earn = Packet::Earned {
        player: 0x20,
        achievement: 7,
        packed_time: 1,
        raw_unknown: 0,
    };
    let first = Packet::ServerFirst {
        name: "Guild".into(),
        player: 0x20,
        achievement: 457,
        raw_link: 1,
    };
    let mut packets = VecDeque::from([
        Packet::AllData(snapshot(&[(6, 100), (9, 300)], &[(40, 3), (41, 5)])),
        other_earn.clone(),
        Packet::Earned {
            player: me,
            achievement: 8,
            packed_time: 200,
            raw_unknown: 0,
        },
        Packet::Earned {
            player: me,
            achievement: 8,
            packed_time: 200,
            raw_unknown: 0,
        },
        first.clone(),
        Packet::CriteriaDeleted { criteria: 41 },
        Packet::AchievementDeleted { achievement: 6 },
        Packet::Inspect {
            player: 0x30,
            data: snapshot(&[(1, 1)], &[]),
        },
    ]);
    let (earned, comparison) = absorb(&mut packets, &mut state, me);
    assert_eq!(earned, [8], "a repeated earn fires once");
    assert!(comparison.is_none(), "nobody asked for 0x30");
    assert_eq!(packets.len(), 3);
    assert_eq!(packets[0], other_earn);
    assert_eq!(packets[1], first);
    assert_eq!(
        state
            .earned
            .iter()
            .map(|(&i, &d)| (i, d))
            .collect::<Vec<_>>(),
        [(8, 200), (9, 300)]
    );
    assert_eq!(state.criteria.get(&40), Some(&3));
    assert_eq!(state.deleted, [41]);
    assert_eq!(
        earned_newest_first(state.earned.iter().map(|(&i, &d)| (i, d))),
        [(9, 300), (8, 200)]
    );

    state.compared = Some(0x30);
    let mut packets = VecDeque::from([Packet::Inspect {
        player: 0x30,
        data: snapshot(&[(1, 1)], &[(40, 9)]),
    }]);
    let (_, comparison) = absorb(&mut packets, &mut state, me);
    let (guid, data) = comparison.expect("the asked player");
    assert_eq!(guid, 0x30);
    let view = snapshot_progress(&data, -1);
    assert_eq!(view.earned, [(1, 1)]);
    assert_eq!(view.criteria.get(&40), Some(&9));
    assert!(packets.is_empty());
}

#[test]
fn races_map_to_the_achievement_factions() {
    assert_eq!(faction(Some(10)), 0, "blood elf is Horde");
    assert_eq!(faction(Some(11)), 1, "draenei is Alliance");
    assert_eq!(faction(None), -1);
}
