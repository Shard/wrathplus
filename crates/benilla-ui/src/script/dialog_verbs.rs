//! The verbs the stock `StaticPopup.lua` dialogs call: the instance-boot and area spirit healer
//! clocks. 12340 has no meeting-stone queue and no pet untrainer, so neither verb set exists.

use mlua::Lua;

use super::Model;

/// The area spirit healer's aura (`0xA18`), the only cancel-aura spell that also fires
/// `AREA_SPIRIT_HEALER_OUT_OF_RANGE` (`0x6e70b6`).
pub const AREA_SPIRIT_HEALER_SPELL: u32 = 2584;

impl super::UiScript {
    /// `GetInstanceBootTimeRemaining()`'s answer, whole seconds (0 idle).
    pub fn set_instance_boot_secs(&mut self, secs: u32) {
        self.model_mut().instance_boot_secs = secs;
    }

    /// The area spirit healer: whether one is cached, and the seconds to its next wave.
    pub fn set_area_spirit_healer(&mut self, cached: bool, secs: u32) {
        let mut model = self.model_mut();
        model.area_spirit_healer_cached = cached;
        model.area_spirit_secs = secs;
    }

    /// `AcceptAreaSpiritHeal()` calls since the last drain, each a `0x2E3` to send.
    pub fn take_area_spirit_accepts(&mut self) -> u32 {
        std::mem::take(&mut self.model_mut().area_spirit_accepts)
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // `0x48b620`: `max(deadline - now, 0) / 1000`, truncated.
    g.set(
        "GetInstanceBootTimeRemaining",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(i64::from(model.instance_boot_secs))
        })?,
    )?;

    // `0x48df20`: silent with no healer cached, else `0x2E3` with the cached guid.
    g.set(
        "AcceptAreaSpiritHeal",
        lua.create_function(|lua, ()| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if model.area_spirit_healer_cached {
                model.area_spirit_accepts += 1;
            }
            Ok(())
        })?,
    )?;
    // `0x48df30` runs the generic cancel-aura (`0x6e7040`): the event, then `0x136` with
    // `u32 2584`, no guid. Its refusal gate (`AttributesEx` bit 13 set, bit 2 clear, then
    // `0x5ee290`) never trips for 2584 in the shipped `Spell.dbc` (the app's
    // `spell_2584_never_trips_the_cancel_gate`), so the send is unconditional.
    g.set(
        "CancelAreaSpiritHeal",
        lua.create_function(|lua, ()| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model
                .pending_events
                .push(("AREA_SPIRIT_HEALER_OUT_OF_RANGE".to_string(), Vec::new()));
            model.cancel_aura_requests.push(AREA_SPIRIT_HEALER_SPELL);
            Ok(())
        })?,
    )?;
    // Whole seconds: `max(0, [0xb4e338] - now) / 1000`.
    g.set(
        "GetAreaSpiritHealerTime",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(i64::from(model.area_spirit_secs))
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::UiScript;

    #[test]
    fn the_clocks_read_what_the_app_feeds() {
        let mut s = UiScript::new().unwrap();
        assert_eq!(
            s.eval::<i64>("return GetInstanceBootTimeRemaining()")
                .unwrap(),
            0
        );
        assert_eq!(
            s.eval::<i64>("return GetAreaSpiritHealerTime()").unwrap(),
            0
        );
        s.set_instance_boot_secs(42);
        s.set_area_spirit_healer(true, 17);
        assert_eq!(
            s.eval::<i64>("return GetInstanceBootTimeRemaining()")
                .unwrap(),
            42
        );
        assert_eq!(
            s.eval::<i64>("return GetAreaSpiritHealerTime()").unwrap(),
            17
        );
    }

    #[test]
    fn accept_area_spirit_heal_is_silent_without_a_cached_healer() {
        let mut s = UiScript::new().unwrap();
        s.run("AcceptAreaSpiritHeal()").unwrap();
        assert_eq!(
            s.take_area_spirit_accepts(),
            0,
            "no healer cached: nothing sent"
        );
        s.set_area_spirit_healer(true, 5);
        s.run("AcceptAreaSpiritHeal() AcceptAreaSpiritHeal()")
            .unwrap();
        assert_eq!(s.take_area_spirit_accepts(), 2);
    }

    #[test]
    fn cancel_area_spirit_heal_cancels_the_aura_and_fires_out_of_range() {
        let mut s = UiScript::new().unwrap();
        s.run(
            r#"F = CreateFrame("Frame") F:RegisterEvent("AREA_SPIRIT_HEALER_OUT_OF_RANGE")
               F:SetScript("OnEvent", function(this, event, arg1, arg2, arg3, arg4) GOT = event end) CancelAreaSpiritHeal()"#,
        )
        .unwrap();
        s.tick(0.0);
        assert_eq!(
            s.eval::<String>("return GOT").unwrap(),
            "AREA_SPIRIT_HEALER_OUT_OF_RANGE"
        );
        assert_eq!(
            s.take_cancel_aura_requests(),
            vec![super::AREA_SPIRIT_HEALER_SPELL]
        );
    }
}
