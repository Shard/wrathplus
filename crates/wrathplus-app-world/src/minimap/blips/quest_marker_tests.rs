//! The selected quest's point, static slot 0 (`0x7f4930`), on the minimap.

use super::*;
use crate::poi_marker::PoiMarker;

/// Off the minimap the point draws the guide arrow, ahead of the directions' own slot; in range
/// it draws nothing, as the slot carries no flags.
#[test]
fn the_selected_quests_point_draws_the_guide_arrow_off_the_minimap() {
    let mut marker = PoiMarker::default();
    marker.set_quest_point(Some((300.0, 0.0, 0, "Vile Familiars")), 1);
    let quest = marker.quest.as_ref().expect("slot 0 set");
    assert_eq!((quest.icon, quest.flags, quest.importance), (6, 0, 0));
    assert!(
        marker.markers_on_map(0).next().is_none(),
        "the current map only"
    );

    let sel = select_landmarks(
        std::iter::empty(),
        marker.markers_on_map(1),
        1,
        0.0,
        0.0,
        133.0,
    );
    assert!(sel.icons.is_empty());
    let arrows: Vec<(&str, RimArrow)> = sel
        .arrows
        .iter()
        .map(|(_, p, a)| (p.name.as_str(), *a))
        .collect();
    assert_eq!(arrows, [("Vile Familiars", RimArrow::Guide)]);

    // Standing on it: no icon, no arrow.
    let sel = select_landmarks(
        std::iter::empty(),
        marker.markers_on_map(1),
        1,
        300.0,
        0.0,
        133.0,
    );
    assert!(sel.icons.is_empty() && sel.arrows.is_empty());

    // A header or no selection clears it.
    marker.set_quest_point(None, 1);
    assert!(marker.markers_on_map(1).next().is_none());
}
