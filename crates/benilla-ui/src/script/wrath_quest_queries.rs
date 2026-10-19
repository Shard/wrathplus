//! Build 12340's quest-log readers the 1.12 surface lacks: `QuestFlagsPVP` (`0x58c3e0`),
//! `GetQuestLink` (`0x5e51d0` → `0x61e5c0`), `GetQuestSortIndex` (`0x5dfa10` → `0x5deae0`),
//! `GetNumQuestItemDrops` (`0x5e4260` → `0x5e1ff0`) and `GetQuestLogItemDrop` (`0x5e60d0` →
//! `0x5e20a0`). Evidence: `/tmp/azb-app-mig/ghidra/{audit1,quest1,quest2}.log`.
//!
//! Labelled: `QuestFlagsPVP`'s gate `0xb2f998` (written at `0x405540`) is read as 0.
//! `GetQuestLogItemDrop`'s second string (`0x9f6f44`) is not resolved and answers empty.

use std::collections::HashMap;

use mlua::{Lua, MultiValue, Value};

use super::{Model, UiScript};

/// `QUEST_FLAGS_FLAGS_PVP`.
const QUEST_FLAGS_PVP: u32 = 0x2000;

#[derive(Default)]
pub(crate) struct State {
    /// The open quest panel's flags (`0xc0d6b4`).
    panel_flags: u32,
    /// Each logged quest's `(source item, objective drops)`.
    item_drops: HashMap<u32, (u32, Vec<(u32, u32)>)>,
}

impl UiScript {
    /// The open quest panel's quest flags, 0 for none.
    pub fn set_wrath_quest_panel_flags(&mut self, flags: u32) {
        self.model_mut().wrath_quests.panel_flags = flags;
    }

    /// Each logged quest's source item and objective drops `(item, count)`.
    pub fn set_wrath_quest_item_drops(&mut self, drops: HashMap<u32, (u32, Vec<(u32, u32)>)>) {
        self.model_mut().wrath_quests.item_drops = drops;
    }
}

/// `0x61e540`: the link colour for a quest `level` against the player's.
fn difficulty_color(level: i64, player: i64, green_range: i64) -> &'static str {
    let diff = level - player;
    if diff > 4 {
        "|cffff2020"
    } else if diff > 2 {
        "|cffff8040"
    } else if diff >= -2 {
        "|cffffff00"
    } else if -diff > green_range {
        "|cff808080"
    } else {
        "|cff40c040"
    }
}

fn number(lua: &Lua, args: &MultiValue, i: usize) -> Option<i64> {
    args.get(i)
        .and_then(|v| lua.coerce_number(v.clone()).ok().flatten())
        .map(|n| n.round() as i64)
}

/// A quest-log row's quest id (headers have none), by 1-based index.
fn row_quest(model: &Model, index: i64) -> Option<u32> {
    let i = usize::try_from(index.checked_sub(1)?).ok()?;
    model
        .quest_log
        .entries
        .get(i)
        .filter(|e| !e.is_header && e.quest_id != 0)
        .map(|e| e.quest_id)
}

