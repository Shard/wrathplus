//! The combat log's chat lines that are not combat-log text on build 12340: the reputation
//! change (`CHAT_MSG_COMBAT_FACTION_CHANGE`), and a cast's created item and a pet's meal
//! (`CHAT_MSG_TRADESKILLS`, `CHAT_MSG_PET_INFO`). The 1.12 `CHAT_MSG_COMBAT_*` sentence families
//! are gone: 3.3.5 prints combat text through `COMBAT_LOG_EVENT` and the stock
//! `Blizzard_CombatLog`, which [`super::native`] feeds.

use wrathplus_protocol::types::SpellLogExecute;

use crate::app_core::chat::ChatEventKind;
use crate::app_core::chat::ChatLog;
use crate::app_core::chat::Fills;
use crate::app_core::session::SelfGuid;
use crate::net::StandingChanged;
use crate::ui_chat::combat;
use crate::ui_chat::feed::ChatLogFeed;

/// `SMSG_SET_FACTION_STANDING`: "Your %s reputation has increased by %d.". The wire carries the
/// new total and the sentence the delta, so `net` reads the old one before the overwrite and an
/// unchanged value prints nothing (`0x62c5f0`). No range gate or classifier: the chat type is the
/// literal `0x55` `COMBAT_FACTION_CHANGE`. The wire's list id is `Faction.dbc`'s `rep_index`.
pub(crate) fn faction_standing<'a>(
    changes: impl IntoIterator<Item = &'a StandingChanged>,
    factions: Option<&crate::app_core::state::Factions>,
    log: &mut ChatLog,
) {
    let Some(catalog) = factions.map(|f| f.catalog()) else {
        return;
    };
    for &StandingChanged { list_id, old, new } in changes {
        let delta = new - old;
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
            kind: crate::app_core::chat::ChatEventKind::CombatFactionChange,
            family: if delta > 0 {
                combat::FACTION_STANDING_INCREASED
            } else {
                combat::FACTION_STANDING_DECREASED
            },
            fills: Fills {
                named: name.to_string(),
                amount: i64::from(delta.abs()),
                ..Default::default()
            },
            subject: 0,
            item: None,
            tries: 0,
        });
    }
}

/// `SMSG_SPELLLOGEXECUTE`'s two lines that are not combat-log text: a created item, on
/// `CHAT_MSG_TRADESKILLS` (`0x50bd90`, chat type `0x1e`), and a fed pet's meal, on
/// `CHAT_MSG_PET_INFO` (`0x50baf0`, `0x1f`). Each picks its first-person key when the caster is
/// the player and waits on the item's name (the reference's item callback, `0x67ca30`); the
/// third-person key names the caster. Inferred: the packet that calls them, which the one Ghidra
/// round did not reach above the item callback (`0x4fb970`, `0x4fb950`).
pub(crate) fn spell_log_execute(
    log_execute: &SpellLogExecute,
    self_guid: &SelfGuid,
    log: &mut ChatLog,
) {
    use wrathplus_protocol::types::ExecuteLog as E;
    let mine = self_guid.0 == Some(log_execute.caster);
    for (_effect, rows) in &log_execute.effects {
        for row in rows {
            let (kind, family, item) = match *row {
                E::CreateItem { item_entry } => (
                    ChatEventKind::Tradeskills,
                    if mine {
                        combat::TRADESKILL_LOG_FIRSTPERSON
                    } else {
                        combat::TRADESKILL_LOG_THIRDPERSON
                    },
                    item_entry,
                ),
                E::FeedPet { item_entry } => (
                    ChatEventKind::PetInfo,
                    if mine {
                        combat::FEEDPET_LOG_FIRSTPERSON
                    } else {
                        combat::FEEDPET_LOG_THIRDPERSON
                    },
                    item_entry,
                ),
                _ => continue,
            };
            log.push_combat(combat::PendingCombat {
                kind,
                family,
                fills: Fills::default(),
                subject: if mine { 0 } else { log_execute.caster },
                item: Some(item),
                tries: 0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_core::chat::Pending;
    use wrathplus_protocol::types::ExecuteLog;

    /// Ours speaks in the first person and waits on the item alone; another caster's names them
    /// and waits on both; the meal goes to `PET_INFO`.
    #[test]
    fn a_created_item_and_a_meal_queue_their_own_lines() {
        let me = SelfGuid(Some(7));
        let mut log = ChatLog::default();
        let cast = |caster, row| SpellLogExecute {
            caster,
            spell_id: 3275,
            effects: vec![(24, vec![row])],
        };
        spell_log_execute(
            &cast(7, ExecuteLog::CreateItem { item_entry: 1251 }),
            &me,
            &mut log,
        );
        spell_log_execute(
            &cast(9, ExecuteLog::CreateItem { item_entry: 1251 }),
            &me,
            &mut log,
        );
        spell_log_execute(
            &cast(7, ExecuteLog::FeedPet { item_entry: 4536 }),
            &me,
            &mut log,
        );
        let lines: Vec<_> = log
            .pending
            .iter()
            .map(|p| match p {
                Pending::Combat(l) => (l.kind, l.family.key, l.subject, l.item),
                _ => panic!("a combat line"),
            })
            .collect();
        assert_eq!(
            lines,
            [
                (
                    ChatEventKind::Tradeskills,
                    "TRADESKILL_LOG_FIRSTPERSON",
                    0,
                    Some(1251)
                ),
                (
                    ChatEventKind::Tradeskills,
                    "TRADESKILL_LOG_THIRDPERSON",
                    9,
                    Some(1251)
                ),
                (
                    ChatEventKind::PetInfo,
                    "FEEDPET_LOG_FIRSTPERSON",
                    0,
                    Some(4536)
                ),
            ]
        );
    }
}
