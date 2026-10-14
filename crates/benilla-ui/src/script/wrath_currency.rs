//! Build-12340 currency display state. No roster or balances are invented before a host push.
//! Contracts: Blizzard_TokenUI.lua:68,180,254,263; Blizzard_TokenUI.xml:331,370;
//! StaticPopup.lua:143-144. Display verbs do not stand in for server operations.

use super::{Model, UiScript};
use mlua::{Lua, MultiValue, Value};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathCurrencyEntry {
    pub name: String,
    pub is_header: bool,
    pub is_expanded: bool,
    pub is_unused: bool,
    pub is_watched: bool,
    pub count: u32,
    /// TokenUI: 1 = arena, 2 = honor, 0 = ordinary item currency.
    pub extra_currency_type: u32,
    pub icon: String,
    pub item_id: u32,
}

/// Ordered headers followed by their entries, including children of collapsed headers.
/// Names/icons/item ids must come from the installed data; counts and caps from host state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathCurrencySnapshot {
    pub entries: Vec<WrathCurrencyEntry>,
    pub honor: (u32, u32),
    pub arena: (u32, u32),
}

impl WrathCurrencySnapshot {
    fn visible(&self) -> Vec<usize> {
        let mut expanded = true;
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                if row.is_header {
                    expanded = row.is_expanded;
                    Some(i)
                } else if expanded {
                    Some(i)
                } else {
                    None
                }
            })
            .collect()
    }

    fn list_index(&self, index: i64) -> Option<usize> {
        let index = usize::try_from(index.checked_sub(1)?).ok()?;
        self.visible().get(index).copied()
    }
}

impl UiScript {
    /// Replace the complete authoritative display snapshot; an empty push clears logout state.
    /// No network request is emitted. Watch slots are bounded by TokenUI's MAX_WATCHED_TOKENS=3.
    pub fn set_wrath_currency_snapshot(&mut self, mut snapshot: WrathCurrencySnapshot) {
        let mut watched = 0;
        for row in &mut snapshot.entries {
            if row.is_header {
                row.is_watched = false;
            } else if row.is_watched {
                watched += 1;
                row.is_watched = watched <= 3;
            }
        }
        self.model_mut().wrath_currency = snapshot;
    }
}

fn flag(value: Value) -> mlua::Result<bool> {
    // Stock TokenUI passes numeric 0/1, not Lua's truthiness (where 0 is true).
    match value {
        Value::Boolean(b) => Ok(b),
        Value::Integer(n) => Ok(n != 0),
        Value::Number(n) => Ok(n != 0.0),
        Value::Nil => Ok(false),
        _ => Err(mlua::Error::external(
            "currency flag must be numeric or boolean",
        )),
    }
}

fn info(lua: &Lua, row: Option<&WrathCurrencyEntry>, backpack: bool) -> mlua::Result<MultiValue> {
    let Some(row) = row else {
        return Ok(MultiValue::from_vec(vec![Value::Nil]));
    };
    let mut out = vec![Value::String(lua.create_string(&row.name)?)];
    if !backpack {
        out.extend([
            Value::Boolean(row.is_header),
            Value::Boolean(row.is_expanded),
            Value::Boolean(row.is_unused),
            Value::Boolean(row.is_watched),
        ]);
    }
    out.extend([
        Value::Integer(row.count.into()),
        Value::Integer(row.extra_currency_type.into()),
        Value::String(lua.create_string(&row.icon)?),
        Value::Integer(row.item_id.into()),
    ]);
    Ok(MultiValue::from_vec(out))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "GetCurrencyListSize",
        lua.create_function(|lua, ()| {
            Ok(lua
                .app_data_ref::<Model>()
                .expect("model")
                .wrath_currency
                .visible()
                .len())
        })?,
    )?;
    lua.globals().set(
        "GetCurrencyListInfo",
        lua.create_function(|lua, index: i64| {
            let model = lua.app_data_ref::<Model>().expect("model");
            let snapshot = &model.wrath_currency;
            info(
                lua,
                snapshot.list_index(index).map(|i| &snapshot.entries[i]),
                false,
            )
        })?,
    )?;
    lua.globals().set(
        "GetBackpackCurrencyInfo",
        lua.create_function(|lua, index: i64| {
            let model = lua.app_data_ref::<Model>().expect("model");
            let row = index
                .checked_sub(1)
                .and_then(|i| usize::try_from(i).ok())
                .filter(|i| *i < 3)
                .and_then(|i| {
                    model
                        .wrath_currency
                        .entries
                        .iter()
                        .filter(|r| !r.is_header && r.is_watched)
                        .nth(i)
                });
            info(lua, row, true)
        })?,
    )?;
    for name in [
        "ExpandCurrencyList",
        "SetCurrencyBackpack",
        "SetCurrencyUnused",
    ] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, (index, value): (i64, Value)| {
                let enabled = flag(value)?;
                let mut model = lua.app_data_mut::<Model>().expect("model");
                let snapshot = &mut model.wrath_currency;
                let Some(i) = snapshot.list_index(index) else {
                    return Ok(());
                };
                match name {
                    "ExpandCurrencyList" if snapshot.entries[i].is_header => {
                        snapshot.entries[i].is_expanded = enabled
                    }
                    "SetCurrencyUnused" if !snapshot.entries[i].is_header => {
                        snapshot.entries[i].is_unused = enabled
                    }
                    "SetCurrencyBackpack" if !snapshot.entries[i].is_header => {
                        let watched = snapshot
                            .entries
                            .iter()
                            .filter(|r| !r.is_header && r.is_watched)
                            .count();
                        if !enabled || watched < 3 || snapshot.entries[i].is_watched {
                            snapshot.entries[i].is_watched = enabled;
                        }
                    }
                    _ => {}
                }
                Ok(())
            })?,
        )?;
    }
    for (name, honor) in [("GetHonorCurrency", true), ("GetArenaCurrency", false)] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, ()| {
                let model = lua.app_data_ref::<Model>().expect("model");
                Ok(if honor {
                    model.wrath_currency.honor
                } else {
                    model.wrath_currency.arena
                })
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
