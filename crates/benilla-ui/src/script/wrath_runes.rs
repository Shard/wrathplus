//! Wrath death-knight runes for `RuneFrame.lua`: `GetRuneType` (`0x6131e0`), `GetRuneCooldown`
//! (`0x613020`) and `GetRuneCount` (`0x613140`), over the app's timed rune snapshot. Slots are
//! 1-based; the bindings admit 1..=8 (`slot - 1 < 8`), of which six runes exist.

use mlua::{Lua, MultiValue, Value};

use super::{Model, UiScript};

/// One rune as the client keeps it: its type and when its cooldown began.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RuneView {
    /// 0 blood, 1 unholy, 2 frost, 3 death; `GetRuneType` answers it plus one.
    pub rune_type: u8,
    /// The cooldown's start on the `GetTime` clock; `None` is ready.
    pub start: Option<f64>,
    /// `1 / PLAYER_RUNE_REGEN[type]` seconds, 0 when the rate is not positive (`0x6130b6`).
    pub duration: f32,
}

/// The player's runes; `None` for a character without them (`0x5edfc0`'s gate).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathRunes(pub Option<[RuneView; 6]>);

impl UiScript {
    pub fn set_wrath_runes(&mut self, runes: WrathRunes) {
        self.model_mut().wrath_runes = runes;
    }
}

/// The slot argument (`0x84df20` number test, `0x88b9c0` rounding) as a 0-based index under 8.
fn slot(v: &Value, usage: &str) -> mlua::Result<Option<usize>> {
    let n = match v {
        Value::Integer(i) => *i as f64,
        Value::Number(f) => *f,
        Value::String(s) => match s.to_str()?.trim().parse::<f64>() {
            Ok(n) => n,
            Err(_) => return Err(mlua::Error::runtime(usage.to_string())),
        },
        _ => return Err(mlua::Error::runtime(usage.to_string())),
    };
    Ok(usize::try_from(n.round() as i64 - 1)
        .ok()
        .filter(|&i| i < 8))
}

fn rune(lua: &Lua, i: usize) -> Option<RuneView> {
    let model = lua.app_data_ref::<Model>().expect("model");
    model.wrath_runes.0.and_then(|r| r.get(i).copied())
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "GetRuneType",
        lua.create_function(|lua, v: Value| {
            let Some(i) = slot(&v, "Usage: Script_GetRuneType(slot)")? else {
                return Ok(MultiValue::new());
            };
            Ok(match rune(lua, i) {
                Some(r) => MultiValue::from_vec(vec![Value::Integer(i64::from(r.rune_type) + 1)]),
                None => MultiValue::new(),
            })
        })?,
    )?;
    // start (seconds), duration, runeReady: start 0 and ready while the rune is up.
    g.set(
        "GetRuneCooldown",
        lua.create_function(|lua, v: Value| {
            let Some(i) = slot(&v, "Usage: GetRuneCooldown(slot)")? else {
                return Ok(MultiValue::new());
            };
            let r = rune(lua, i).unwrap_or_default();
            Ok(MultiValue::from_vec(vec![
                Value::Number(r.start.unwrap_or(0.0)),
                Value::Number(f64::from(r.duration)),
                Value::Boolean(r.start.is_none()),
            ]))
        })?,
    )?;
    // The ready mask bit (`[0xc24388]`): 1 or 0.
    g.set(
        "GetRuneCount",
        lua.create_function(|lua, v: Value| {
            let Some(i) = slot(&v, "Usage: GetRuneCount(slot)")? else {
                return Ok(MultiValue::new());
            };
            let ready = rune(lua, i).is_some_and(|r| r.start.is_none());
            Ok(MultiValue::from_vec(vec![Value::Integer(i64::from(ready))]))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::ScriptDialect;

    #[test]
    fn bindings_read_the_timed_snapshot() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.run("assert(select('#', GetRuneType(1)) == 0) local a, b, c = GetRuneCooldown(1) assert(a == 0 and b == 0 and c == true) assert(GetRuneCount(1) == 0)")
            .unwrap();
        let mut runes = [RuneView {
            rune_type: 0,
            start: None,
            duration: 10.0,
        }; 6];
        runes[2] = RuneView {
            rune_type: 3,
            start: Some(12.5),
            duration: 8.0,
        };
        s.set_wrath_runes(WrathRunes(Some(runes)));
        s.run(
            r#"
            assert(GetRuneType(1) == 1 and GetRuneType(3) == 4)
            local start, duration, ready = GetRuneCooldown(3)
            assert(start == 12.5 and duration == 8 and ready == false)
            start, duration, ready = GetRuneCooldown(1)
            assert(start == 0 and duration == 10 and ready == true)
            assert(GetRuneCount(1) == 1 and GetRuneCount(3) == 0)
            assert(select('#', GetRuneType(7)) == 0 and select('#', GetRuneType(9)) == 0)
            assert(not pcall(GetRuneCooldown) and not pcall(GetRuneType, {}))
            "#,
        )
        .unwrap();
        assert!(UiScript::new_vanilla()
            .unwrap()
            .eval::<bool>("return GetRuneCooldown == nil")
            .unwrap());
    }
}
