//! Build-12340 quest POI and quest-log special items (WorldMapFrame, WatchFrame, QuestLogFrame;
//! one Wow.exe round: `/tmp/azb-ui-questpoi-native.log`, `/tmp/azb-ui-questpoi-inner.log`).
//!
//! The POIs are `SMSG_QUEST_POI_QUERY_RESPONSE`'s blobs as the host publishes them; the query
//! itself is engine-side (native `0x5e6940` runs on every quest-log rebuild, not from a Lua
//! binding), so App sends it. A blob counts on the displayed map when its points' centroid
//! projects onto it through the world-map projector (native tests the blob against the zone or
//! dungeon floor, `0x5e0110`/`0x5e0180`). Labelled: `QuestPOIUpdateIcons` (`0x5e2eb0`) and
//! `SortQuestWatches`' sort (`0x5e5c10`) are unread.

use mlua::{Lua, MultiValue, Table, Value};

use super::Model;

/// One POI blob of a quest (AC `quest_poi`, its points `quest_poi_points`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathQuestPoiBlob {
    /// The objective it marks, -1 the turn-in.
    pub objective_index: i32,
    pub map_id: u32,
    /// WorldMapArea.dbc id.
    pub area_id: u32,
    pub floor: u32,
    /// World x, y.
    pub points: Vec<(i32, i32)>,
}

/// A quest's POIs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathQuestPoi {
    pub quest_id: u32,
    pub blobs: Vec<WrathQuestPoiBlob>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    pois: Vec<WrathQuestPoi>,
    /// Quest id → the bag slot holding its special item.
    special_items: Vec<(u32, (i64, u32))>,
    /// `QuestMapUpdateAllQuests`' visible quests, in quest-log order (`0xc23b28`).
    visible: Vec<u32>,
}

impl super::UiScript {
    /// Publish the quest POIs, whole.
    pub fn set_wrath_quest_pois(&mut self, pois: Vec<WrathQuestPoi>) {
        self.model_mut().wrath_quest_poi.pois = pois;
    }

    /// Publish where each quest's special item sits in the bags (quest id, bag, slot).
    pub fn set_wrath_quest_special_items(&mut self, items: Vec<(u32, i64, u32)>) {
        self.model_mut().wrath_quest_poi.special_items =
            items.into_iter().map(|(q, b, s)| (q, (b, s))).collect();
    }
}

/// The quest's incomplete-objective mask (`0x5e2950`): -1 when complete.
fn incomplete_mask(m: &Model, quest_id: u32) -> Option<u32> {
    let e = m.quest_log.entries.iter().find(|e| !e.is_header && e.quest_id == quest_id)?;
    if e.complete == 1 {
        return Some(u32::MAX);
    }
    Some(
        e.objectives
            .iter()
            .enumerate()
            .filter(|(_, o)| !o.finished)
            .fold(0, |mask, (i, _)| mask | 1 << (i & 31)),
    )
}

/// The blob a quest's icon marks on the displayed map, projected: the turn-in when complete,
/// else the first blob of an incomplete objective.
fn icon(m: &Model, quest_id: u32) -> Option<(bool, f32, f32, i32)> {
    let mask = incomplete_mask(m, quest_id)?;
    let complete = mask == u32::MAX;
    let poi = m.wrath_quest_poi.pois.iter().find(|p| p.quest_id == quest_id)?;
    let project = m.world_loc_projector.as_ref()?;
    let wm = &m.worldmap;
    let selection = (wm.selection.0, wm.selection.1, wm.direct_area);
    poi.blobs
        .iter()
        .filter(|b| {
            if complete {
                b.objective_index == -1
            } else {
                b.objective_index >= 0 && mask & (1 << (b.objective_index & 31)) != 0
            }
        })
        .find_map(|b| {
            let n = b.points.len().max(1) as f32;
            let (sx, sy) = b.points.iter().fold((0.0, 0.0), |(x, y), p| (x + p.0 as f32, y + p.1 as f32));
            let (u, v) = project(selection, b.map_id, sx / n, sy / n)?;
            (u * v).abs().gt(&f32::EPSILON).then_some((complete, u, v, b.objective_index))
        })
}

fn log_index(m: &Model, quest_id: u32) -> Option<usize> {
    m.quest_log
        .entries
        .iter()
        .position(|e| !e.is_header && e.quest_id == quest_id)
        .map(|i| i + 1)
}

