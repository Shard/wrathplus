use super::*;

fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn q(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn debug(events: &[SessionEvent]) -> String {
    // `SessionEvent` has no `PartialEq`; its `Debug` form is total over the fields.
    format!("{events:?}")
}

#[test]
fn contacts_split_into_friends_and_ignores_and_statuses_keep_their_tail() {
    let list = [
        w(7),
        w(3),
        // An online friend, an offline friend, an ignore.
        q(1), w(1), b"note\0".to_vec(), vec![1], w(12), w(80), w(5),
        q(2), w(1), vec![0], vec![0],
        q(3), w(2), vec![0],
    ]
    .concat();
    let expected = [
        SessionEvent::FriendList {
            friends: vec![
                FriendEntry { guid: 1, status: 1, area: 12, level: 80, class: 5 },
                FriendEntry { guid: 2, status: 0, area: 0, level: 0, class: 0 },
            ],
        },
        SessionEvent::IgnoreList { guids: vec![3] },
    ];
    assert_eq!(debug(&decode(0x67, &list).unwrap()), debug(&expected));
    let online = [vec![2], q(9), vec![1], w(12), w(80), w(5)].concat();
    assert_eq!(
        debug(&decode(0x68, &online).unwrap()),
        debug(&[SessionEvent::FriendStatus(FriendStatusUpdate {
            result: 2,
            guid: 9,
            online: Some(FriendOnline { status: 1, area: 12, level: 80, class: 5 }),
        })])
    );
    let removed = [vec![5], q(9)].concat();
    assert_eq!(
        debug(&decode(0x68, &removed).unwrap()),
        debug(&[SessionEvent::FriendStatus(FriendStatusUpdate { result: 5, guid: 9, online: None })])
    );
    assert!(decode(0x68, &[removed.clone(), vec![0]].concat()).is_err());
}

#[test]
fn duels_map_to_the_shared_events_with_seconds_and_retreats() {
    let rows: Vec<(u16, Vec<u8>, SessionEvent)> = vec![
        (0x167, [q(5), q(6)].concat(), SessionEvent::DuelRequested { arbiter: 5, challenger: 6 }),
        (0x168, vec![], SessionEvent::DuelOutOfBounds),
        (0x169, vec![], SessionEvent::DuelInBounds),
        (0x16a, vec![1], SessionEvent::DuelComplete { started: true }),
        (
            0x16b,
            [vec![1], b"A\0B\0".to_vec()].concat(),
            SessionEvent::DuelWinner { fled: true, winner: "A".into(), loser: "B".into() },
        ),
        (0x2b7, w(3000), SessionEvent::DuelCountdown { seconds: 3 }),
    ];
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in rows {
        assert_eq!(debug(&decode(opcode, &body).unwrap()), debug(&[expected.clone()]), "{opcode:#x}");
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(debug(&session_events), debug(&[expected]))
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
        assert!(matches!(
            adapter.dispatch(opcode, &[body.as_slice(), &[0]].concat(), 0).unwrap(),
            crate::wrath_events::WrathDispatch::Unhandled { .. }
        ));
    }
}
