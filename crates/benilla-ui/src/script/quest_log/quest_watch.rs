//! The quest watch set behind the tracker: `AddQuestWatch`, `RemoveQuestWatch`, the queries and
//! `ShiftQuestWatches`, keyed by quest id so a log shuffle keeps its watches.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use mlua::{Lua, Value};

use super::super::binding_abi::flag;
use super::super::Model;

/// The watched quest ids in watch order, and a timed watch's expiry in `GetTime` seconds (a
/// permanent watch has none); `set_quest_log` prunes a quest that left the log.
#[derive(Clone, Debug, Default)]
pub(crate) struct QuestWatches {
    pub(crate) ids: Vec<u32>,
    pub(crate) expiry: HashMap<u32, f64>,
}

impl QuestWatches {
    /// Drops the expiry of a quest no longer watched.
    pub(crate) fn prune_expiry(&mut self) {
        let ids = &self.ids;
        self.expiry.retain(|id, _| ids.contains(id));
    }
}

/// The watch set's cap, `0x19` in `0x5e64d0`; stock FrameXML's `MAX_WATCHABLE_QUESTS` agrees.
const MAX_WATCHES: usize = 25;

/// Drops every timed watch whose expiry has passed.
fn expire_watches(lua: &Lua) {
    let now = super::super::clock::now(lua);
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    let QuestWatches { ids, expiry } = &mut model.quest_watches;
    expiry.retain(|id, at| {
        let live = *at > now;
        if !live {
            ids.retain(|w| w != id);
        }
        live
    });
}

