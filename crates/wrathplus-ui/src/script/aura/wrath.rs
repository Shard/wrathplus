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
    for word in filter
        .unwrap_or("")
        .split(|c: char| c == '|' || c.is_whitespace())
    {
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

/// The aura `UnitAura(unit, index | name, …, filter)` names, shared with
/// `GameTooltip:SetUnitAura`.
pub(crate) fn find_aura(
    model: &Model,
    token: &str,
    which: &Value,
    filter: Option<&str>,
    default_helpful: bool,
) -> mlua::Result<Option<AuraState>> {
    let filter = parse_filter(filter, default_helpful);
    Ok(auras_of(model, token)?.and_then(|list| {
        let mut matching = list.iter().filter(|a| {
            a.helpful == filter.helpful && filter.cancelable.is_none_or(|c| a.cancelable == c)
        });
        match which {
            Value::Integer(i) if *i >= 1 => matching.nth((*i - 1) as usize),
            Value::Number(n) if *n >= 1.0 => matching.nth((*n as usize) - 1),
            Value::String(name) => {
                let name = name.to_string_lossy();
                matching.find(|a| a.name.as_deref() == Some(name.as_str()))
            }
            _ => None,
        }
        .cloned()
    }))
}

fn find(
    lua: &Lua,
    token: Option<String>,
    which: Value,
    filter: Option<String>,
    default_helpful: bool,
) -> mlua::Result<MultiValue> {
    let hit = {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let Some(token) = token else {
            return Ok(MultiValue::new());
        };
        find_aura(&model, &token, &which, filter.as_deref(), default_helpful)?
    };
    match hit {
        Some(a) => returns(lua, &a),
        None => Ok(MultiValue::new()),
    }
}

/// CancelUnitBuff(unit, index | name [, rank] [, filter]) (12340 `0x804220`): on the player, the
/// aura the arguments name (HELPFUL by default) is cancelled when it may be
/// ([`super::cancel_authorized`]). Labelled: the pet and vehicle branch is not carried, and the
/// rank argument is not compared.
fn install_cancel(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "CancelUnitBuff",
        lua.create_function(|lua, args: MultiValue| {
            let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Nil);
            let Some(token) = lua.coerce_string(arg(0))?.map(|s| s.to_string_lossy()) else {
                return Err(mlua::Error::runtime(
                    r#"Usage: CancelUnitBuff("unit", [index] or ["name", "rank"][, "filter"])"#,
                ));
            };
            if !token.eq_ignore_ascii_case("player") {
                return Ok(());
            }
            let which = arg(1);
            // A name's filter follows the rank; an index's follows the index.
            let filter = match &which {
                Value::String(_) => arg(3),
                _ => arg(2),
            };
            let filter = lua.coerce_string(filter)?.map(|s| s.to_string_lossy());
            let filter = parse_filter(filter.as_deref(), true);
            let spell = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
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
                    .filter(|a| super::cancel_authorized(a))
                    .map(|a| a.spell_id)
                })
            };
            if let Some(spell) = spell {
                lua.app_data_mut::<Model>()
                    .expect("model app_data")
                    .cancel_aura_requests
                    .push(spell);
            }
            Ok(())
        })?,
    )
}

/// Replace the 1.12 readers with build 12340's.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    install_cancel(lua)?;
    let g = lua.globals();
    g.set(
        "UnitAura",
        lua.create_function(
            |lua,
             (token, which, rank_or_filter, filter): (
                Option<String>,
                Value,
                Option<String>,
                Option<String>,
            )| {
                // `UnitAura(unit, name, rank, filter)` by name, else `(unit, index, filter)`.
                let filter = if matches!(which, Value::String(_)) {
                    filter
                } else {
                    rank_or_filter
                };
                find(lua, token, which, filter, true)
            },
        )?,
    )?;
    for (name, helpful) in [("UnitBuff", true), ("UnitDebuff", false)] {
        g.set(
            name,
            lua.create_function(
                move |lua,
                      (token, which, rank_or_filter, filter): (
                    Option<String>,
                    Value,
                    Option<String>,
                    Option<String>,
                )| {
                    let filter = if matches!(which, Value::String(_)) {
                        filter
                    } else {
                        rank_or_filter
                    };
                    let word = if helpful { "HELPFUL" } else { "HARMFUL" };
                    let filter = Some(format!("{} {word}", filter.unwrap_or_default()));
                    find(lua, token, which, filter, helpful)
                },
            )?,
        )?;
    }
    Ok(())
}
