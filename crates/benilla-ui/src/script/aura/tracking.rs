//! Supported non-spell Wrath tracking: trivial quests. The other native NPC tracking types
//! need their own classifiers before being advertised to FrameXML.
use crate::script::{model::Model, ScriptDialect};
use mlua::{Lua, MultiValue, Value};

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // Wrath-only API names must not extend the Vanilla global surface.
    if lua.app_data_ref::<Model>().expect("model app_data").dialect != ScriptDialect::Wrath12340 {
        return Ok(());
    }
    let g = lua.globals();
    g.set(
        "GetNumTrackingTypes",
        lua.create_function(|lua, ()| {
            let m = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(if m.dialect == ScriptDialect::Wrath12340 {
                1
            } else {
                0
            })
        })?,
    )?;
    g.set(
        "GetTrackingInfo",
        lua.create_function(|lua, index: i64| {
            let m = lua.app_data_ref::<Model>().expect("model app_data");
            if m.dialect != ScriptDialect::Wrath12340 || index != 1 {
                return Ok(MultiValue::new());
            }
            // Wow.exe 0x57eb00 enumerates the 20-byte table at 0xa11c50; final row
            // 0xa11d68 is {3,0,MINIMAP_TRACKING_TRIVIAL_QUESTS,"TrivialQuests",0}.
            let name = lua
                .globals()
                .get::<Option<String>>("MINIMAP_TRACKING_TRIVIAL_QUESTS")?
                .unwrap_or_else(|| "Low Level Quests".into());
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(name)?),
                Value::String(lua.create_string("Interface\\Minimap\\Tracking\\TrivialQuests")?),
                Value::Boolean(m.wrath_tracking_selection == Some(1)),
                Value::String(lua.create_string("other")?),
            ]))
        })?,
    )?;
    g.set(
        "SetTracking",
        lua.create_function(|lua, index: Option<usize>| {
            let mut m = lua.app_data_mut::<Model>().expect("model app_data");
            if m.dialect == ScriptDialect::Wrath12340 {
                m.wrath_tracking_selection = index.filter(|i| *i == 1);
                m.pending_events
                    .push(("MINIMAP_UPDATE_TRACKING".into(), Vec::new()));
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

impl crate::script::UiScript {
    /// Native 0x57bf30: selected non-spell tracking entry has type 3.
    pub fn track_trivial_quests(&self) -> bool {
        let m = self.model_ref();
        m.dialect == ScriptDialect::Wrath12340 && m.wrath_tracking_selection == Some(1)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn trivial_tracking_is_the_one_type_and_can_be_cleared() {
        let s = crate::script::UiScript::new_with_dialect(crate::script::ScriptDialect::Wrath12340)
            .unwrap();
        s.lua().load("assert(GetNumTrackingTypes()==1); SetTracking(1); local n,t,a,c=GetTrackingInfo(1); assert(n and t and a and c=='other'); assert(GetTrackingTexture()==t)").exec().unwrap();
        assert!(s.track_trivial_quests());
        {
            let m = s
                .lua()
                .app_data_ref::<crate::script::model::Model>()
                .unwrap();
            assert_eq!(
                m.pending_events.last().unwrap().0,
                "MINIMAP_UPDATE_TRACKING"
            );
        }
        s.lua().load("SetTracking(0); local n,t,a,c=GetTrackingInfo(1); assert(a==false); assert(GetTrackingTexture()==nil)").exec().unwrap();
        assert!(!s.track_trivial_quests());
        assert_eq!(
            s.lua()
                .app_data_ref::<crate::script::model::Model>()
                .unwrap()
                .pending_events
                .iter()
                .filter(|e| e.0 == "MINIMAP_UPDATE_TRACKING")
                .count(),
            2
        );
    }
}