// A quest row's id; a header has none (`0x5e67c0` reads `[0xc237b8]`, the header flag).
fn watch_id_at(model: &Model, index: u32) -> Option<u32> {
    (index as usize)
        .checked_sub(1)
        .and_then(|n| model.quest_log.entries.get(n))
        .filter(|e| !e.is_header)
        .map(|e| e.quest_id)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // WatchFrame.lua:1468 passes watch positions, not quest-log indices.
    g.set(
        "ShiftQuestWatches",
        lua.create_function(|lua, (from, to): (usize, usize)| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let watches = &mut model.quest_watches.ids;
            if from > 0 && to > 0 && from <= watches.len() && to <= watches.len() && from != to {
                let id = watches.remove(from - 1);
                watches.insert(to - 1, id);
            }
            Ok(())
        })?,
    )?;
    g.set(
        "GetQuestWatchIndex",
        lua.create_function(|lua, index: u32| {
            expire_watches(lua);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(watch_id_at(&model, index).and_then(|id| {
                model
                    .quest_watches
                    .ids
                    .iter()
                    .position(|w| *w == id)
                    .map(|i| i + 1)
            }))
        })?,
    )?;

    // The watch set, keyed by quest id (`[0xc23680]`, 12-byte rows, count `[0xc23ae0]`).
    // AddQuestWatch(index[, seconds]) (`0x5e67c0` → `0x5e64d0`): at most 25 (`0x19`), a header or
    // out-of-range row adds nothing. `seconds` arms a timed watch; re-adding a permanent watch
    // changes nothing, and re-adding a timed one re-arms it, or makes it permanent with no
    // seconds. The native stamps `time()` and its expiry sweep is untraced: here the expiry is a
    // `GetTime` stamp and an expired watch drops the next time a watch verb reads the set.
    g.set(
        "GetNumQuestWatches",
        lua.create_function(|lua, ()| {
            expire_watches(lua);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.quest_watches.ids.len() as i64)
        })?,
    )?;
    g.set(
        "IsQuestWatched",
        lua.create_function(|lua, i: u32| {
            expire_watches(lua);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(flag(
                watch_id_at(&model, i).is_some_and(|id| model.quest_watches.ids.contains(&id)),
            ))
        })?,
    )?;
    g.set(
        "AddQuestWatch",
        lua.create_function(|lua, (i, seconds): (u32, Option<f64>)| {
            expire_watches(lua);
            let now = super::super::clock::now(lua);
            let seconds = seconds.map_or(0, |s| s.round() as i64);
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let Some(id) = watch_id_at(&model, i) else {
                return Ok(());
            };
            let arm = (seconds != 0).then_some(now + seconds as f64);
            if model.quest_watches.ids.contains(&id) {
                if let Entry::Occupied(mut timed) = model.quest_watches.expiry.entry(id) {
                    match arm {
                        Some(at) => {
                            timed.insert(at);
                        }
                        None => {
                            timed.remove();
                        }
                    }
                }
            } else if model.quest_watches.ids.len() < MAX_WATCHES {
                model.quest_watches.ids.push(id);
                if let Some(at) = arm {
                    model.quest_watches.expiry.insert(id, at);
                }
            }
            Ok(())
        })?,
    )?;
    g.set(
        "RemoveQuestWatch",
        lua.create_function(|lua, i: u32| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if let Some(id) = watch_id_at(&model, i) {
                model.quest_watches.ids.retain(|w| *w != id);
                model.quest_watches.expiry.remove(&id);
            }
            Ok(())
        })?,
    )?;
    // The watched quest's current 1-based row, nil when it is not a visible row.
    g.set(
        "GetQuestIndexForWatch",
        lua.create_function(|lua, w: usize| {
            expire_watches(lua);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let index = w
                .checked_sub(1)
                .and_then(|n| model.quest_watches.ids.get(n))
                .and_then(|id| {
                    model
                        .quest_log
                        .entries
                        .iter()
                        .position(|e| e.quest_id == *id)
                })
                .map(|p| p as i64 + 1);
            Ok(match index {
                Some(i) => Value::Integer(i),
                None => Value::Nil,
            })
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{QuestLogEntryView, QuestLogState};
    use crate::script::UiScript;

    #[test]
    fn watch_cap_is_twenty_five_and_dupes_are_ignored() {
        let mut s = UiScript::new().unwrap();
        let mut state = QuestLogState::default();
        for q in 1..=26u32 {
            state.entries.push(QuestLogEntryView {
                quest_id: q,
                title: format!("Quest {q}"),
                level: 1,
                ..Default::default()
            });
        }
        s.set_quest_log(state);
        for i in 1..=26 {
            s.run(&format!("AddQuestWatch({i})")).unwrap();
        }
        // `0x5e64d0` stops at `0x19`.
        assert_eq!(s.eval::<i64>("return GetNumQuestWatches()").unwrap(), 25);
        s.run("AddQuestWatch(1)").unwrap();
        assert_eq!(s.eval::<i64>("return GetNumQuestWatches()").unwrap(), 25);
        assert_eq!(s.quest_log_watched(), (1..=25).collect::<Vec<u32>>());
    }

    /// `AddQuestWatch(index, seconds)` arms a timed watch that re-arms on a re-add and expires;
    /// a re-add without seconds makes it permanent, and a permanent watch ignores seconds.
    #[test]
    fn a_timed_watch_re_arms_expires_and_a_permanent_one_stays() {
        let mut s = UiScript::new().unwrap();
        let mut state = QuestLogState::default();
        for q in 1..=3u32 {
            state.entries.push(QuestLogEntryView {
                quest_id: q,
                title: format!("Quest {q}"),
                level: 1,
                ..Default::default()
            });
        }
        state.entries[2].is_header = true;
        s.set_quest_log(state);
        let watched = |s: &UiScript, i: u32| {
            s.eval::<bool>(&format!("return IsQuestWatched({i}) == 1"))
                .unwrap()
        };
        s.tick(10.0);
        s.run("AddQuestWatch(1, 300) AddQuestWatch(2) AddQuestWatch(2, 300) AddQuestWatch(3)")
            .unwrap();
        assert!(!watched(&s, 3), "a header adds nothing");
        s.tick(299.0);
        assert!(watched(&s, 1));
        s.run("AddQuestWatch(1, 300)").unwrap();
        s.tick(299.0);
        assert!(watched(&s, 1), "re-armed");
        s.tick(2.0);
        assert!(!watched(&s, 1), "expired 300 s after the last arm");
        assert!(
            watched(&s, 2),
            "a permanent watch ignores a later timed add"
        );
        s.run("AddQuestWatch(1, 300) AddQuestWatch(1)").unwrap();
        s.tick(400.0);
        assert!(watched(&s, 1), "a re-add without seconds made it permanent");
    }
}