/// A quest-log row's special item slot, by 1-based log index (`0x5e0da0`).
fn special_slot(m: &Model, index: i64) -> Option<(i64, u32)> {
    let entry = usize::try_from(index - 1).ok().and_then(|i| m.quest_log.entries.get(i))?;
    m.wrath_quest_poi
        .special_items
        .iter()
        .find(|(q, _)| *q == entry.quest_id && !entry.is_header)
        .map(|(_, at)| *at)
        .filter(|(bag, slot)| {
            m.containers
                .get(bag)
                .and_then(|c| c.slots.get(slot))
                .is_some_and(|s| s.item_id != 0)
        })
}

fn number(lua: &Lua, v: Value, usage: &str) -> mlua::Result<i64> {
    lua.coerce_number(v)?
        .map(|n| n.round_ties_even() as i64)
        .ok_or_else(|| mlua::Error::runtime(usage.to_string()))
}

fn set(
    lua: &Lua,
    name: &str,
    f: impl Fn(&Lua, MultiValue) -> mlua::Result<MultiValue> + Send + 'static,
) -> mlua::Result<()> {
    lua.globals()
        .set(name, lua.create_function(move |lua, args| f(lua, args))?)
}

fn arg(args: &MultiValue, i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Nil)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // QuestMapUpdateAllQuests() (`0x5e63d0` → `0x5e5a50`) → how many log quests have a POI on the
    // displayed map, which become the visible list.
    set(lua, "QuestMapUpdateAllQuests", |lua, _| {
        let mut m = lua.app_data_mut::<Model>().expect("model app_data");
        let visible: Vec<u32> = m
            .quest_log
            .entries
            .iter()
            .filter(|e| !e.is_header)
            .map(|e| e.quest_id)
            .take(25)
            .filter(|&q| icon(&m, q).is_some())
            .collect();
        let n = visible.len() as i64;
        m.wrath_quest_poi.visible = visible;
        Ok(MultiValue::from_vec(vec![Value::Integer(n)]))
    })?;
    // QuestPOIGetQuestIDByVisibleIndex(index) (`0x5e5750`) → questID, quest-log index.
    set(lua, "QuestPOIGetQuestIDByVisibleIndex", |lua, args| {
        let i = number(lua, arg(&args, 0), "Usage: QuestPOIGetQuestIDByVisibleIndex(index)")?;
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        let Some(&quest) = usize::try_from(i - 1).ok().filter(|&i| i < 25).and_then(|i| m.wrath_quest_poi.visible.get(i)) else {
            return Ok(MultiValue::new());
        };
        let index = log_index(&m, quest).map_or(0, |i| i as i64);
        Ok(MultiValue::from_vec(vec![Value::Integer(quest.into()), Value::Integer(index)]))
    })?;
    // QuestPOIGetIconInfo(questID) (`0x5e0590`) → completed, x, y, objective on the displayed
    // map; nothing off it.
    set(lua, "QuestPOIGetIconInfo", |lua, args| {
        let q = number(lua, arg(&args, 0), "Usage: QuestPOIGetIconInfo(questID)")?;
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        let Some((complete, x, y, objective)) = u32::try_from(q).ok().and_then(|q| icon(&m, q)) else {
            return Ok(MultiValue::new());
        };
        Ok(MultiValue::from_vec(vec![
            Value::Boolean(complete),
            Value::Number(f64::from(x)),
            Value::Number(f64::from(y)),
            Value::Integer(objective.into()),
        ]))
    })?;
    // QuestPOIUpdateIcons() (`0x5e5740` → `0x5e2eb0`, labelled: unread): the icons are computed
    // on read here.
    set(lua, "QuestPOIUpdateIcons", |_, _| Ok(MultiValue::new()))?;
    // GetQuestPOILeaderBoard(objective, questIndex) (`0x5e6650`) → text, type, finished (1/nil);
    // three nils for a header or a miss.
    set(lua, "GetQuestPOILeaderBoard", |lua, args| {
        let objective = number(lua, arg(&args, 0), "Usage: GetQuestLogLeaderBoard(index)")?;
        let quest = lua.coerce_number(arg(&args, 1))?.map_or(0, |n| n.round_ties_even() as i64);
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        let line = usize::try_from(quest - 1)
            .ok()
            .and_then(|i| m.quest_log.entries.get(i))
            .filter(|e| !e.is_header)
            .and_then(|e| usize::try_from(objective - 1).ok().and_then(|o| e.objectives.get(o)));
        Ok(MultiValue::from_vec(match line {
            Some(l) => vec![
                Value::String(lua.create_string(&l.text)?),
                Value::String(lua.create_string(&l.kind)?),
                super::binding_abi::flag(l.finished),
            ],
            None => vec![Value::Nil, Value::Nil, Value::Nil],
        }))
    })?;
    // SortQuestWatches() (`0x5e64b0`) → whether the watch order changed. Labelled: the sort
    // `0x5e5c10` is unread; the order stands.
    set(lua, "SortQuestWatches", |_, _| Ok(MultiValue::from_vec(vec![Value::Boolean(false)])))?;

    // ── the quest log's special item: the bag slot the host names ──
    // GetQuestLogSpecialItemInfo(index) (`0x5e52d0`) → link, icon, charges.
    set(lua, "GetQuestLogSpecialItemInfo", |lua, args| {
        let i = number(lua, arg(&args, 0), "Usage: GetQuestLogSpecialItemInfo(index)")?;
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        let Some(slot) = special_slot(&m, i).and_then(|(b, s)| m.containers.get(&b)?.slots.get(&s).cloned()) else {
            return Ok(MultiValue::new());
        };
        let text = |s: &Option<String>| -> mlua::Result<Value> {
            Ok(match s {
                Some(s) => Value::String(lua.create_string(s)?),
                None => Value::Nil,
            })
        };
        Ok(MultiValue::from_vec(vec![
            text(&slot.link)?,
            text(&slot.texture)?,
            Value::Integer(slot.count.into()),
        ]))
    })?;
    // GetQuestLogSpecialItemCooldown(index) (`0x5e53d0`) → start, duration, enable: the slot's
    // cooldown as GetContainerItemCooldown reads it.
    set(lua, "GetQuestLogSpecialItemCooldown", |lua, args| {
        let i = number(lua, arg(&args, 0), "Usage: GetQuestLogSpecialItemCooldown(index)")?;
        let at = {
            let m = lua.app_data_ref::<Model>().expect("model app_data");
            special_slot(&m, i)
        };
        match at {
            Some((bag, slot)) => lua
                .globals()
                .get::<mlua::Function>("GetContainerItemCooldown")?
                .call::<MultiValue>((bag, slot)),
            None => Ok(MultiValue::new()),
        }
    })?;
    // IsQuestLogSpecialItemInRange(index, unit) (`0x5e54c0`) → 1, 0 or nil. Labelled: the item
    // spell's range check `0x809610` needs host range data; this answers nil, "no range".
    set(lua, "IsQuestLogSpecialItemInRange", |lua, args| {
        number(lua, arg(&args, 0), "Usage: IsQuestLogSpecialItemInRange(index, unit)")?;
        Ok(MultiValue::from_vec(vec![Value::Nil]))
    })?;
    // UseQuestLogSpecialItem(index) (`0x5e5640`): the item used, as from its bag slot.
    set(lua, "UseQuestLogSpecialItem", |lua, args| {
        let i = number(lua, arg(&args, 0), "Usage: UseQuestLogSpecialItem(index)")?;
        let at = {
            let m = lua.app_data_ref::<Model>().expect("model app_data");
            special_slot(&m, i)
        };
        if let Some((bag, slot)) = at {
            lua.globals()
                .get::<mlua::Function>("UseContainerItem")?
                .call::<()>((bag, slot))?;
        }
        Ok(MultiValue::new())
    })?;
    // GameTooltip:SetQuestLogSpecialItem(index) (`0x630990`): the special item's tooltip.
    let tooltip: Table = lua.named_registry_value(super::tooltip::REG_TOOLTIP_METHODS)?;
    tooltip.set(
        "SetQuestLogSpecialItem",
        lua.create_function(|lua, (this, index): (Table, Value)| {
            let name: Option<String> = mlua::ObjectLike::call_method(&this, "GetName", ())?;
            let i = number(
                lua,
                index,
                &format!("Usage: {}:SetQuestLogSpecialItem(index)", name.unwrap_or_else(|| "<unnamed>".into())),
            )?;
            let at = {
                let m = lua.app_data_ref::<Model>().expect("model app_data");
                special_slot(&m, i)
            };
            if let Some((bag, slot)) = at {
                mlua::ObjectLike::call_method::<()>(&this, "SetBagItem", (bag, slot))?;
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{
        ContainerSlot, ContainerState, QuestLogEntryView, QuestLogObjectiveView, QuestLogState,
        ScriptDialect, UiScript,
    };

    fn vm() -> UiScript {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.set_quest_log(QuestLogState {
            entries: vec![
                QuestLogEntryView { title: "Zone".into(), is_header: true, ..Default::default() },
                QuestLogEntryView {
                    quest_id: 10,
                    title: "Kill".into(),
                    objectives: vec![
                        QuestLogObjectiveView { text: "Wolves: 2/5".into(), kind: "monster".into(), finished: false, ..Default::default() },
                        QuestLogObjectiveView { text: "Bears: 5/5".into(), kind: "monster".into(), finished: true, ..Default::default() },
                    ],
                    ..Default::default()
                },
                QuestLogEntryView { quest_id: 11, title: "Done".into(), complete: 1, ..Default::default() },
                QuestLogEntryView { quest_id: 12, title: "Elsewhere".into(), ..Default::default() },
            ],
            num_quests: 3,
            ..Default::default()
        });
        // Map 0 projects to the displayed map; anything else is off it.
        s.set_world_loc_projector(Box::new(|_, map, x, y| (map == 0).then_some((x / 100.0, y / 100.0))));
        let blob = |objective_index, map_id, points: Vec<(i32, i32)>| WrathQuestPoiBlob {
            objective_index,
            map_id,
            points,
            ..Default::default()
        };
        s.set_wrath_quest_pois(vec![
            WrathQuestPoi { quest_id: 10, blobs: vec![blob(1, 0, vec![(10, 10)]), blob(0, 0, vec![(20, 40), (40, 60)])] },
            WrathQuestPoi { quest_id: 11, blobs: vec![blob(-1, 0, vec![(50, 50)])] },
            WrathQuestPoi { quest_id: 12, blobs: vec![blob(0, 1, vec![(50, 50)])] },
        ]);
        s
    }

    #[test]
    fn visible_quests_icons_and_leaderboards() {
        let s = vm();
        for check in [
            "QuestMapUpdateAllQuests() == 2",
            "select(1, QuestPOIGetQuestIDByVisibleIndex(1)) == 10 and select(2, QuestPOIGetQuestIDByVisibleIndex(1)) == 2",
            "select(1, QuestPOIGetQuestIDByVisibleIndex(2)) == 11 and select('#', QuestPOIGetQuestIDByVisibleIndex(3)) == 0",
            "select(1, QuestPOIGetIconInfo(10)) == false and math.abs(select(2, QuestPOIGetIconInfo(10)) - 0.3) < 1e-6 and select(4, QuestPOIGetIconInfo(10)) == 0",
            "select(1, QuestPOIGetIconInfo(11)) == true and select(4, QuestPOIGetIconInfo(11)) == -1",
            "select('#', QuestPOIGetIconInfo(12)) == 0",
            "select(1, GetQuestPOILeaderBoard(1, 2)) == 'Wolves: 2/5' and select(3, GetQuestPOILeaderBoard(2, 2)) == 1",
            "select('#', GetQuestPOILeaderBoard(1, 1)) == 3 and GetQuestPOILeaderBoard(1, 1) == nil",
            "SortQuestWatches() == false and select('#', QuestPOIUpdateIcons()) == 0",
        ] {
            assert!(s.eval::<bool>(&format!("return {check}")).unwrap(), "{check}");
        }
        assert!(s.run("QuestPOIGetIconInfo()").is_err() && s.run("GetQuestPOILeaderBoard()").is_err());
    }

    #[test]
    fn special_items_read_their_bag_slot() {
        let mut s = vm();
        assert!(s.eval::<bool>("return select('#', GetQuestLogSpecialItemInfo(2)) == 0").unwrap());
        let mut bag = ContainerState { num_slots: 4, ..Default::default() };
        bag.slots.insert(
            2,
            ContainerSlot {
                item_id: 33,
                count: 1,
                texture: Some("Interface\\Icons\\INV_Misc_Rune_01".into()),
                link: Some("|Hitem:33|h[Rune]|h".into()),
                ..Default::default()
            },
        );
        s.set_container(0, Some(bag));
        s.set_wrath_quest_special_items(vec![(10, 0, 2)]);
        assert!(s
            .eval::<bool>("local l, t, c = GetQuestLogSpecialItemInfo(2) return l == '|Hitem:33|h[Rune]|h' and t:find('Rune') ~= nil and c == 1 and select('#', GetQuestLogSpecialItemInfo(3)) == 0 and IsQuestLogSpecialItemInRange(2, 'target') == nil")
            .unwrap());
        s.run("GetQuestLogSpecialItemCooldown(2) UseQuestLogSpecialItem(2) UseQuestLogSpecialItem(4)").unwrap();
        assert!(s.run("UseQuestLogSpecialItem()").is_err());
    }
}
