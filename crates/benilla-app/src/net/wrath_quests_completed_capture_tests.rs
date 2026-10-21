use super::*;
use crate::net::test_support::quests_completed::dispatch;
use benilla_protocol::wrath_events::WrathEventAdapter;

#[test]
fn ordered_replies_keep_empty_duplicates_and_reject_malformed() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let body = [2, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255];
    dispatch(&mut app, &mut adapter, &body);
    dispatch(&mut app, &mut adapter, &[0; 4]);
    dispatch(&mut app, &mut adapter, &body);
    for end in 0..body.len() {
        dispatch(&mut app, &mut adapter, &body[..end]);
    }
    let mut trailing = body.to_vec();
    trailing.push(0);
    dispatch(&mut app, &mut adapter, &trailing);
    assert_eq!(
        app.world()
            .resource::<NativeQuestsCompletedPackets>()
            .0
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            Packet {
                quest_ids: vec![u32::MAX, u32::MAX]
            },
            Packet { quest_ids: vec![] },
            Packet {
                quest_ids: vec![u32::MAX, u32::MAX]
            },
        ]
    );
}
