//! The world map's visible quest order.

use crate::script::{Model, QuestLogEntryView};

/// The log quests with an icon on the displayed map (the first 25 log quests), in `0x5e3840`'s
/// order: the completed quests first (`0x5e0ea0`, objectives satisfied), then the rest in log
/// order. Stock `WorldMapFrame_UpdateQuests` depends on it: a completed quest's swap POI takes the
/// list index, and `QuestPOI_HideButtons(…, numPOICompleteSwap + 1)` keeps only the leading ones,
/// so a completed quest listed after an unfinished one would hide its own "?".
pub(super) fn visible_quests(m: &Model) -> Vec<u32> {
    let done = |e: &&QuestLogEntryView| {
        e.complete == 1
            || (e.complete == 0
                && !e.objectives.is_empty()
                && e.objectives.iter().all(|o| o.finished))
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
    first.iter().chain(&rest).map(|e| e.quest_id).collect()
}
