//! Build 12340's staged quest reputation rewards, `ProcessQuestLogRewardFactions` (`0x5df8a0` →
//! `0x58d050` → `0x58c790`), which `GetNumQuestLogRewardFactions` and
//! `GetQuestLogRewardFactionInfo` read. Evidence: `/tmp/azb-app-mig/ghidra/{quest1,quest2,rep1,
//! rep2}.log`.
//!
//! Each of the template's five slots names a `Faction.dbc` row, a signed `QuestFactionReward`
//! index (row 1 for a gain, row 2 for a loss, at the absolute index below 10) and an override.
//! The amount, in hundredths of a point, is the override, else the table's amount times 100. A
//! slot whose table amount is 0 stages nothing, override or not. A staged gain spills into the
//! faction's parent (`ParentFactionMod[1]`, capped below `ParentFactionCap[1]`'s rank) and its
//! children (each child's `ParentFactionMod[0]`, under its `ParentFactionCap[0]`), recursively, never
//! back into the faction it came from, unless the slot's bit in `RewardRepShowMask` is set. A stage
//! is skipped when the standing would not move, and at most ten are kept.
//!
//! Labelled: the standing (`0x5d05b0`) is compared in hundredths, as the caps are, and the child
//! order (`0x5d2680`) is taken as descending id.

use wrathplus_formats::{FactionCatalog, QuestFactionRewardCatalog};

/// `0xa2d300`: each rank's upper bound, Hated through Exalted. A loss's cap table (`0xa2d2fc`, the
/// lower bounds) never applies: `0x58d050` and the spillover pass no loss cap.
const RANK_UPPER: [i32; 9] = [-6000, -3000, 0, 3000, 9000, 21000, 42000, 43000, 0];
/// `0x58c790` keeps ten.
const MAX_STAGED: usize = 10;

/// The staged `(faction, amount)` list for one quest's reward slots; `standing` is a faction's
/// current points, base included.
pub(crate) fn staged(
    slots: &[(u32, i32, i32); 5],
    show_mask: u32,
    table: &QuestFactionRewardCatalog,
    factions: &FactionCatalog,
    standing: &dyn Fn(u32) -> i32,
) -> Vec<(u32, i32)> {
    let mut out = Vec::new();
    for (k, &(faction, value, over)) in slots.iter().enumerate() {
        if factions.faction(faction).is_none() || value.unsigned_abs() >= 10 {
            continue;
        }
        let Some(base) = table.amount(value).filter(|&a| a != 0) else {
            continue;
        };
        let amount = if over != 0 { over } else { base * 100 };
        let no_spill = show_mask & (1 << k) != 0;
        stage(
            &mut out, factions, standing, faction, amount, None, 0, no_spill,
        );
    }
    out
}

/// `0x58c790`: stage one faction's change, then its spillover.
#[allow(clippy::too_many_arguments)]
fn stage(
    out: &mut Vec<(u32, i32)>,
    factions: &FactionCatalog,
    standing: &dyn Fn(u32) -> i32,
    faction: u32,
    amount: i32,
    cap_rank: Option<u32>,
    origin: u32,
    no_spill: bool,
) {
    let Some(info) = factions.faction(faction) else {
        return;
    };
    let current = standing(faction) * 100;
    let mut next = current + amount;
    if amount >= 1 {
        if let Some(cap) = cap_rank.and_then(|r| RANK_UPPER.get(r as usize)) {
            let cap = cap * 100 - 100;
            if current.max(cap) <= next {
                next = cap;
                if cap < current {
                    return;
                }
            }
        }
    }
    if next == current || out.len() >= MAX_STAGED {
        return;
    }
    out.push((faction, amount));
    if no_spill {
        return;
    }
    let up = (amount as f32 * info.parent_mod[1]).round() as i32;
    if info.team != 0 && info.team != origin && up != 0 {
        stage(
            out,
            factions,
            standing,
            info.team,
            up,
            Some(info.parent_cap[1]),
            faction,
            false,
        );
    }
    for child in factions.children(faction).into_iter().rev() {
        let Some(c) = factions.faction(child) else {
            continue;
        };
        let down = (amount as f32 * c.parent_mod[0]).round() as i32;
        if child != origin && down != 0 {
            stage(
                out,
                factions,
                standing,
                child,
                down,
                Some(c.parent_cap[0]),
                faction,
                false,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrathplus_formats::FactionInfo;
    use std::collections::HashMap;

    fn catalog() -> FactionCatalog {
        let mut c = FactionCatalog::from_rows(HashMap::new(), HashMap::new());
        // A parent (169) whose child 21 spills half up, capped below Exalted; 21 has a child 22
        // that takes a quarter down, capped below Friendly.
        c.insert_faction(169, FactionInfo::for_tests(-1, 0, [0.0, 0.0], [0, 0]));
        c.insert_faction(21, FactionInfo::for_tests(1, 169, [0.0, 0.5], [0, 7]));
        c.insert_faction(22, FactionInfo::for_tests(2, 21, [0.25, 0.0], [4, 0]));
        c
    }

    #[test]
    fn a_gain_stages_and_spills_up_and_down_but_not_back() {
        let table = QuestFactionRewardCatalog::from_rows([(
            1,
            [0, 10, 25, 75, 150, 250, 350, 500, 1000, 0],
        )]);
        let slots = [(21, 5, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0)];
        let staged = staged(&slots, 0, &table, &catalog(), &|_| 0);
        assert_eq!(staged, [(21, 25000), (169, 12500), (22, 6250)]);
        assert_eq!(
            super::staged(&slots, 1, &table, &catalog(), &|_| 0),
            [(21, 25000)],
            "the slot's show-mask bit stops the spill"
        );
        let over = [(21, 5, 300), (0, 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0)];
        assert_eq!(
            super::staged(&over, 1, &table, &catalog(), &|_| 0),
            [(21, 300)]
        );
    }

    #[test]
    fn a_child_at_its_cap_takes_nothing() {
        let table = QuestFactionRewardCatalog::from_rows([(
            1,
            [0, 10, 25, 75, 150, 250, 350, 500, 1000, 0],
        )]);
        let slots = [(21, 5, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0)];
        // 22 already sits above its Friendly cap (9000 * 100 - 100).
        let standing = |f: u32| if f == 22 { 9500 } else { 0 };
        let staged = staged(&slots, 0, &table, &catalog(), &standing);
        assert_eq!(staged, [(21, 25000), (169, 12500)]);
    }
}
