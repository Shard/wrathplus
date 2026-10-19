//! Build 12340's gameplay-state readers over state the app pushes: `GetQuestResetTime`
//! (`0x5e6de0`), `GetMirrorTimerInfo` (`0x51cc10`) and `GetMirrorTimerProgress` (`0x517aa0`),
//! `IsPVPTimerRunning` (`0x51bd60`) and `GetPVPTimer` (`0x51bd00` → `0x6de7a0`), and
//! `GetInstanceInfo` (`0x51a8c0`). Evidence: `/tmp/azb-app-mig/ghidra/shim{1,2}.log`.
//!
//! Labelled: the writers of the daily reset (`0x5dff40`), of the mirror-timer table (`0x513ea0`)
//! and of the PvP timer's end (`player+0x1940`) are not decompiled; the app stamps them on the
//! packet or flag edge that carries them.

use mlua::{Lua, MultiValue, Value};

use super::{Model, UiScript};

/// One mirror-timer slot (`0xbd0b80`, 7 words), the slot being the timer's type.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathMirrorTimer {
    /// The value at the start, ms.
    pub value: i32,
    pub max_value: i32,
    /// ms of value per ms of time, signed.
    pub scale: i32,
    pub paused: bool,
    /// The owning spell's name, else `<NAME>_LABEL`.
    pub label: String,
    /// `GetTime()` at the start, seconds.
    pub started: f64,
}

/// The current map's `GetInstanceInfo` row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathInstanceInfo {
    /// `Map.dbc` name.
    pub name: String,
    /// `Map.dbc` `InstanceType`.
    pub instance_type: u32,
    /// The instance difficulty, 0-based (`0xbd0894`); Lua sees it plus one.
    pub difficulty: u32,
    /// The `MapDifficulty` row's `(difficulty string key, max players)` for the map and difficulty.
    pub map_difficulty: Option<(String, i32)>,
    /// `0xbd1980`, `SMSG_INSTANCE_DIFFICULTY`'s second word.
    pub dynamic_difficulty: u32,
    /// `Map.dbc` `Flags & 0x100`.
    pub is_dynamic: bool,
}

#[derive(Default)]
pub(crate) struct State {
    /// The daily reset, Unix seconds (`0xc23aec`).
    quest_reset_at: i64,
    mirror: [Option<WrathMirrorTimer>; 3],
    /// `None` without a player.
    pvp: Option<PvpTimer>,
    instance: Option<WrathInstanceInfo>,
    /// Each bag's `BagFamily`, `GetContainerNumFreeSlots`'s second value.
    bag_families: std::collections::HashMap<i64, u32>,
    /// The current area's `AreaTable` flag `0x8000000`.
    can_hearth_and_resurrect: bool,
    /// The non-zero `PLAYER_FIELD_DAILY_QUESTS` slots.
    daily_quests_completed: u32,
    /// The gossip NPC's creature `type_flags & 0x8000000`.
    force_gossip: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PvpTimer {
    /// `PLAYER_FLAGS_PVP_TIMER` (`0x40000`).
    running: bool,
    /// `GetTime()` when the timer ends; `None` for no end.
    ends: Option<f64>,
}

/// `0x513e00`: a slot's name.
fn mirror_name(slot: usize) -> &'static str {
    match slot {
        0 => "EXHAUSTION",
        1 => "BREATH",
        2 => "FEIGNDEATH",
        _ => "UNKNOWN",
    }
}

/// `0xac8074`: `GetInstanceInfo`'s type word, `"none"` from 5 up.
fn instance_type_word(ty: u32) -> &'static str {
    match ty {
        1 => "party",
        2 => "raid",
        3 => "pvp",
        4 => "arena",
        _ => "none",
    }
}

impl UiScript {
    /// The daily quest reset as Unix seconds, off `SMSG_QUERY_TIME_RESPONSE`.
    pub fn set_wrath_quest_reset_at(&mut self, unix_secs: i64) {
        self.model_mut().wrath_state.quest_reset_at = unix_secs;
    }

