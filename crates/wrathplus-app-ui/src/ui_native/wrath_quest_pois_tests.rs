use super::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

pub fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

/// One quest's 01E4 entry: one POI on map 571, area 488, with two points.
pub fn one_quest_body(quest: u32) -> Vec<u8> {
    let mut b = 1u32.to_le_bytes().to_vec();
    for v in [quest, 1, 0, 0] {
        b.extend(v.to_le_bytes()); // quest, POI count, POI id, objective 0
    }
    for v in [571u32, 488, 0, 0, 0, 2] {
        b.extend(v.to_le_bytes()); // map, area, floor, two raw words, point count
    }
    for v in [10i32, -20, 30, 40] {
        b.extend(v.to_le_bytes());
    }
    b
}

#[test]
fn a_response_becomes_the_logged_quests_blobs_in_log_order() {
    let mut app = App::new();
    app.init_resource::<crate::app_core::state::QuestGiver>();
    crate::net::wrath_quest_pois::register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    dispatch(&mut app, &mut adapter, 0x1e4, &one_quest_body(7));
    // An empty answer is data too: quest 8 has no POIs and is not asked again.
    let mut empty = 1u32.to_le_bytes().to_vec();
    empty.extend([8u32, 0].iter().flat_map(|v| v.to_le_bytes()));
    dispatch(&mut app, &mut adapter, 0x1e4, &empty);
    let mut pois = app.world_mut().resource_mut::<QuestPois>();
    let view = pois.view(&[8, 7]);
    assert_eq!(
        view,
        [WrathQuestPoi {
            quest_id: 7,
            blobs: vec![WrathQuestPoiBlob {
                objective_index: 0,
                map_id: 571,
                area_id: 488,
                floor: 0,
                points: vec![(10, -20), (30, 40)],
            }],
        }]
    );
    assert!(pois.queries(&[8, 7]).is_empty(), "both are answered");
    assert!(
        pois.view(&[]).is_empty(),
        "a quest that left the log is not shown"
    );
}

#[test]
fn missing_quests_are_asked_once_in_packets_of_twenty_five() {
    let mut pois = QuestPois::default();
    let logged: Vec<u32> = (1..=30).collect();
    let asks = pois.queries(&logged);
    assert_eq!(asks.iter().map(Vec::len).collect::<Vec<_>>(), [25, 5]);
    assert_eq!(asks[0][0], 1);
    assert!(
        pois.queries(&logged).is_empty(),
        "an unanswered quest is not asked again"
    );
    assert_eq!(pois.queries(&[31]), [vec![31]]);
}
