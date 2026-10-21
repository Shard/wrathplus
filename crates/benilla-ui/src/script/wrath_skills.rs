//! Wrath skill-point verbs behind `SkillFrame.xml`'s detail bar: `AddSkillUp` (`0x5ce460`),
//! `RemoveSkillUp` (`0x5ce4d0`) and `BuySkillTier` (`0x5ce6f0`, `CMSG_SKILL_BUY_STEP` `0x21F`).
//! Build 12340 has no skill points to spend: `GetSkillLineInfo` answers nil step and rank costs
//! and no temporary points, so the stock bar never shows these buttons. The bindings keep their
//! usage checks and change nothing. Unresolved, labelled: the temp-point allocators
//! `0x5cdbf0`/`0x5cdd20` and the purchase gate (`0x810460` cost against `0x5cd970` points).

use mlua::{Lua, Value};

fn verb(lua: &Lua, usage: &'static str) -> mlua::Result<mlua::Function> {
    lua.create_function(move |_, v: Value| match v {
        Value::Integer(_) | Value::Number(_) => Ok(()),
        Value::String(s) if s.to_str()?.trim().parse::<f64>().is_ok() => Ok(()),
        _ => Err(mlua::Error::runtime(usage)),
    })
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set("AddSkillUp", verb(lua, "Usage: AddSkillUp(index)")?)?;
    g.set("RemoveSkillUp", verb(lua, "Usage: RemoveSkillUp(index)")?)?;
    g.set("BuySkillTier", verb(lua, "Usage: BuySkillTier(index)")?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::UiScript;

    #[test]
    fn the_skill_point_verbs_check_usage_and_buy_nothing() {
        let mut s = UiScript::new().unwrap();
        s.run("AddSkillUp(1) RemoveSkillUp('2') BuySkillTier(3) assert(not pcall(AddSkillUp)) assert(not pcall(BuySkillTier, {}))")
            .unwrap();
        assert!(s.take_script_calls().is_empty());
    }
}
