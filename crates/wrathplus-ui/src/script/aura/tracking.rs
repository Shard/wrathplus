//! Build 12340's minimap tracking menu (`Minimap.lua:408-466`). The list is the player's tracking
//! spells first (`[0xbe8dec]`, `[0xbe8de8]` long), then the rows of the 15-row type table at
//! `0xa11c50` the player's class may use (`0x57e980`, `0x57eb00`): a row whose class mask is 0, or
//! holds `1 << class`. One entry is active at a time. A spell is active while its aura is on the
//! player (`0x727760` → `0x57ea30`, which also drops the selected row); a row is selected
//! client-side (`0x57e070`), never sent to the server, and kept in `minimapTrackedInfo` (registered
//! with an empty default, `0x51f3a9`), whence the load restores it (`0x5128d0` → `0x57ea60`).
//!
//! Evidence: `measure/ghidra-tracking{,2,..,11}.log`. Labelled: the list's admission test is
//! untraced (the spellbook's adder pushes through a helper this round did not reach), so the app
//! feeds the book's spells whose effect is a tracking aura.

use crate::script::model::Model;
use mlua::{Lua, MultiValue, Value};

/// One row of the type table at `0xa11c50`, 20 bytes: kind (1 an NPC flag, 2 a game-object type,
/// 3 trivial quests), the flag mask or object type, the name's global, the texture under
/// `Interface\Minimap\Tracking` and the class mask.
struct Row {
    kind: u8,
    mask: u32,
    key: &'static str,
    texture: &'static str,
    classes: u32,
}

const fn row(kind: u8, mask: u32, key: &'static str, texture: &'static str, classes: u32) -> Row {
    Row {
        kind,
        mask,
        key,
        texture,
        classes,
    }
}

/// The table at `0xa11c50`, in its own order.
const ROWS: [Row; 15] = [
    row(1, 0x1000, "MINIMAP_TRACKING_REPAIR", "Repair", 0),
    row(1, 0x200, "MINIMAP_TRACKING_VENDOR_FOOD", "Food", 0),
    row(1, 0x400, "MINIMAP_TRACKING_VENDOR_POISON", "Poisons", 0x10),
    row(1, 0x100, "MINIMAP_TRACKING_VENDOR_AMMO", "Ammunition", 0x1a),
    row(1, 0x800, "MINIMAP_TRACKING_VENDOR_REAGENT", "Reagents", 0),
    row(1, 0x10000, "MINIMAP_TRACKING_INNKEEPER", "Innkeeper", 0),
    row(
        1,
        0x2000,
        "MINIMAP_TRACKING_FLIGHTMASTER",
        "FlightMaster",
        0,
    ),
    row(
        1,
        0x400000,
        "MINIMAP_TRACKING_STABLEMASTER",
        "StableMaster",
        0x8,
    ),
    row(
        1,
        0x100000,
        "MINIMAP_TRACKING_BATTLEMASTER",
        "BattleMaster",
        0,
    ),
    row(1, 0x20, "MINIMAP_TRACKING_TRAINER_CLASS", "Class", 0),
    row(
        1,
        0x40,
        "MINIMAP_TRACKING_TRAINER_PROFESSION",
        "Profession",
        0,
    ),
    row(1, 0x200000, "MINIMAP_TRACKING_AUCTIONEER", "Auctioneer", 0),
    row(1, 0x20000, "MINIMAP_TRACKING_BANKER", "Banker", 0),
    row(2, 19, "MINIMAP_TRACKING_MAILBOX", "Mailbox", 0),
    row(3, 0, "MINIMAP_TRACKING_TRIVIAL_QUESTS", "TrivialQuests", 0),
];

/// The trivial-quest row, the one kind-3 entry.
const TRIVIAL_QUESTS: usize = 14;

/// The folder every row texture and the empty icon sit in (`0x57f1b0`, `0x57f4f0`).
const FOLDER: &str = "Interface\\Minimap\\Tracking";

