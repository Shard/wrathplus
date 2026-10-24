//! The selected quest's minimap marker. 12340's `SelectQuestLogEntry` (`0x5e02f0` → `0x5dffa0`)
//! points static POI slot 0 at the quest's record (`0x5deb60` → `0x7f4930`), and re-runs it when
//! the record arrives (`0x5de960`). Out of range the slot draws the rim's guide arrow
//! (`0x57d860`: slots 0 and 1 take `Rotating-MinimapGuideArrow`). Labelled: the record fields are
//! taken as `PointX`, `PointY`, `PointOpt` and the title, and a selection kept across a log
//! rebuild re-points the slot here, where the reference re-points it only on a select call.

use bevy::prelude::*;
use wrathplus_ui::script::UiScript;

use crate::net::NetCommands;
use crate::poi_marker::PoiMarker;
use crate::ui_quest_log::QuestLog;

pub fn register(app: &mut App) {
    app.add_systems(
        Update,
        sync_quest_marker
            .after(crate::ui_quest_log::feed_quest_log)
            .after(crate::poi_marker::PoiMarkerUpkeep)
            .in_set(crate::app_core::schedule::UiFeed)
            .run_if(crate::app_core::ui_state::ingame_ui_up),
    );
}

/// The selected row's quest id, `None` for none or a header: `GetQuestLogTitle`'s return 9.
fn selected_quest(script: &UiScript) -> Option<u32> {
    if script.quest_log_selection() == 0 {
        return None;
    }
    script
        .eval::<i64>("return (select(9, GetQuestLogTitle(GetQuestLogSelection())))")
        .ok()
        .and_then(|id| u32::try_from(id).ok())
        .filter(|&id| id != 0)
}

/// What the slot was last set from: the quest, and whether its record had arrived.
type Shown = Option<(u32, bool)>;

fn sync_quest_marker(
    script: Option<NonSend<UiScript>>,
    quest_log: Option<Res<QuestLog>>,
    commands: Res<NetCommands>,
    current_map: Option<Res<wrathplus_world::world_map::CurrentMap>>,
    mut marker: ResMut<PoiMarker>,
    mut shown: Local<crate::app_core::vm_memo::VmMemo<Shown>>,
) {
    let (Some(script), Some(quest_log)) = (script, quest_log) else {
        return;
    };
    let selected = selected_quest(&script);
    let template = selected.and_then(|q| quest_log.template(q, &commands));
    let now: Shown = selected.map(|q| (q, template.is_some()));
    let shown = shown.get(&script);
    if now == *shown {
        return;
    }
    *shown = now;
    let map_id = current_map.as_ref().map_or(0, |m| m.0);
    marker.set_quest_point(
        template.map(|t| (t.point_x, t.point_y, t.point_opt, t.title.as_str())),
        map_id,
    );
}

#[cfg(test)]
#[path = "wrath_quest_marker_tests.rs"]
mod tests;
