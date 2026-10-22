//! `FollowUnit` and `FollowByName`; each call queues a [`FollowRequest`] the app drains.
//! `FollowUnit("target")` is a bare `/follow` and the `FOLLOWTARGET` binding. `FollowByName`
//! (`0x489ec0`) calls the by-name resolver `0x493aa0` for players (`0x10`) who `CanAssist` and are
//! alive, with its second argument as the exact-only flag: `UnitPopup.lua:623` passes 1, so the
//! menu never prefix-matches someone else, while `/follow rag` may. benilla parses `/follow` in
//! Rust, so these globals serve the stock UI and addons.

use mlua::Lua;

use super::Model;

/// A follow intent queued by the globals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FollowRequest {
    /// `FollowUnit(token)`: whoever the token names.
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

    // FollowUnit(unitOrName [, exactMatch]) (12340 `0x5224c0`): the resolver `0x520190` takes a
    // unit token or a player's name (typemask `0x10`, then `0x8`) with the exact flag, so the stock
    // popup's `FollowUnit(fullname, 1)` (UnitPopup.lua:1324-1325) follows by name.
    g.set(
        "FollowUnit",
        lua.create_function(|lua, (who, exact): (String, Option<mlua::Value>)| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let request = if model.unit_guids.guid_of(&who).is_ok() {
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
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model.follow_requests.push(FollowRequest::ByName {
                name,
                exact: truthy(exact),
            });
            Ok(())
        })?,
    )?;

    Ok(())
}

/// The exact flag as `GetBoolOrDefault` reads it, default false: nil, false and 0 are false and any
/// other value is true.
fn truthy(v: Option<mlua::Value>) -> bool {
    match v {
        None | Some(mlua::Value::Nil) | Some(mlua::Value::Boolean(false)) => false,
        Some(mlua::Value::Integer(n)) => n != 0,
        Some(mlua::Value::Number(n)) => n != 0.0,
        Some(_) => true,
    }
}