/// The player's class bit, 0 with no player (`0x57e980`).
fn class_bit(model: &Model) -> u32 {
    let class = model
        .unit("player")
        .and_then(|u| u.class_file.as_deref())
        .map_or(0, crate::script::unit::wrath_queries::class_id);
    match class {
        0 => 0,
        c => 1 << (c & 0x1f),
    }
}

/// The rows the player's class may use, in table order (`0x57eb00`).
fn rows_for(model: &Model) -> impl Iterator<Item = usize> + '_ {
    let bit = class_bit(model);
    (0..ROWS.len()).filter(move |&i| ROWS[i].classes == 0 || ROWS[i].classes & bit != 0)
}

/// A 1-based menu index, resolved: a tracking spell, or a row of the table.
enum Entry {
    Spell(u32),
    Row(usize),
}

fn entry(model: &Model, index: i64) -> Option<Entry> {
    let i = usize::try_from(index).ok()?.checked_sub(1)?;
    let spells = &model.tracking_spells;
    match spells.get(i) {
        Some(&id) => Some(Entry::Spell(id)),
        None => rows_for(model).nth(i - spells.len()).map(Entry::Row),
    }
}

/// A spell's name and icon, off the spellbook the app pushed.
fn spell_face(model: &Model, id: u32) -> (Option<String>, Option<String>) {
    model
        .spellbook
        .slots
        .iter()
        .find(|s| s.spell_id == id)
        .map_or((None, None), |s| (Some(s.name.clone()), s.texture.clone()))
}

fn opt_string(lua: &Lua, s: Option<String>) -> mlua::Result<Value> {
    s.map_or(Ok(Value::Nil), |s| Ok(Value::String(lua.create_string(s)?)))
}

/// 1 or nil, as the reference pushes `active`.
fn flag(on: bool) -> Value {
    if on {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}

/// Select `row`, or none (`0x57e070`): the `minimapTrackedInfo` write, when the host registers
/// it, and `MINIMAP_UPDATE_TRACKING`.
fn select_row(model: &mut Model, row: Option<usize>) {
    model.wrath_tracking_selection = row;
    if model.cvars.contains_key("minimaptrackedinfo") {
        let value = row.map_or_else(String::new, |r| ROWS[r].key.to_string());
        crate::script::cvars::write_cvar(model, "minimapTrackedInfo", value);
    }
    model
        .pending_events
        .push(("MINIMAP_UPDATE_TRACKING".into(), Vec::new()));
}

impl Model {
    /// The selected row's name global, `GameTooltip:SetTracking`'s line (`0x625940`).
    pub(in crate::script) fn tracking_row_key(&self) -> Option<&'static str> {
        self.wrath_tracking_selection.map(|r| ROWS[r].key)
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // GetNumTrackingTypes() (`0x57f170`): the tracking spells and the rows the class may use.
    g.set(
        "GetNumTrackingTypes",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.tracking_spells.len() + rows_for(&model).count())
        })?,
    )?;

    // GetTrackingInfo(index) (`0x57f1b0`) → name, texture, active, category: a spell's name and
    // icon, active while its aura is the player's tracking aura, "spell"; a row's localised name,
    // its texture under the folder, active while selected, "other". Past the end, nothing.
    g.set(
        "GetTrackingInfo",
        lua.create_function(|lua, index: i64| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let values = match entry(&model, index) {
                None => Vec::new(),
                Some(Entry::Spell(id)) => {
                    let (name, icon) = spell_face(&model, id);
                    let active = model.tracking.as_ref().is_some_and(|t| t.spell_id == id);
                    vec![
                        opt_string(lua, name)?,
                        opt_string(lua, icon)?,
                        flag(active),
                        Value::String(lua.create_string("spell")?),
                    ]
                }
                Some(Entry::Row(r)) => {
                    let name = lua.globals().get::<Option<String>>(ROWS[r].key)?;
                    vec![
                        opt_string(lua, name)?,
                        Value::String(lua.create_string(format!("{FOLDER}\\{}", ROWS[r].texture))?),
                        flag(model.wrath_tracking_selection == Some(r)),
                        Value::String(lua.create_string("other")?),
                    ]
                }
            };
            Ok(MultiValue::from_vec(values))
        })?,
    )?;

    // SetTracking([index]) (`0x57f380`): a spell index casts the spell (`0x80da40`); a row index
    // cancels the tracking aura (`0x802f80`) and selects the row; no index cancels both.
    g.set(
        "SetTracking",
        lua.create_function(|lua, index: Option<i64>| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let found = index.and_then(|i| entry(&model, i));
            match (index, found) {
                (Some(_), None) => {} // past the end: nothing
                (_, Some(Entry::Spell(id))) => {
                    model
                        .script_calls
                        .push(crate::script::calls::ScriptCall::CastSpell {
                            spell_id: id,
                            on_self: false,
                        });
                }
                (_, row) => {
                    if let Some(id) = model.tracking.as_ref().map(|t| t.spell_id) {
                        model.cancel_aura_requests.push(id);
                    }
                    let row = match row {
                        Some(Entry::Row(r)) => Some(r),
                        _ => None,
                    };
                    select_row(&mut model, row);
                }
            }
            Ok(())
        })?,
    )?;

    // GetTrackingTexture() (`0x57f4f0`): the selected row's texture, else the tracking spell's
    // icon, else the folder's `None`; never nil, so `MiniMapTrackingIcon` always draws.
    g.set(
        "GetTrackingTexture",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(match model.wrath_tracking_selection {
                Some(r) => format!("{FOLDER}\\{}", ROWS[r].texture),
                None => model
                    .tracking
                    .as_ref()
                    .and_then(|t| t.icon.clone())
                    .unwrap_or_else(|| format!("{FOLDER}\\None")),
            })
        })?,
    )?;
    Ok(())
}

