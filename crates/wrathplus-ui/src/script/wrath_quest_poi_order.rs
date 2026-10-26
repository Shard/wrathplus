//! The world map's visible quest order.

use crate::script::{Model, QuestLogEntryView};

/// The log quests with an icon on the displayed map (the first 25 log quests), in `0x5e3840`'s
/// three passes: the done quests (`0x5e0ea0`, objectives satisfied, or `0x5e10b0`, a quest with
/// nothing to collect or kill), then the watched ones in watch order (`0xc23680`), then the rest
/// in log order. Stock `WorldMapFrame_UpdateQuests` numbers its numeric POIs along this list and
/// `WatchFrame_DisplayTrackedQuests` along the watch list, so the shared quests carry the same
/// numbers; a done quest's swap POI takes the list index, and `QuestPOI_HideButtons(…,
/// numPOICompleteSwap + 1)` keeps only the leading ones. Labelled: `0x5e10b0`'s template checks
/// (money, spell, flags) read here as an in-progress quest with no objective lines.
pub(super) fn visible_quests(m: &Model) -> Vec<u32> {
    let done = |e: &&QuestLogEntryView| {
        e.complete == 1 || (e.complete == 0 && e.objectives.iter().all(|o| o.finished))
    };
    let quests: Vec<&QuestLogEntryView> = m
        .quest_log
        .entries
        .iter()
        .filter(|e| !e.is_header)
        .take(25)
        .filter(|e| super::icon(m, e.quest_id).is_some())
        .collect();
    let (first, rest): (Vec<_>, Vec<_>) = quests.into_iter().partition(done);
    let watched = &m.quest_watches.ids;
    let by_watch = watched
        .iter()
        .filter_map(|id| rest.iter().find(|e| e.quest_id == *id));
    let unwatched = rest.iter().filter(|e| !watched.contains(&e.quest_id));
    first
        .iter()
        .chain(by_watch)
        .chain(unwatched)
        .map(|e| e.quest_id)
        .collect()
}
