//! Build 12340's player-state verbs over the host's [`GameQuery`](super::GameQuery), each read in
//! the call: IsMounted, IsStealthed, IsSwimming, IsFlying, IsFlyableArea, GetUnitSpeed and
//! UnitIsPVPSanctuary. Every predicate answers the number 1 or nil (`0x84e2a0` with 1.0, else
//! `0x84e280`). They install with the engine, before FrameXML: `RestrictedEnvironment.lua` copies
//! the no-argument ones at file load.

use mlua::{Lua, Value};

use super::binding_abi::flag;
use super::query::ask;
use super::{Model, PlayerState};

/// The guid a token names through `0x60abf0` with no default to the target (its third argument
/// 0): a non-string, an unknown token or nobody is guid 0, which no unit holds.
fn token_guid(lua: &Lua, token: &Value) -> u64 {
    let Ok(Some(token)) = lua.coerce_string(token.clone()) else {
        return 0;
    };
    let model = lua.app_data_ref::<Model>().expect("model app_data");
    let token = token.to_string_lossy();
    model.unit_guid(&token).unwrap_or(0)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // The local player's object (`0x4d4db0`, type 0x10) or nil, then its field.
    for (name, pick) in [
        (
            "IsMounted",
            (|p: &PlayerState| p.mounted) as fn(&PlayerState) -> bool,
        ),
        ("IsStealthed", |p| p.stealthed),
        ("IsSwimming", |p| p.swimming),
        ("IsFlying", |p| p.flying),
    ] {
        g.set(
            name,
            lua.create_function(move |lua, ()| {
                Ok(flag(
                    ask(lua, |q| q.player_state()).is_some_and(|p| pick(&p)),
                ))
            })?,
        )?;
    }
    g.set(
        "IsFlyableArea",
        lua.create_function(|lua, ()| Ok(flag(ask(lua, |q| q.flyable_area()) == Some(true))))?,
    )?;
    // GetUnitSpeed(unit) (`0x613290`): a non-string raises (`0x6132a9`); one number, the unit's
    // cached speed, or 0 when the token names no held unit (`0x613313`). One value: the four
    // (current, run, flight, swim) are 4.2's.
    g.set(
        "GetUnitSpeed",
        lua.create_function(|lua, token: Value| {
            if !matches!(
                token,
                Value::String(_) | Value::Integer(_) | Value::Number(_)
            ) {
                return Err(mlua::Error::runtime("Usage: GetUnitSpeed(\"unit\")"));
            }
            let guid = token_guid(lua, &token);
            Ok(f64::from(ask(lua, |q| q.unit_speed(guid)).unwrap_or(0.0)))
        })?,
    )?;
    // UnitIsPVPSanctuary(unit) (`0x60cf20`): no usage check; 1 when the held unit carries the
    // sanctuary bit (`0x60cf6d`), else nil.
    g.set(
        "UnitIsPVPSanctuary",
        lua.create_function(|lua, token: Value| {
            let guid = token_guid(lua, &token);
            Ok(flag(ask(lua, |q| q.unit_sanctuary(guid)) == Some(true)))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{GameQuery, PlayerState, UiScript, UnitGuids};

    struct Fake;
    impl GameQuery for Fake {
        fn player_state(&self) -> Option<PlayerState> {
            Some(PlayerState {
                mounted: true,
                flying: true,
                ..Default::default()
            })
        }
        fn flyable_area(&self) -> Option<bool> {
            Some(true)
        }
        fn unit_speed(&self, unit: u64) -> Option<f32> {
            [(1, 7.0), (2, 3.5)]
                .into_iter()
                .find(|&(g, _)| g == unit)
                .map(|(_, v)| v)
        }
        fn unit_sanctuary(&self, unit: u64) -> Option<bool> {
            Some(unit == 2)
        }
    }

    /// Installed by the engine, so a VM with no provider has them and answers nil (0 for the
    /// speed); with one, the number 1 or nil, one speed value, and no target default.
    #[test]
    fn the_player_state_verbs_answer_one_or_nil_through_the_provider() {
        let mut s = UiScript::new().unwrap();
        let bare = "return IsMounted() == nil and IsStealthed() == nil and IsSwimming() == nil \
                    and IsFlying() == nil and IsFlyableArea() == nil \
                    and GetUnitSpeed('player') == 0 and UnitIsPVPSanctuary('player') == nil";
        assert!(s.eval::<bool>(bare).unwrap());
        s.set_game_query(Box::new(Fake));
        let mut guids = UnitGuids::default();
        (guids.player, guids.target) = (1, 2);
        s.set_unit_guids(&guids);
        assert!(s
            .eval::<bool>(
                "return IsMounted() == 1 and IsFlying() == 1 and IsSwimming() == nil \
                 and IsStealthed() == nil and IsFlyableArea() == 1 \
                 and select('#', GetUnitSpeed('player')) == 1 and GetUnitSpeed('player') == 7 \
                 and GetUnitSpeed('target') == 3.5 and GetUnitSpeed('') == 0 \
                 and UnitIsPVPSanctuary('target') == 1 and UnitIsPVPSanctuary('player') == nil \
                 and UnitIsPVPSanctuary() == nil"
            )
            .unwrap());
        assert!(s.run("GetUnitSpeed()").is_err());
    }
}
