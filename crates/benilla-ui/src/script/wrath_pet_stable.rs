//! Build 12340's pet stable pair for `PetStable.lua`: `GetPetTalentTree` (`0x5d3d80`), the
//! active pet's talent tree name (the first `TalentTab` whose pet mask has its family's pet talent
//! type), and `IsAtStableMaster` (`0x5a1090`), whether a stable master's window is open. The app
//! pushes both.

use mlua::{Lua, MultiValue, Value};

use super::{Model, UiScript};

impl UiScript {
    /// The active pet's talent tree name, `None` without a pet that has one, and whether a stable
    /// master is open.
    pub fn set_wrath_pet_stable(&mut self, talent_tree: Option<String>, at_stable_master: bool) {
        let mut model = self.model_mut();
        model.pet_talent_tree = talent_tree;
        model.at_stable_master = at_stable_master;
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // The tree's name, or nil.
    g.set(
        "GetPetTalentTree",
        lua.create_function(|lua, _: MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model");
            Ok(match &model.pet_talent_tree {
                Some(name) => Value::String(lua.create_string(name)?),
                None => Value::Nil,
            })
        })?,
    )?;
    // A boolean, never nil.
    g.set(
        "IsAtStableMaster",
        lua.create_function(|lua, _: MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model");
            Ok(model.at_stable_master)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    #[test]
    fn the_pair_reads_the_pushed_state() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        assert!(s
            .eval::<bool>("return GetPetTalentTree() == nil and IsAtStableMaster() == false")
            .unwrap());
        s.set_wrath_pet_stable(Some("Ferocity".into()), true);
        assert!(s
            .eval::<bool>("return GetPetTalentTree() == 'Ferocity' and IsAtStableMaster() == true")
            .unwrap());
    }
}