fn item_count(lua: &Lua, item: u32) -> i64 {
    lua.globals()
        .get::<mlua::Function>("GetItemCount")
        .ok()
        .and_then(|f| f.call::<Option<f64>>(item).ok().flatten())
        .map_or(0, |n| n as i64)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "QuestFlagsPVP",
        lua.create_function(|lua, ()| {
            let flags = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_quests
                .panel_flags;
            Ok((flags & QUEST_FLAGS_PVP != 0).then_some(1.0))
        })?,
    )?;
    // GetQuestLink(index): `"%s|Hquest:%d:%d|h[%s]|h%s"` with the difficulty colour, the quest id,
    // its level and title, and `|r`.
    g.set(
        "GetQuestLink",
        lua.create_function(|lua, args: MultiValue| {
            let Some(index) = number(lua, &args, 0) else {
                return Err(mlua::Error::runtime("Usage: GetQuestLink(questID)"));
            };
            let green = lua
                .globals()
                .get::<mlua::Function>("GetQuestGreenRange")
                .ok()
                .and_then(|f| f.call::<Option<f64>>(()).ok().flatten())
                .map_or(0, |n| n as i64);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let player = model.unit("player").map_or(0, |u| i64::from(u.level));
            let link = usize::try_from(index - 1).ok().and_then(|i| {
                let e = model.quest_log.entries.get(i)?;
                if e.is_header || e.quest_id == 0 {
                    return None;
                }
                let color = difficulty_color(i64::from(e.level), player, green);
                Some(format!(
                    "{color}|Hquest:{}:{}|h[{}]|h|r",
                    e.quest_id, e.level, e.title
                ))
            });
            Ok(match link {
                Some(l) => Value::String(lua.create_string(&l)?),
                None => Value::Nil,
            })
        })?,
    )?;
    // GetQuestSortIndex(index): the 1-based place of the row's header among the headers, 0 when
    // none.
    g.set(
        "GetQuestSortIndex",
        lua.create_function(|lua, args: MultiValue| {
            let Some(index) = number(lua, &args, 0) else {
                return Err(mlua::Error::runtime("Usage: GetQuestSortIndex(questIndex)"));
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let entries = &model.quest_log.entries;
            let Some(i) = usize::try_from(index - 1)
                .ok()
                .filter(|&i| i < entries.len())
            else {
                return Ok(0.0);
            };
            let headers = entries[..=i].iter().filter(|e| e.is_header).count();
            Ok(headers as f64)
        })?,
    )?;
    // GetNumQuestItemDrops([index]): the quest's drops not yet carried; a drop of the source item
    // counts only above one. No index reads the selected quest.
    g.set(
        "GetNumQuestItemDrops",
        lua.create_function(|lua, args: MultiValue| {
            let drops = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let quest = match number(lua, &args, 0) {
                    Some(i) => row_quest(&model, i),
                    None => row_quest(&model, i64::from(model.quest_log_selection)),
                };
                quest.and_then(|q| model.wrath_quests.item_drops.get(&q).cloned())
            };
            let Some((source, drops)) = drops else {
                return Ok(0.0);
            };
            let n = drops
                .iter()
                .filter(|&&(item, count)| {
                    item > 0 && (item != source || count > 1) && item_count(lua, item) == 0
                })
                .count();
            Ok(n as f64)
        })?,
    )?;
    // GetQuestLogItemDrop(dropIndex, questIndex): `QUEST_ITEMS_NEEDED` for the drop-th non-empty
    // slot, then whether it is done.
    g.set(
        "GetQuestLogItemDrop",
        lua.create_function(|lua, args: MultiValue| {
            let (Some(drop), Some(index)) = (number(lua, &args, 0), number(lua, &args, 1)) else {
                return Err(mlua::Error::runtime("Usage: GetQuestLogItemDrop(index)"));
            };
            let found = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                row_quest(&model, index)
                    .and_then(|q| model.wrath_quests.item_drops.get(&q))
                    .and_then(|(_, drops)| {
                        let k = usize::try_from(drop - 1).ok()?;
                        drops.iter().filter(|(item, _)| *item > 0).nth(k).copied()
                    })
                    .map(|(item, need)| {
                        let name = model
                            .item_templates
                            .get(&item)
                            .map(|t| t.name.clone())
                            .unwrap_or_default();
                        (item, need, name)
                    })
            };
            let Some((item, need, name)) = found else {
                return Ok(MultiValue::from_vec(vec![
                    Value::Nil,
                    Value::Nil,
                    Value::Nil,
                ]));
            };
            let have = item_count(lua, item).min(i64::from(need));
            let format = lua
                .globals()
                .get::<Option<String>>("QUEST_ITEMS_NEEDED")
                .ok()
                .flatten()
                .unwrap_or_else(|| "%s: %d/%d".into());
            let text = format
                .replacen("%s", &name, 1)
                .replacen("%d", &have.to_string(), 1)
                .replacen("%d", &need.to_string(), 1);
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(&text)?),
                Value::String(lua.create_string("")?),
                if i64::from(need) <= have {
                    Value::Number(1.0)
                } else {
                    Value::Nil
                },
            ]))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::quest_log::{QuestLogEntryView, QuestLogState};
    use crate::script::{ScriptDialect, UnitState};

    fn vm() -> UiScript {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.set_unit(
            "player",
            Some(UnitState {
                exists: true,
                level: 20,
                ..Default::default()
            }),
        );
        s.set_quest_log(QuestLogState {
            entries: vec![
                QuestLogEntryView {
                    is_header: true,
                    title: "Elwynn".into(),
                    ..Default::default()
                },
                QuestLogEntryView {
                    quest_id: 7,
                    title: "Wolves".into(),
                    level: 23,
                    ..Default::default()
                },
            ],
            num_quests: 1,
            ..Default::default()
        });
        s.run(
            "GetQuestGreenRange = function() return 5 end GetItemCount = function() return 0 end",
        )
        .unwrap();
        s
    }

    #[test]
    fn the_link_colours_by_level_and_the_sort_index_counts_headers() {
        let s = vm();
        assert_eq!(
            s.eval::<String>("return GetQuestLink(2)").unwrap(),
            "|cffff8040|Hquest:7:23|h[Wolves]|h|r"
        );
        assert!(s.eval::<bool>("return GetQuestLink(1) == nil").unwrap());
        assert_eq!(s.eval::<f64>("return GetQuestSortIndex(2)").unwrap(), 1.0);
        assert_eq!(difficulty_color(10, 20, 5), "|cff808080");
        assert_eq!(difficulty_color(15, 20, 5), "|cff40c040");
    }

    #[test]
    fn drops_count_what_is_not_carried_and_format_their_progress() {
        let mut s = vm();
        let mut drops = HashMap::new();
        drops.insert(7, (500, vec![(500, 1), (600, 3), (0, 0), (0, 0)]));
        s.set_wrath_quest_item_drops(drops);
        assert_eq!(
            s.eval::<f64>("return GetNumQuestItemDrops(2)").unwrap(),
            1.0
        );
        assert!(s
            .eval::<bool>(
                "local t, x, done = GetQuestLogItemDrop(2, 2) \
                 return t == ': 0/3' and x == '' and done == nil"
            )
            .unwrap());
        s.set_wrath_quest_panel_flags(0x2000);
        assert_eq!(s.eval::<f64>("return QuestFlagsPVP()").unwrap(), 1.0);
    }
}
