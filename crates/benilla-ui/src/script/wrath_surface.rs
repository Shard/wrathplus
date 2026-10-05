//! The deliberately bounded Wrath 3.3.5a global API surface.
//!
//! Construction installs the mature Vanilla implementation first, then this module removes every
//! engine function not explicitly admitted below and replaces the build/unit bindings whose Wrath
//! ABI differs. Lua 5.1's own globals and libraries remain available. Addons may subsequently
//! publish their own globals normally; pruning happens only once, before any UI or addon loads.

use mlua::{Function, Lua, Table, Value};

#[path = "wrath_loot.rs"]
mod wrath_loot;

#[path = "wrath_power.rs"]
mod wrath_power;

use super::Model;

/// Engine functions admitted to the Wrath addon VM. Widget/event operations are methods on the
/// frame objects, so their much larger surface does not need to appear here.
const ALLOWED_ENGINE_FUNCTIONS: &[&str] = &[
    // Widget construction and safe screen/input observations.
    "CreateFrame",
    "CreateFont",
    "EnumerateFrames",
    "GetScreenWidth",
    "GetScreenHeight",
    "SetupFullscreenScale",
    "GetCursorPosition",
    "GetMouseFocus",
    "IsMacClient",
    "IsShiftKeyDown",
    "IsControlKeyDown",
    "IsAltKeyDown",
    // Addon registry, demand loading, saved state and reload.
    "GetNumAddOns",
    "GetAddOnInfo",
    "GetAddOnMetadata",
    "GetAddOnDependencies",
    "IsAddOnLoaded",
    "IsAddOnLoadOnDemand",
    "EnableAddOn",
    "DisableAddOn",
    "EnableAllAddOns",
    "DisableAllAddOns",
    "ResetDisabledAddOns",
    "LoadAddOn",
    "RegisterForSave",
    "ReloadUI",
    // The bounded stock GameMenu slice uses the real session and sound queues.
    // Manual solo corpse loot; the profile adapter supplies the build-12340 ABI.
    "CloseLoot",
    "GetItemQualityColor",
    "GetLootSlotInfo",
    "GetLootSlotLink",
    "GetNumLootItems",
    "IsAutoLootKeyDown",
    "IsFishingLoot",
    "LootSlot",
    "LootSlotIsCoin",
    "LootSlotIsItem",
    "Logout",
    "Quit",
    "PlaySound",
    // Panel presentation is ordinary widget visibility; no protected execution is involved.
    "ShowUIPanel",
    "HideUIPanel",
    // Truthful client/session observations.
    "GetBuildInfo",
    "GetLocale",
    "GetRealmName",
    "GetTime",
    "GetGameTime",
    "GetFramerate",
    // Bounded read-only access to host settings admitted centrally by the CVar store.
    "GetCVar",
    // The observed-unit slice fed by the engine.
    "UnitName",
    "UnitExists",
    "UnitIsUnit",
    "UnitHealth",
    "UnitHealthMax",
    "UnitPower",
    "UnitPowerMax",
    "UnitPowerType",
    "UnitLevel",
    "UnitGUID",
    // Safe scripting/error helpers supplied by the host rather than Lua itself.
    "debugstack",
    "geterrorhandler",
    "seterrorhandler",
    "getglobal",
    "setglobal",
    "time",
    "date",
    "difftime",
    "strlen",
    "strsub",
    "strupper",
    "strlower",
    "strrep",
    "strfind",
    "strbyte",
    "strchar",
    "gsub",
    "tinsert",
    "tremove",
    // Installed build-12340 LootFrame.lua:145 uses the bare alias supplied by stdlib.
    "ceil",
];

/// Functions present in an untouched Lua 5.1 state. These are language facilities, not game API.
const LUA51_GLOBAL_FUNCTIONS: &[&str] = &[
    "assert",
    "collectgarbage",
    "error",
    "gcinfo",
    "getfenv",
    "getmetatable",
    "ipairs",
    "loadstring",
    "next",
    "pairs",
    "pcall",
    "print",
    "rawequal",
    "rawget",
    "rawset",
    "select",
    "setfenv",
    "setmetatable",
    "tonumber",
    "tostring",
    "type",
    "unpack",
    "xpcall",
];

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    install_truthful_readers(lua)?;
    install_panel_visibility(lua)?;
    wrath_loot::install(lua)?;
    wrath_power::install(lua)?;
    reject_secure_templates(lua)?;
    prune_engine_functions(lua)
}

/// Wrath's GameMenu buttons use the panel helpers only to show or hide ordinary frames. The full
/// UIParent panel-position delegate is outside this bounded stock slice; these bindings preserve
/// the helpers' concrete visibility effect without claiming options/social panel support.
fn install_panel_visibility(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for (name, method) in [("ShowUIPanel", "Show"), ("HideUIPanel", "Hide")] {
        globals.set(
            name,
            lua.create_function(move |_, frame: Table| {
                let call: Function = frame.get(method)?;
                call.call::<()>(frame)
            })?,
        )?;
    }
    Ok(())
}

