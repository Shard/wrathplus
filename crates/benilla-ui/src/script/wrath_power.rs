//! Build-12340 resource readers over profile-native raw snapshots.

use mlua::{Lua, Value};

use super::Model;

const TOKENS: [&str; 7] = [
    "MANA",
    "RAGE",
    "FOCUS",
    "ENERGY",
    "HAPPINESS",
    "RUNES",
    "RUNIC_POWER",
];

fn numeric_index(value: Option<Value>, default: i64) -> i64 {
    match value {
        Some(Value::Integer(value)) => value,
        Some(Value::Number(value)) => value as i64,
        Some(Value::String(value)) => value
            .to_str()
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .map_or(default, |value| value as i64),
        _ => default,
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "UnitPowerType",
        lua.create_function(|lua, (token, index): (String, Option<Value>)| {
            if numeric_index(index, 1) != 1 {
                return Err(mlua::Error::RuntimeError(
                    "indexed Wrath power-display records are unavailable".into(),
                ));
            }
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Some(unit) = model.unit(&token).filter(|u| u.exists) else {
                return Ok((0, ""));
            };
            let ty = unit.power_type;
            Ok((ty, TOKENS.get(usize::from(ty)).copied().unwrap_or("")))
        })?,
    )?;
    for (name, maximum) in [("UnitPower", false), ("UnitPowerMax", true)] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, (token, requested): (String, Option<Value>)| {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let Some(unit) = model.unit(&token).filter(|u| u.exists) else {
                    return Ok(0u32);
                };
                // Native 0x60ed40/0x60ef40: 7 selects the active type; invalid indices return 0.
                let requested = numeric_index(requested, 7);
                if !(0..=7).contains(&requested) {
                    return Ok(0);
                }
                let ty = if requested == 7 {
                    unit.power_type
                } else {
                    requested as u8
                };
                let is_self = unit.guid != 0
                    && model
                        .unit("player")
                        .is_some_and(|player| player.guid == unit.guid);
                if !maximum {
                    // Native 0x4cee50 compares identity, not token spelling.
                    if ty != unit.power_type && !is_self {
                        return Ok(0);
                    }
                    // Native 0x512a30 exempts self and membership-table identities. The latter
                    // are not qualified yet; refuse rather than invent a feigner's visibility.
                    if unit.dynamic_flags & 0x20 != 0 && !is_self {
                        return Err(mlua::Error::RuntimeError(
                            "Wrath flagged-unit power visibility is unavailable".into(),
                        ));
                    }
                }
                let powers = unit.powers.as_ref().ok_or_else(|| {
                    mlua::Error::RuntimeError("Wrath resource snapshot is unavailable".into())
                })?;
                let Some(value) = powers.get(usize::from(ty)) else {
                    return Ok(0);
                };
                // Build-12340 helper 0x7fde00/table0xaf5220; third arguments are ignored.
                let divisor = match ty {
                    1 | 6 => 10,
                    4 => 1000,
                    _ => 1,
                };
                Ok((if maximum {
                    value.maximum
                } else {
                    value.current
                }) / divisor)
            })?,
        )?;
    }
    Ok(())
}
