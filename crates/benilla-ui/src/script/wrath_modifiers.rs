//! Build-12340 aggregate modifier query (WorldMapFrameTemplates.xml:153).

use super::{binding_abi, Model};
use mlua::Lua;

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "IsModifierKeyDown",
        lua.create_function(|lua, ()| {
            // 0x514430 ignores arguments, checking all six left/right modifier
            // keys. Like the individual queries it pushes one number 1 or nil.
            let model = lua.app_data_ref::<Model>().expect("model");
            let (shift, ctrl, alt) = model.modifiers;
            Ok(binding_abi::flag(shift || ctrl || alt))
        })?,
    )
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    #[test]
    fn aggregate_uses_existing_modifier_snapshot_and_native_arity() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        for bits in 0..8 {
            script.set_modifiers(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            script
                .run(if bits == 0 {
                    "assert(IsModifierKeyDown()==nil); assert(select('#',IsModifierKeyDown())==1)"
                } else {
                    "assert(IsModifierKeyDown()==1); assert(type(IsModifierKeyDown())=='number'); assert(select('#',IsModifierKeyDown())==1)"
                })
                .unwrap();
        }
        script.set_modifiers(false, false, false);
        script.run("assert(IsModifierKeyDown()==nil)").unwrap();
    }

    #[test]
    fn arguments_are_ignored_not_parsed_as_modifier_filters() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        for held in [false, true] {
            script.set_modifiers(held, false, false);
            script
                .run(
                    r#"
                    local expected=IsModifierKeyDown()
                    for _,arg in ipairs({'CTRL','ALT','SHIFT','invalid',0,false,{}}) do
                        assert(IsModifierKeyDown(arg)==expected)
                        assert(select('#',IsModifierKeyDown(arg))==1)
                    end
                    assert(IsModifierKeyDown(nil,true,'CTRL')==expected)
                    "#,
                )
                .unwrap();
        }
    }

}