impl crate::script::UiScript {
    /// The spellbook's tracking spells, in book order, ahead of the table's rows in the menu.
    pub fn set_tracking_spells(&mut self, spells: Vec<u32>) {
        self.model_mut().tracking_spells = spells;
    }

    /// Whether the trivial-quest row is selected: the minimap's filter (`0x57bf30`, kind 3).
    pub fn track_trivial_quests(&self) -> bool {
        self.model_ref().wrath_tracking_selection == Some(TRIVIAL_QUESTS)
    }

    /// The selected town type, `(kind, mask)`: an NPC flag (kind 1) or a game-object type (kind
    /// 2). `None` for no row or the trivial-quest filter.
    pub fn tracked_town_type(&self) -> Option<(u8, u32)> {
        let r = self.model_ref().wrath_tracking_selection?;
        (ROWS[r].kind != 3).then_some((ROWS[r].kind, ROWS[r].mask))
    }

    /// Restore the selected row from `minimapTrackedInfo` (`0x5128d0` → `0x57ea60`): the row whose
    /// key the CVar names, if the player's class may use it; anything else selects nothing.
    pub fn restore_tracking_from_cvar(&mut self) {
        let Some(key) = self.cvar("minimapTrackedInfo").filter(|k| !k.is_empty()) else {
            return;
        };
        let mut model = self.model_mut();
        let row = rows_for(&model).find(|&r| ROWS[r].key.eq_ignore_ascii_case(&key));
        if row.is_some() {
            select_row(&mut model, row);
        }
    }

    /// The minimap's view of the selection: the trivial-quest filter and the town type.
    pub fn tracking_menu(&self) -> (bool, Option<(u8, u32)>) {
        (self.track_trivial_quests(), self.tracked_town_type())
    }
}

#[cfg(test)]
#[path = "tracking_tests.rs"]
mod tests;
