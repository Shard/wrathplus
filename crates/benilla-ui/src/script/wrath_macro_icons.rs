//! Build 12340's macro icon chooser: `GetNumMacroIcons` (`0x566490`) and `GetMacroIconInfo`
//! (`0x564e90`) over the spell-icon list `0x565840` builds (`/tmp/azb-app-mig/ghidra/
//! macroicons*.log`); the item-icon pair is `wrath_guild_bank`'s. The host pushes the list's names
//! ([`UiScript::set_macro_spell_icons`]); the binding splices `Interface\Icons\` back on, and an
//! index out of range or an empty name answers "".

use mlua::{Lua, MultiValue, Value};

#[derive(Default)]
struct MacroSpellIcons(Vec<String>);

impl super::UiScript {
    /// Publish the spell-icon list `GetNumMacroIcons`/`GetMacroIconInfo` read, in order.
    pub fn set_macro_spell_icons(&mut self, icons: Vec<String>) {
        self.lua.set_app_data(MacroSpellIcons(icons));
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.set_app_data(MacroSpellIcons::default());
    lua.globals().set(
        "GetNumMacroIcons",
        lua.create_function(|lua, ()| {
            Ok(lua
                .app_data_ref::<MacroSpellIcons>()
                .map_or(0, |i| i.0.len()) as i64)
        })?,
    )?;
    lua.globals().set(
        "GetMacroIconInfo",
        lua.create_function(|lua, args: MultiValue| {
            let index = args.into_iter().next().unwrap_or(Value::Nil);
            let n = lua
                .coerce_number(index)?
                .ok_or_else(|| mlua::Error::runtime("Usage: GetMacroIconInfo(index)"))?;
            let path = usize::try_from(n.round_ties_even() as i64 - 1)
                .ok()
                .and_then(|i| {
                    let icons = lua.app_data_ref::<MacroSpellIcons>()?;
                    icons.0.get(i).filter(|n| !n.is_empty()).cloned()
                })
                .map_or_else(String::new, |n| format!("Interface\\Icons\\{n}"));
            lua.create_string(&path)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    #[test]
    fn the_chooser_answers_the_pushed_names_as_icon_paths_and_empty_on_a_miss() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        assert_eq!(s.eval::<i64>("return GetNumMacroIcons()").unwrap(), 0);
        s.set_macro_spell_icons(vec![
            "INV_Misc_QuestionMark".into(),
            "Spell_Holy_Heal".into(),
        ]);
        for check in [
            "GetNumMacroIcons() == 2",
            "GetMacroIconInfo(1) == 'Interface\\\\Icons\\\\INV_Misc_QuestionMark'",
            "GetMacroIconInfo(2.4) == 'Interface\\\\Icons\\\\Spell_Holy_Heal'",
            "GetMacroIconInfo(3) == '' and GetMacroIconInfo(0) == ''",
        ] {
            assert!(
                s.eval::<bool>(&format!("return {check}")).unwrap(),
                "{check}"
            );
        }
        assert!(s.run("GetMacroIconInfo('x')").is_err());
    }
}