    /// A mirror-timer slot by wire type (0 fatigue, 1 breath, 2 feign death); `None` stops it.
    pub fn set_wrath_mirror_timer(&mut self, kind: usize, timer: Option<WrathMirrorTimer>) {
        if let Some(slot) = self.model_mut().wrath_state.mirror.get_mut(kind) {
            *slot = timer;
        }
    }

    /// `SMSG_PAUSE_MIRROR_TIMER` on a running slot.
    pub fn set_wrath_mirror_timer_paused(&mut self, kind: usize, paused: bool) {
        if let Some(Some(t)) = self.model_mut().wrath_state.mirror.get_mut(kind) {
            t.paused = paused;
        }
    }

    /// The player's PvP timer: the flag and its end on the `GetTime()` clock; `None` without a
    /// player.
    pub fn set_wrath_pvp_timer(&mut self, timer: Option<(bool, Option<f64>)>) {
        self.model_mut().wrath_state.pvp = timer.map(|(running, ends)| PvpTimer { running, ends });
    }

    /// A bag's `BagFamily`.
    pub fn set_wrath_bag_family(&mut self, bag: i64, family: u32) {
        self.model_mut()
            .wrath_state
            .bag_families
            .insert(bag, family);
    }

    /// `CanHearthAndResurrectFromArea`'s answer for the current area.
    pub fn set_wrath_can_hearth_and_resurrect(&mut self, can: bool) {
        self.model_mut().wrath_state.can_hearth_and_resurrect = can;
    }

    /// `ForceGossip`'s answer for the open gossip NPC.
    pub fn set_wrath_force_gossip(&mut self, force: bool) {
        self.model_mut().wrath_state.force_gossip = force;
    }

    /// `GetDailyQuestsCompleted`'s count.
    pub fn set_wrath_daily_quests_completed(&mut self, n: u32) {
        self.model_mut().wrath_state.daily_quests_completed = n;
    }

    /// The current map's instance row; `None` for a map with no `Map.dbc` row.
    pub fn set_wrath_instance_info(&mut self, info: Option<WrathInstanceInfo>) {
        self.model_mut().wrath_state.instance = info;
    }
}

