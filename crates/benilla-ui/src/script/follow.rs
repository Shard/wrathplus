//! `FollowUnit` and `FollowByName`; each call queues a [`FollowRequest`] the app drains.
//! `FollowUnit("target")` is a bare `/follow` and the `FOLLOWTARGET` binding. `FollowByName`
//! (`0x489ec0`) calls the by-name resolver `0x493aa0` for players (`0x10`) who `CanAssist` and are
//! alive, with its second argument as the exact-only flag. 3.3.5's popup calls
//! `FollowUnit(name, 1)` (`UnitPopup.lua:1325`), so the menu never prefix-matches someone else,
//! while `/follow rag` may. benilla parses `/follow` in Rust, so these globals serve the stock UI
//! and addons.

use mlua::Lua;

use super::Model;

/// A follow intent queued by the globals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FollowRequest {
    /// `FollowUnit(unit)`: whoever the token names, of any type; the reference applies no
    /// typemask here.
    ByUnit(String),
    /// `FollowByName(name, exactMatch)`: with `exact`, the resolver skips its longest-prefix tier
    /// and takes only a whole-name, case-insensitive match.
    ByName { name: String, exact: bool },
}

impl super::UiScript {
    /// Drain the follow intents queued since the last call.
    pub fn take_follow_requests(&mut self) -> Vec<FollowRequest> {
        std::mem::take(&mut self.model_mut().follow_requests)
    }
}

/// Register the follow globals.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // 3.3.5's `FollowUnit(unitOrName [, exact])`: stock `UnitPopup.lua:1325` passes the target's
    // name with exact 1. A recognised unit token follows the unit, anything else by name.
    // Labelled: inferred from the stock callers; 12340's binding is not traced.
    g.set(
        "FollowUnit",
        lua.create_function(|lua, (who, exact): (String, Option<mlua::Value>)| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let request = if super::unit::token_recognised(&who, model.unit_guids.wrath) {
                FollowRequest::ByUnit(who)
            } else {
                FollowRequest::ByName {
                    name: who,
                    exact: truthy(exact),
                }
            };
            model.follow_requests.push(request);
            Ok(())
        })?,
    )?;

    g.set(
        "FollowByName",
        lua.create_function(|lua, (name, exact): (String, Option<mlua::Value>)| {
            let exact = truthy(exact);
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model
                .follow_requests
                .push(FollowRequest::ByName { name, exact });
            Ok(())
        })?,
    )?;

    Ok(())
}

/// The exact flag as the reference reads it, `GetBoolOrDefault` (`0x6f1c10`), default false: nil,
/// false and 0 are false and any other value is true.
fn truthy(value: Option<mlua::Value>) -> bool {
    match value {
        None | Some(mlua::Value::Nil) | Some(mlua::Value::Boolean(false)) => false,
        Some(mlua::Value::Integer(n)) => n != 0,
        Some(mlua::Value::Number(n)) => n != 0.0,
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::FollowRequest;
    use crate::script::UiScript;

    /// A unit token follows the unit; stock `UnitPopup.lua`'s `FollowUnit(name, 1)` follows the
    /// name exactly.
    #[test]
    fn follow_unit_takes_a_token_or_a_name() {
        let mut s = UiScript::new().unwrap();
        s.run(r#"FollowUnit("target"); FollowUnit("Ally", 1); FollowUnit("Bob")"#)
            .unwrap();
        assert_eq!(
            s.take_follow_requests(),
            [
                FollowRequest::ByUnit("target".into()),
                FollowRequest::ByName {
                    name: "Ally".into(),
                    exact: true,
                },
                FollowRequest::ByName {
                    name: "Bob".into(),
                    exact: false,
                },
            ]
        );
    }
}
