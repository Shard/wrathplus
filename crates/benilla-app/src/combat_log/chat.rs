//! The combat log's one chat line that is not combat-log text on build 12340: the reputation
//! change, `CHAT_MSG_COMBAT_FACTION_CHANGE`. The 1.12 `CHAT_MSG_COMBAT_*` sentence families are
//! gone: 3.3.5 prints combat text through `COMBAT_LOG_EVENT` and the stock `Blizzard_CombatLog`,
//! which [`super::native`] feeds.

use crate::net::Reputations;
use crate::ui_chat::combat::{self, Fills};
use crate::ui_chat::feed::ChatLogFeed;
use crate::ui_chat::ChatLog;

/// `SMSG_SET_FACTION_STANDING`: "Your %s reputation has increased by %d.". The wire carries the
/// new total and the sentence the delta, so this runs before the store is overwritten, and an
/// unchanged value prints nothing (`0x62c5f0`). No range gate or classifier: the chat type is the
/// literal `0x55` `COMBAT_FACTION_CHANGE`. The wire's list id is `Faction.dbc`'s `rep_index`.
pub(crate) fn faction_standing(
    deltas: &[(u32, i32)],
    reputations: &Reputations,
    factions: Option<&crate::target::ring::Factions>,
    log: &mut ChatLog,
) {
    let Some(catalog) = factions.map(|f| f.catalog()) else {
        return;
    };
    for &(list_id, standing) in deltas {
        let old = reputations
            .0
            .get(list_id as usize)
            .map_or(0, |(_, standing)| *standing);
        let delta = standing - old;
        if delta == 0 {
            continue;
        }
        let Ok(index) = i32::try_from(list_id) else {
            continue;
        };
        let Some(name) = catalog
            .reputation_factions()
            .find(|(_, f)| f.rep_index == index)
            .and_then(|(id, _)| catalog.faction_name(id))
        else {
            continue;
        };
        log.push_combat(combat::PendingCombat {
            kind: crate::ui_chat::ChatEventKind::CombatFactionChange,
            family: if delta > 0 {
                combat::FACTION_STANDING_INCREASED
            } else {
                combat::FACTION_STANDING_DECREASED
            },
            fills: Fills {
                named: name.to_string(),
                amount: i64::from(delta.abs()),
            },
        });
    }
}