fn install_truthful_readers(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();

    globals.set(
        "GetBuildInfo",
        lua.create_function(|lua, ()| {
            Ok((
                lua.create_string("3.3.5")?,
                lua.create_string("12340")?,
                lua.create_string("Jun 24 2010")?,
                30300u32,
            ))
        })?,
    )?;

    // Wrath predicates are booleans. A token is observable only when the engine supplied an
    // existing unit snapshot; no placeholder or guessed identity is manufactured.
    globals.set(
        "UnitExists",
        lua.create_function(|lua, token: Option<String>| {
            let exists = token.as_deref().is_some_and(|token| {
                lua.app_data_ref::<Model>()
                    .expect("model app_data")
                    .unit(token)
                    .is_some_and(|unit| unit.exists)
            });
            Ok(exists)
        })?,
    )?;
    globals.set(
        "UnitGUID",
        lua.create_function(|lua, token: Option<String>| {
            let guid = token.as_deref().and_then(|token| {
                lua.app_data_ref::<Model>()
                    .expect("model app_data")
                    .unit(token)
                    .filter(|unit| unit.exists && unit.guid != 0)
                    .map(|unit| unit.guid)
            });
            match guid {
                Some(guid) => Ok(Value::String(lua.create_string(format!("0x{guid:016X}"))?)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    Ok(())
}

/// Secure templates require taint tracking, protected attributes and restricted execution, none
/// of which this host implements. Failing at construction is safer than creating an ordinary
/// frame that merely looks protected.
fn reject_secure_templates(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let create: Function = globals.get("CreateFrame")?;
    globals.set(
        "CreateFrame",
        lua.create_function(
            move |lua, (kind, name, parent, inherits): (String, Option<Value>, Option<Value>, Option<Value>)| {
                if let Some(template) = inherits.as_ref()
                    .map(|value| lua.coerce_string(value.clone()))
                    .transpose()?
                    .flatten()
                {
                    let template = template.to_string_lossy();
                    if crate::loader::is_secure_template(lua, &template) {
                        return Err(mlua::Error::runtime(format!(
                            "CreateFrame: secure template '{template}' is unsupported: Wrath protected execution is not implemented"
                        )));
                    }
                }
                create.call::<Table>((kind, name, parent, inherits))
            },
        )?,
    )
}

fn prune_engine_functions(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let mut remove = Vec::new();
    for pair in globals.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let (Value::String(key), Value::Function(_)) = (key, value) else {
            continue;
        };
        let name = key.to_string_lossy();
        if !LUA51_GLOBAL_FUNCTIONS.contains(&name.as_str())
            && !ALLOWED_ENGINE_FUNCTIONS.contains(&name.as_str())
            && !wrath_loot::ENGINE_FUNCTIONS.contains(&name.as_str())
        {
            remove.push(key);
        }
    }
    for key in remove {
        globals.raw_set(key, Value::Nil)?;
    }
    Ok(())
}

/// Public documentation/introspection for the bounded engine surface.
pub fn allowed_engine_functions() -> &'static [&'static str] {
    ALLOWED_ENGINE_FUNCTIONS
}

#[cfg(test)]
mod tests {
    use super::super::{ScriptDialect, SessionRequest, UiScript};

    #[test]
    fn wrath_loot_math_alias_survives_pruning_without_opening_engine_actions() {
        let script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        assert!(script
            .eval::<bool>("return ceil == math.ceil and ceil(7/6)==2 and ceil(-1.5)==-1")
            .unwrap());
        assert!(script
            .eval::<bool>("return UseContainerItem==nil and CastSpellByName==nil")
            .unwrap());
    }

    #[test]
    fn wrath_panel_helpers_have_real_visibility_and_session_effects() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script
            .run(
                "local f=CreateFrame('Frame','MenuProbe'); f:Hide(); ShowUIPanel(f); \
                 assert(f:IsShown()); HideUIPanel(f); assert(not f:IsShown()); Logout(); Quit()",
            )
            .unwrap();
        assert_eq!(
            script.take_session_requests(),
            [SessionRequest::Logout, SessionRequest::Quit]
        );
    }

    #[test]
    fn bounded_menu_work_does_not_weaken_secure_template_rejection() {
        let script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        benilla_ui_test_secure_template(&script);
        let error = script
            .run("CreateFrame('Button', nil, nil, 'SecureActionButtonTemplate')")
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("protected execution is not implemented"),
            "{error}"
        );
    }

    fn benilla_ui_test_secure_template(script: &UiScript) {
        let doc = crate::framexml::parse(
            r#"<Ui><Button name="SecureActionButtonTemplate" virtual="true"/></Ui>"#,
        )
        .unwrap();
        let report = crate::loader::load(script, &doc, &|_| None);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
    }
}