fn number(lua: &Lua, args: &MultiValue, i: usize) -> Option<f64> {
    args.get(i)
        .and_then(|v| lua.coerce_number(v.clone()).ok().flatten())
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // GetQuestResetTime(): the reset less `time(NULL)`, the local wall clock.
    g.set(
        "GetQuestResetTime",
        lua.create_function(|lua, ()| {
            let at = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_state
                .quest_reset_at;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            Ok((at - now) as f64)
        })?,
    )?;
    // GetMirrorTimerInfo(index): slot `index - 1`'s name, value, max, scale, paused and label.
    g.set(
        "GetMirrorTimerInfo",
        lua.create_function(|lua, args: MultiValue| {
            let slot = number(lua, &args, 0)
                .map(|n| n as i64 - 1)
                .and_then(|i| usize::try_from(i).ok())
                .filter(|&i| i < 3)
                .ok_or_else(|| mlua::Error::runtime("Usage: GetMirrorTimerInfo(index)"))?;
            let timer = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_state
                .mirror[slot]
                .clone();
            let s = |t: &str| lua.create_string(t).map(Value::String);
            Ok(MultiValue::from_vec(match timer {
                Some(t) => vec![
                    s(mirror_name(slot))?,
                    Value::Number(f64::from(t.value)),
                    Value::Number(f64::from(t.max_value)),
                    Value::Number(f64::from(t.scale)),
                    Value::Number(if t.paused { 1.0 } else { 0.0 }),
                    s(&t.label)?,
                ],
                // Labelled: an idle slot reads as `"UNKNOWN"` with zeroed words.
                None => vec![
                    s("UNKNOWN")?,
                    Value::Number(0.0),
                    Value::Number(0.0),
                    Value::Number(0.0),
                    Value::Number(0.0),
                    s("")?,
                ],
            }))
        })?,
    )?;
    // GetMirrorTimerProgress(name): `(now - start) * scale + value`, ms (`0x517aa0`).
    g.set(
        "GetMirrorTimerProgress",
        lua.create_function(|lua, name: Option<String>| {
            let slot = name
                .as_deref()
                .and_then(|n| (0..3).find(|&i| mirror_name(i) == n))
                .ok_or_else(|| mlua::Error::runtime("Usage: GetMirrorTimerProgress(\"timer\")"))?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(match &model.wrath_state.mirror[slot] {
                Some(t) => {
                    let elapsed_ms = ((model.now - t.started) * 1000.0).round();
                    elapsed_ms * f64::from(t.scale) + f64::from(t.value)
                }
                None => 0.0,
            })
        })?,
    )?;
    // IsPVPTimerRunning(): 1 while `PLAYER_FLAGS & 0x40000`, else nil.
    g.set(
        "IsPVPTimerRunning",
        lua.create_function(|lua, ()| {
            let running = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_state
                .pvp
                .is_some_and(|p| p.running);
            Ok(running.then_some(1.0))
        })?,
    )?;
    // GetPVPTimer(): `0x6de7a0` plus 1000; 300000 with the flag clear, the ms left while it runs,
    // -1 past its end; 0 without a player.
    g.set(
        "GetPVPTimer",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(match model.wrath_state.pvp {
                None => 0.0,
                Some(PvpTimer { running: false, .. }) => 301_000.0,
                Some(PvpTimer { ends, .. }) => match ends {
                    Some(end) if end > model.now => ((end - model.now) * 1000.0).round() + 1000.0,
                    _ => 999.0,
                },
            })
        })?,
    )?;
    // GetInstanceInfo(): name, type, difficulty + 1, difficulty name, max players, dynamic
    // difficulty, isDynamic; a map with no row reads nil, "none", 0, "", 0, 0, nil.
    g.set(
        "GetInstanceInfo",
        lua.create_function(|lua, ()| {
            let info = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_state
                .instance
                .clone();
            let s = |t: &str| lua.create_string(t).map(Value::String);
            let Some(info) = info else {
                return Ok(MultiValue::from_vec(vec![
                    Value::Nil,
                    s("none")?,
                    Value::Number(0.0),
                    s("")?,
                    Value::Number(0.0),
                    Value::Number(0.0),
                    Value::Nil,
                ]));
            };
            // `0x819d40`: the row's string key resolved as a global string. Labelled: a key with no
            // global string reads empty.
            let (difficulty_name, max_players) = match &info.map_difficulty {
                Some((key, max)) => (
                    lua.globals()
                        .get::<Option<String>>(key.as_str())
                        .ok()
                        .flatten()
                        .unwrap_or_default(),
                    f64::from(*max),
                ),
                None => (String::new(), 0.0),
            };
            Ok(MultiValue::from_vec(vec![
                s(&info.name)?,
                s(instance_type_word(info.instance_type))?,
                Value::Number(f64::from(info.difficulty) + 1.0),
                s(&difficulty_name)?,
                Value::Number(max_players),
                Value::Number(f64::from(info.dynamic_difficulty)),
                Value::Boolean(info.is_dynamic),
            ]))
        })?,
    )?;
    // GetContainerNumFreeSlots(bag) (`0x5d7590`): the empty slots and the bag's `BagFamily`.
    // Labelled: the vault (-1) reads as pushed, where the client also needs the bank open.
    g.set(
        "GetContainerNumFreeSlots",
        lua.create_function(|lua, args: MultiValue| {
            let bag = number(lua, &args, 0)
                .map(|n| n as i64)
                .ok_or_else(|| mlua::Error::runtime("Usage: GetContainerFreeSlots(index)"))?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Some(c) = model.containers.get(&bag) else {
                return Ok(MultiValue::from_vec(vec![Value::Number(0.0)]));
            };
            let free = (1..=c.num_slots)
                .filter(|s| !c.slots.contains_key(s))
                .count();
            let family = model
                .wrath_state
                .bag_families
                .get(&bag)
                .copied()
                .unwrap_or(0);
            Ok(MultiValue::from_vec(vec![
                Value::Number(free as f64),
                Value::Number(f64::from(family)),
            ]))
        })?,
    )?;
    // IsEquippableItem(item) (`0x511150`): the template's `InventoryType` is not 0. Labelled: read
    // as `GetItemInfo`'s equip location, which is empty for type 0.
    g.set(
        "IsEquippableItem",
        lua.create_function(|lua, item: Value| {
            let Ok(info) = lua.globals().get::<mlua::Function>("GetItemInfo") else {
                return Ok(Value::Nil);
            };
            let v: MultiValue = info.call(item).unwrap_or_default();
            let equip = v.get(8).and_then(|x| match x {
                Value::String(s) => Some(s.to_string_lossy()),
                _ => None,
            });
            Ok(if equip.is_some_and(|e| !e.is_empty()) {
                Value::Number(1.0)
            } else {
                Value::Nil
            })
        })?,
    )?;
    g.set(
        "CanHearthAndResurrectFromArea",
        lua.create_function(|lua, ()| {
            let can = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_state
                .can_hearth_and_resurrect;
            Ok(can.then_some(1.0))
        })?,
    )?;
    g.set(
        "GetDailyQuestsCompleted",
        lua.create_function(|lua, ()| {
            Ok(f64::from(
                lua.app_data_ref::<Model>()
                    .expect("model app_data")
                    .wrath_state
                    .daily_quests_completed,
            ))
        })?,
    )?;
    g.set(
        "ForceGossip",
        lua.create_function(|lua, ()| {
            Ok(lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_state
                .force_gossip)
        })?,
    )?;
    // GetContainerItemQuestInfo(bag, slot) (`0x5d9400`): the item's template, `Bonding` 4 for a
    // quest item and its `StartQuest`, and whether that quest is in the log (`0x5e11d0`).
    // Labelled: `0x5e11d0` is read as the quest's presence in the quest log.
    g.set(
        "GetContainerItemQuestInfo",
        lua.create_function(|lua, args: MultiValue| {
            let (Some(bag), Some(slot)) = (number(lua, &args, 0), number(lua, &args, 1)) else {
                return Err(mlua::Error::runtime(
                    "Usage: GetContainerItemQuestInfo(bag, slot)",
                ));
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let template = model
                .containers
                .get(&(bag as i64))
                .and_then(|c| c.slots.get(&(slot as u32)))
                .and_then(|s| model.item_templates.get(&s.item_id));
            let Some(t) = template else {
                return Ok(MultiValue::from_vec(vec![
                    Value::Nil,
                    Value::Nil,
                    Value::Nil,
                ]));
            };
            let quest_item = if t.bonding == 4 {
                Value::Boolean(true)
            } else {
                Value::Nil
            };
            Ok(MultiValue::from_vec(if t.start_quest == 0 {
                vec![quest_item, Value::Nil, Value::Nil]
            } else {
                let active = model
                    .quest_log
                    .entries
                    .iter()
                    .any(|e| !e.is_header && e.quest_id == t.start_quest);
                vec![
                    quest_item,
                    Value::Number(f64::from(t.start_quest)),
                    Value::Boolean(active),
                ]
            }))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::ScriptDialect;

    fn vm() -> UiScript {
        UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap()
    }

    #[test]
    fn the_quest_reset_counts_down_on_the_wall_clock() {
        let mut s = vm();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        s.set_wrath_quest_reset_at(now + 3600);
        let left = s.eval::<f64>("return GetQuestResetTime()").unwrap();
        assert!((3598.0..=3600.0).contains(&left), "{left}");
    }

    #[test]
    fn a_mirror_timer_reads_its_slot_and_integrates_its_rate() {
        let mut s = vm();
        s.set_now(100.0);
        s.set_wrath_mirror_timer(
            1,
            Some(WrathMirrorTimer {
                value: 60_000,
                max_value: 60_000,
                scale: -1,
                paused: false,
                label: "Breath".into(),
                started: 100.0,
            }),
        );
        assert!(s
            .eval::<bool>(
                "local n, v, m, sc, p, l = GetMirrorTimerInfo(2) \
                 return n == 'BREATH' and v == 60000 and m == 60000 and sc == -1 and p == 0 \
                 and l == 'Breath'"
            )
            .unwrap());
        assert_eq!(
            s.eval::<String>("return (GetMirrorTimerInfo(1))").unwrap(),
            "UNKNOWN"
        );
        s.set_now(102.5);
        assert_eq!(
            s.eval::<f64>("return GetMirrorTimerProgress('BREATH')")
                .unwrap(),
            57_500.0
        );
        assert!(s.run("GetMirrorTimerInfo(4)").is_err());
        assert!(s.run("GetMirrorTimerProgress('UNKNOWN')").is_err());
    }

    #[test]
    fn the_pvp_timer_reads_the_flag_and_its_end() {
        let mut s = vm();
        assert_eq!(s.eval::<f64>("return GetPVPTimer()").unwrap(), 0.0);
        s.set_wrath_pvp_timer(Some((false, None)));
        assert_eq!(s.eval::<f64>("return GetPVPTimer()").unwrap(), 301_000.0);
        assert!(s.eval::<bool>("return IsPVPTimerRunning() == nil").unwrap());
        s.set_now(10.0);
        s.set_wrath_pvp_timer(Some((true, Some(310.0))));
        assert_eq!(s.eval::<f64>("return IsPVPTimerRunning()").unwrap(), 1.0);
        assert_eq!(s.eval::<f64>("return GetPVPTimer()").unwrap(), 301_000.0);
        s.set_now(400.0);
        assert_eq!(s.eval::<f64>("return GetPVPTimer()").unwrap(), 999.0);
    }

    #[test]
    fn instance_info_reads_the_map_and_its_difficulty_row() {
        let mut s = vm();
        assert!(s
            .eval::<bool>(
                "local n, t, d, dn, m, dy, isd = GetInstanceInfo() \
                 return n == nil and t == 'none' and d == 0 and dn == '' and m == 0 and isd == nil"
            )
            .unwrap());
        s.run("RAID_DIFFICULTY_25PLAYER = '25 Player'").unwrap();
        s.set_wrath_instance_info(Some(WrathInstanceInfo {
            name: "Naxxramas".into(),
            instance_type: 2,
            difficulty: 1,
            map_difficulty: Some(("RAID_DIFFICULTY_25PLAYER".into(), 25)),
            dynamic_difficulty: 0,
            is_dynamic: false,
        }));
        assert!(s
            .eval::<bool>(
                "local n, t, d, dn, m, dy, isd = GetInstanceInfo() \
                 return n == 'Naxxramas' and t == 'raid' and d == 2 and dn == '25 Player' \
                 and m == 25 and dy == 0 and isd == false"
            )
            .unwrap());
    }

    #[test]
    fn the_bag_area_and_daily_readers_read_the_pushed_state() {
        let mut s = vm();
        let mut slots = std::collections::HashMap::new();
        slots.insert(2, crate::script::ContainerSlot::default());
        s.set_container(
            1,
            Some(crate::script::ContainerState {
                name: None,
                num_slots: 4,
                slots,
            }),
        );
        s.set_wrath_bag_family(1, 8);
        s.set_wrath_can_hearth_and_resurrect(true);
        s.set_wrath_daily_quests_completed(3);
        assert!(s
            .eval::<bool>(
                "local f, fam = GetContainerNumFreeSlots(1) \
                 return f == 3 and fam == 8 and GetContainerNumFreeSlots(3) == 0 \
                 and CanHearthAndResurrectFromArea() == 1 and GetDailyQuestsCompleted() == 3"
            )
            .unwrap());
        assert!(s.run("GetContainerNumFreeSlots()").is_err());
    }
}
