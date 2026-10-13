//! Build 12340's aura readers: `UnitAura(unit, index [, filter])`, and `UnitBuff`/`UnitDebuff`
//! as its `HELPFUL`/`HARMFUL` forms, returning `name, rank, icon, count, debuffType, duration,
//! expirationTime, unitCaster, isStealable, shouldConsolidate, spellId`.
//!
//! The filter's `HELPFUL`/`HARMFUL` and `CANCELABLE`/`NOT_CANCELABLE` words apply. `PLAYER` and
//! `RAID` do not: [`AuraState`] carries neither the caster nor castability, so they keep every
//! aura, and `unitCaster` is nil.

use mlua::{Lua, MultiValue, Value};

use super::{auras_of, AuraState, Model};

struct Filter {
    helpful: bool,
    cancelable: Option<bool>,
}

fn parse_filter(filter: Option<&str>, default_helpful: bool) -> Filter {
    let mut parsed = Filter {
        helpful: default_helpful,
        cancelable: None,
    };
    for word in filter.unwrap_or("").split(|c: char| c == '|' || c.is_whitespace()) {
        match word.to_ascii_uppercase().as_str() {
            "HELPFUL" => parsed.helpful = true,
            "HARMFUL" => parsed.helpful = false,
            "CANCELABLE" => parsed.cancelable = Some(true),
            "NOT_CANCELABLE" => parsed.cancelable = Some(false),
            _ => {}
        }
    }
    parsed
}

fn returns(lua: &Lua, a: &AuraState) -> mlua::Result<MultiValue> {
    let text = |s: &Option<String>| -> mlua::Result<Value> {
        Ok(match s {
            Some(s) => Value::String(lua.create_string(s)?),
            None => Value::Nil,
        })
    };
    Ok(MultiValue::from_vec(vec![
        Value::String(lua.create_string(a.name.as_deref().unwrap_or(""))?),
        Value::String(lua.create_string("")?),
        text(&a.icon)?,
        Value::Integer(i64::from(a.count)),
        text(&a.debuff_type)?,
        Value::Number(a.duration),
        Value::Number(a.expiration_time),
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Integer(i64::from(a.spell_id)),
    ]))
}

fn find(
    lua: &Lua,
    token: Option<String>,
    which: Value,
    filter: Option<String>,
    default_helpful: bool,
) -> mlua::Result<MultiValue> {
    let filter = parse_filter(filter.as_deref(), default_helpful);
    let hit = {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let Some(token) = token else {
            return Ok(MultiValue::new());
        };
        auras_of(&model, &token)?.and_then(|list| {
            let mut matching = list.iter().filter(|a| {
                a.helpful == filter.helpful
                    && filter.cancelable.is_none_or(|c| a.cancelable == c)
            });
            match &which {
                Value::Integer(i) if *i >= 1 => matching.nth((*i - 1) as usize),
                Value::Number(n) if *n >= 1.0 => matching.nth((*n as usize) - 1),
                Value::String(name) => {
                    let name = name.to_string_lossy();
                    matching.find(|a| a.name.as_deref() == Some(name.as_str()))
                }
                _ => None,
            }
            .cloned()
        })
    };
    match hit {
        Some(a) => returns(lua, &a),
        None => Ok(MultiValue::new()),
    }
}

/// Replace the 1.12 readers with build 12340's.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "UnitAura",
        lua.create_function(
            |lua, (token, which, rank_or_filter, filter): (Option<String>, Value, Option<String>, Option<String>)| {
                // `UnitAura(unit, name, rank, filter)` by name, else `(unit, index, filter)`.
                let filter = if matches!(which, Value::String(_)) { filter } else { rank_or_filter };
                find(lua, token, which, filter, true)
            },
        )?,
    )?;
    for (name, helpful) in [("UnitBuff", true), ("UnitDebuff", false)] {
        g.set(
            name,
            lua.create_function(
                move |lua, (token, which, rank_or_filter, filter): (Option<String>, Value, Option<String>, Option<String>)| {
                    let filter = if matches!(which, Value::String(_)) { filter } else { rank_or_filter };
                    let word = if helpful { "HELPFUL" } else { "HARMFUL" };
                    let filter = Some(format!("{} {word}", filter.unwrap_or_default()));
                    find(lua, token, which, filter, helpful)
                },
            )?,
        )?;
    }
    Ok(())
}
