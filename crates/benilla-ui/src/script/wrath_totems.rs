//! Build 12340's totem slots and the multi-cast bar's spell lists, pushed by the app from
//! `SMSG_TOTEM_CREATED` and the known spells: `GetTotemInfo`, `GetTotemTimeLeft`,
//! `GetMultiCastTotemSpells` (`0x5a8330`) and `GameTooltip:SetTotem` (`0x6205c0`). Slots are
//! 1-based in Lua: fire, earth, water, air (`SUMMON_SLOT_TOTEM_FIRE` 1 ..).

use mlua::{Lua, MultiValue, Table, Value};

use super::object::frame_handle_of;
use super::tooltip::{append_line, clear_content, fire_cleared, show_or_hide_empty};
use super::Model;

/// One live totem.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TotemView {
    /// `GetTotemInfo`'s name: the summoning spell's.
    pub name: String,
    pub icon: String,
    /// `GetTime` seconds at creation.
    pub start: f64,
    /// Seconds; 0 for a totem with no duration.
    pub duration: f64,
}

/// The four totem slots and the multi-cast bar's per-slot spell lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TotemState {
    pub slots: [Option<TotemView>; 4],
    /// The known spells that summon into each slot, in the client's order.
    pub multicast: [Vec<u32>; 4],
}

impl super::UiScript {
    /// Replace the totem state.
    pub fn set_totems(&mut self, state: TotemState) {
        self.model_mut().totems = state;
    }
}

/// A 1-based Lua slot, as `0x5a8330` indexes it: `(slot - 1) & 3`.
fn index(slot: i64) -> usize {
    (slot.wrapping_sub(1) & 3) as usize
}

/// A totem slot as `GetTotemInfo` reads it: only 1..=4.
fn live(model: &Model, slot: i64) -> Option<&TotemView> {
    (1..=4)
        .contains(&slot)
        .then(|| model.totems.slots[index(slot)].as_ref())
        .flatten()
}

fn time_left(view: &TotemView, now: f64) -> f64 {
    if view.duration <= 0.0 {
        return 0.0;
    }
    (view.start + view.duration - now).max(0.0)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // `GetTotemInfo(slot)`: haveTotem, name, startTime, duration, icon.
    g.set(
        "GetTotemInfo",
        lua.create_function(|lua, slot: i64| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(match live(&model, slot) {
                Some(t) => (true, t.name.clone(), t.start, t.duration, t.icon.clone()),
                None => (false, String::new(), 0.0, 0.0, String::new()),
            })
        })?,
    )?;
    g.set(
        "GetTotemTimeLeft",
        lua.create_function(|lua, slot: i64| {
            let now = super::clock::now(lua);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(live(&model, slot).map_or(0.0, |t| time_left(t, now)))
        })?,
    )?;
    // `GetMultiCastTotemSpells(slot)`: the slot's spells as separate returns. The multi-cast bar
    // also passes action ids past 4, which wrap (`0x5a8330`).
    g.set(
        "GetMultiCastTotemSpells",
        lua.create_function(|lua, slot: i64| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(MultiValue::from_vec(
                model.totems.multicast[index(slot)]
                    .iter()
                    .map(|&id| Value::Integer(i64::from(id)))
                    .collect(),
            ))
        })?,
    )?;
    Ok(())
}

/// `GameTooltip:SetTotem(slot)` (`0x6205c0`): for a live totem, its name, then
/// `SPELL_TIME_REMAINING`'s line; an empty slot leaves the tooltip as it was.
pub(super) fn install_tooltip_method(lua: &Lua, m: &Table) -> mlua::Result<()> {
    m.set(
        "SetTotem",
        lua.create_function(|lua, (this, slot): (Table, i64)| {
            let now = super::clock::now(lua);
            let (name, left_ms) = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let Some(t) = live(&model, slot) else {
                    return Ok(());
                };
                let left = time_left(t, now);
                (t.name.clone(), (left * 1000.0).round() as u32)
            };
            let h = frame_handle_of(lua, &this)?;
            {
                let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                clear_content(&mut model, h);
            }
            fire_cleared(lua, h);
            const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
            append_line(lua, &this, (name, WHITE), None, false)?;
            let g = lua.globals();
            if let Some(line) =
                super::tooltip::duration_text(left_ms, "SPELL_TIME_REMAINING", true, &|key| {
                    g.get::<String>(key).ok()
                })
            {
                append_line(lua, &this, (line, WHITE), None, false)?;
            }
            show_or_hide_empty(lua, h);
            Ok(())
        })?,
    )
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    use super::*;

    fn script() -> UiScript {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let mut state = TotemState::default();
        state.slots[1] = Some(TotemView {
            name: "Stoneskin Totem".into(),
            icon: "Interface\\Icons\\Spell_Nature_StoneSkinTotem".into(),
            start: 0.0,
            duration: 120.0,
        });
        state.multicast[1] = vec![8071, 8075];
        s.set_totems(state);
        s
    }

    #[test]
    fn totem_info_and_time_left_read_the_pushed_slot() {
        let s = script();
        let (have, name, _, duration): (bool, String, f64, f64) = s
            .eval("local a, b, c, d = GetTotemInfo(2) return a, b, c, d")
            .unwrap();
        assert!(have);
        assert_eq!((name.as_str(), duration), ("Stoneskin Totem", 120.0));
        let empty: bool = s.eval("return (GetTotemInfo(1))").unwrap();
        assert!(!empty);
        let out_of_range: bool = s.eval("return (GetTotemInfo(5))").unwrap();
        assert!(!out_of_range);
        let left: f64 = s.eval("return GetTotemTimeLeft(2)").unwrap();
        assert!(left > 0.0 && left <= 120.0, "{left}");
        assert_eq!(s.eval::<f64>("return GetTotemTimeLeft(3)").unwrap(), 0.0);
    }

    #[test]
    fn multicast_spells_wrap_the_slot_as_the_bar_passes_action_ids() {
        let s = script();
        let n: i64 = s
            .eval("return select('#', GetMultiCastTotemSpells(2))")
            .unwrap();
        assert_eq!(n, 2);
        // Action id 6 is page 2's earth slot: (6 - 1) & 3 = 1.
        let first: i64 = s.eval("return (GetMultiCastTotemSpells(6))").unwrap();
        assert_eq!(first, 8071);
        let none: i64 = s
            .eval("return select('#', GetMultiCastTotemSpells(1))")
            .unwrap();
        assert_eq!(none, 0);
    }
}
