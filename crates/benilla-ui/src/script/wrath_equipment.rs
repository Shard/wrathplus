//! Build-12340 equipment-set queries before a saved-set feed exists.
//! PaperDollFrame.lua:1118,2218,2227,2524 and EquipmentManager.lua:297 consume these.
//! Missing sets return no values, not fabricated records, tables or mutation success.

use super::{Model, binding_abi};
use mlua::{Lua, MultiValue, Value};

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(
        "CanUseEquipmentSets",
        lua.create_function(|lua, ()| {
            // 0x5AD880 pushes one NUMBER from equipmentManager's integer value.
            // Native also gates on the equipment-use busy latch C1E956 (set by
            // 0x5ADDD8, cleared by result handler 0x5AF71C). This read-only
            // empty-set implementation never starts an equipment-set operation.
            let model = lua.app_data_ref::<Model>().expect("model");
            let enabled = model
                .cvars
                .get("equipmentmanager")
                .is_some_and(|slot| super::cvars::sstr_to_int(&slot.value) != 0);
            Ok(i32::from(enabled))
        })?,
    )?;
    globals.set("GetNumEquipmentSets", lua.create_function(|_, ()| Ok(0))?)?;
    globals.set(
        "GetEquipmentSetInfo",
        lua.create_function(|lua, index: Value| {
            // 0x5AF120 uses an unguarded tonumber, truncates, then checks 1..10.
            let index = binding_abi::coerced_number(lua, Some(index)) as i64 as i32;
            if !(1..=10).contains(&index) {
                return Err(mlua::Error::RuntimeError(
                    "Usage: GetEquipmentSetInfo(index)".into(),
                ));
            }
            Ok(MultiValue::new())
        })?,
    )?;
    for (name, usage) in [
        (
            "GetEquipmentSetInfoByName",
            "Usage: GetEquipmentSetInfoByName(name)",
        ),
        (
            "GetEquipmentSetItemIDs",
            "Usage: GetEquipmentSetItemIDs(name)",
        ),
        (
            "EquipmentSetContainsLockedItems",
            "Usage: EquipmentSetContainsLockedItems(name)",
        ),
    ] {
        globals.set(
            name,
            lua.create_function(move |lua, name: Value| {
                binding_abi::string_arg(lua, name, usage)?;
                // 0x5AF1D0 / 0x5AEF90 / 0x5AF320 missing-name branches push
                // nothing. In particular ItemIDs must not touch a caller's table.
                Ok(MultiValue::new())
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    fn script() -> UiScript {
        UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap()
    }

    #[test]
    fn missing_sets_have_native_zero_result_arity() {
        script()
            .run(
                r#"
            assert(GetNumEquipmentSets()==0)
            assert(select('#',GetNumEquipmentSets())==1)
            for i=1,10 do assert(select('#',GetEquipmentSetInfo(i))==0) end
            for _,name in ipairs({'', 'missing', 123}) do
                assert(select('#',GetEquipmentSetInfoByName(name))==0)
                assert(select('#',GetEquipmentSetItemIDs(name))==0)
                assert(select('#',EquipmentSetContainsLockedItems(name))==0)
            end
            local work={17, keep='untouched'}
            assert(select('#',GetEquipmentSetItemIDs('missing',work))==0)
            assert(work[1]==17 and work.keep=='untouched')
            assert(select('#',GetEquipmentSetInfo('1.9'))==0)
        "#,
            )
            .unwrap();
    }

    #[test]
    fn availability_is_numeric_cvar_not_set_count() {
        script()
            .run(
                r#"
            assert(CanUseEquipmentSets()==0 and select('#',CanUseEquipmentSets())==1)
            assert(type(CanUseEquipmentSets())=='number')
            SetCVar('equipmentManager','1')
            assert(CanUseEquipmentSets()==1 and GetNumEquipmentSets()==0)
            SetCVar('equipmentManager','-1')
            assert(CanUseEquipmentSets()==1)
            SetCVar('equipmentManager','0')
            assert(CanUseEquipmentSets()==0)
        "#,
            )
            .unwrap();
    }

    #[test]
    fn invalid_arguments_raise_and_do_not_create_sets() {
        script().run(r#"
            for _,arg in ipairs({0,-1,11,false,{}}) do
                assert(not pcall(GetEquipmentSetInfo,arg))
            end
            assert(not pcall(GetEquipmentSetInfo))
            for _,query in ipairs({GetEquipmentSetInfoByName,GetEquipmentSetItemIDs,EquipmentSetContainsLockedItems}) do
                assert(not pcall(query))
                assert(not pcall(query,false))
                assert(not pcall(query,{}))
            end
            assert(GetNumEquipmentSets()==0)
        "#).unwrap();
    }

    #[test]
    fn vanilla_surface_is_unchanged() {
        UiScript::new()
            .unwrap()
            .run(
                r#"
            assert(CanUseEquipmentSets==nil and GetNumEquipmentSets==nil)
            assert(GetEquipmentSetInfo==nil and GetEquipmentSetInfoByName==nil)
            assert(GetEquipmentSetItemIDs==nil and EquipmentSetContainsLockedItems==nil)
        "#,
            )
            .unwrap();
    }
}
