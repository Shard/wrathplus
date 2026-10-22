//! Build-12340 barber shop (Blizzard_BarbershopUI): the style getters, the preview cycling and
//! the apply/cancel/reset verbs over the host's [`WrathBarberShop`] (two Wow.exe passes:
//! `/tmp/azb-ui-barber-native.log`, `/tmp/azb-ui-barber-inner.log`).
//!
//! The preview selection is local, as native keeps it on the player's barber block (`+0xb4c`)
//! and the selected style rows (`0xbd19ac` hair, `0xbd19b0` facial hair, `0xbd19b4` skin); the
//! host renders the preview model from [`super::UiScript::wrath_barber_preview`]. Apply and Cancel
//! go out as [`WrathBarberRequest`]s.

use mlua::{Lua, MultiValue, Value};

use super::Model;

/// A BarberShopStyle.dbc row for the player's race and sex.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathBarberStyle {
    pub id: u32,
    pub name: String,
    pub description: String,
    /// The cost multiplier (`+0x10`).
    pub cost_modifier: f32,
    /// The appearance value it selects (`+0x1c`): hair style, facial hair or skin.
    pub data: u32,
}

/// The barber chair as the host publishes it while the player sits in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathBarberShop {
    /// gtBarberShopCostBase for the player's level (`0x7f6990`).
    pub base_cost: f32,
    pub hair_styles: Vec<WrathBarberStyle>,
    pub facial_styles: Vec<WrathBarberStyle>,
    /// Empty for a race whose skin the barber cannot change (`CanAlterSkin`).
    pub skin_styles: Vec<WrathBarberStyle>,
    /// How many hair colors the race and sex have.
    pub hair_colors: u32,
    /// The player's current appearance (`PLAYER_BYTES`): hair style, hair color, facial hair,
    /// skin.
    pub hair_style: u32,
    pub hair_color: u32,
    pub facial_hair: u32,
    pub skin: u32,
    /// ChrRaces' hair and facial-hair customization tokens ("HAIR", "HORNS", …).
    pub hair_customization: String,
    pub facial_customization: String,
}

/// One barber verb for App, in call order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WrathBarberRequest {
    /// `ApplyBarberShopStyle()` (`0x52eef0`): CMSG_ALTER_APPEARANCE's BarberShopStyle ids and the
    /// hair color; skin 0 when unchanged.
    Apply {
        hair: u32,
        hair_color: u32,
        facial_hair: u32,
        skin_color: u32,
    },
    /// `CancelBarberShop()` (`0x52ed80`): leave the chair.
    Cancel,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    shop: Option<WrathBarberShop>,
    /// The preview: hair style, hair color, facial hair and skin values.
    preview: (u32, u32, u32, u32),
    requests: Vec<WrathBarberRequest>,
}

impl super::UiScript {
    /// Open the barber shop with the chair's data, or close it with `None`. Opening starts the
    /// preview at the current appearance.
    pub fn set_wrath_barber_shop(&mut self, shop: Option<WrathBarberShop>) {
        let mut model = self.model_mut();
        let state = &mut model.wrath_barber;
        if let Some(s) = &shop {
            state.preview = (s.hair_style, s.hair_color, s.facial_hair, s.skin);
        }
        state.shop = shop;
    }

    /// The preview to render: hair style, hair color, facial hair, skin; `None` when closed.
    pub fn wrath_barber_preview(&self) -> Option<(u32, u32, u32, u32)> {
        let model = self.model_ref();
        model.wrath_barber.shop.as_ref().map(|_| model.wrath_barber.preview)
    }

    /// Drain the barber verbs in call order.
    pub fn take_wrath_barber_requests(&mut self) -> Vec<WrathBarberRequest> {
        std::mem::take(&mut self.model_mut().wrath_barber.requests)
    }
}

fn row(list: &[WrathBarberStyle], data: u32) -> Option<&WrathBarberStyle> {
    list.iter().find(|r| r.data == data)
}

/// The category's selected row (`0x52e490`): 0 hair style, 2 facial hair, 3 skin.
fn selected(state: &State, shop: &WrathBarberShop, cat: u32) -> Option<WrathBarberStyle> {
    let (hair, _, facial, skin) = state.preview;
    match cat {
        0 => row(&shop.hair_styles, hair),
        2 => row(&shop.facial_styles, facial),
        3 => row(&shop.skin_styles, skin),
        _ => None,
    }
    .cloned()
}

/// `0x52e790`: the category's preview is the current appearance.
fn is_current(state: &State, shop: &WrathBarberShop, cat: u32) -> bool {
    let (hair, color, facial, skin) = state.preview;
    match cat {
        0 => shop.hair_style == hair,
        1 => shop.hair_color == color,
        2 => shop.facial_hair == facial,
        3 => shop.skin == skin,
        _ => false,
    }
}

/// Native rounds through the x87's nearest-even mode.
fn round(x: f32) -> i64 {
    x.round_ties_even() as i64
}

/// `0x52eba0`: the hair style's cost, else the hair color's (half the base), plus facial hair and
/// skin, for each changed category, each from the rounded base.
fn total_cost(state: &State, shop: &WrathBarberShop) -> i64 {
    let base = round(shop.base_cost) as f32;
    let mut cost = 0;
    if !is_current(state, shop, 0) {
        cost = round(base * selected(state, shop, 0).map_or(0.0, |r| r.cost_modifier));
    } else if !is_current(state, shop, 1) {
        cost = round(base * HAIR_COLOR_MODIFIER);
    }
    if !is_current(state, shop, 2) {
        cost += round(base * selected(state, shop, 2).map_or(0.0, |r| r.cost_modifier));
    }
    if !is_current(state, shop, 3) {
        cost += round(base * selected(state, shop, 3).map_or(0.0, |r| r.cost_modifier));
    }
    cost
}

/// The hair color's cost multiplier (`0xa0814c`, 0.5).
const HAIR_COLOR_MODIFIER: f32 = 0.5;

/// The next (or previous) row after `data` in `list`, wrapping.
fn cycle(list: &[WrathBarberStyle], data: u32, backward: bool) -> u32 {
    if list.is_empty() {
        return data;
    }
    let at = list.iter().position(|r| r.data == data).unwrap_or(0);
    let n = list.len();
    let next = if backward { (at + n - 1) % n } else { (at + 1) % n };
    list[next].data
}

fn set(
    lua: &Lua,
    name: &str,
    f: impl Fn(&Lua, MultiValue) -> mlua::Result<MultiValue> + Send + 'static,
) -> mlua::Result<()> {
    lua.globals()
        .set(name, lua.create_function(move |lua, args| f(lua, args))?)
}

fn arg(args: &MultiValue, i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Nil)
}

fn text(lua: &Lua, s: &str) -> mlua::Result<Value> {
    Ok(Value::String(lua.create_string(s)?))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // GetBarberShopStyleInfo(catId) (`0x52e850`) → name, description, cost, isCurrent while the
    // chair is open; the hair color (2) has no name or description and costs half the base; the
    // skin (4) answers only when the race can change it.
    set(lua, "GetBarberShopStyleInfo", |lua, args| {
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        let state = &m.wrath_barber;
        let Some(shop) = state.shop.as_ref() else {
            return Ok(MultiValue::new());
        };
        let Some(n) = lua.coerce_number(arg(&args, 0))? else {
            return Err(mlua::Error::runtime("Usage: GetBarberShopStyleInfo(catId)"));
        };
        let cat = (n.round_ties_even() as i64 - 1) as u32;
        if cat > 3 || (cat == 3 && shop.skin_styles.is_empty()) {
            return Ok(MultiValue::new());
        }
        let (name, description, cost) = if cat == 1 {
            (Value::Nil, Value::Nil, round(shop.base_cost * HAIR_COLOR_MODIFIER))
        } else {
            let r = selected(state, shop, cat).unwrap_or_default();
            (text(lua, &r.name)?, text(lua, &r.description)?, round(shop.base_cost * r.cost_modifier))
        };
        Ok(MultiValue::from_vec(vec![
            name,
            description,
            Value::Integer(cost),
            super::binding_abi::flag(is_current(state, shop, cat)),
        ]))
    })?;
    // SetNextBarberShopStyle(catId [, backward]) (`0x52fdd0` → `0x52fc80`): the category's
    // preview steps to the next row (or color), wrapping. Labelled: native steps the model's
    // geoset and then finds the matching row (`0x52f760`); this steps the rows directly.
    set(lua, "SetNextBarberShopStyle", |lua, args| {
        let Some(n) = lua.coerce_number(arg(&args, 0))? else {
            return Err(mlua::Error::runtime(
                "Usage: SetNextBarberShopStyle(type[, backward])",
            ));
        };
        let backward = super::binding_abi::bool_or_default(args.get(1), false);
        let mut m = lua.app_data_mut::<Model>().expect("model app_data");
        let state = &mut m.wrath_barber;
        let Some(shop) = state.shop.clone() else {
            return Ok(MultiValue::new());
        };
        let cat = (n.round_ties_even() as i64 - 1) as u32;
        let p = &mut state.preview;
        match cat {
            0 => p.0 = cycle(&shop.hair_styles, p.0, backward),
            1 if shop.hair_colors > 0 => {
                let n = shop.hair_colors;
                p.1 = if backward { (p.1 + n - 1) % n } else { (p.1 + 1) % n };
            }
            2 => p.2 = cycle(&shop.facial_styles, p.2, backward),
            3 if !shop.skin_styles.is_empty() => p.3 = cycle(&shop.skin_styles, p.3, backward),
            _ => {}
        }
        Ok(MultiValue::new())
    })?;
    // GetBarberShopTotalCost() (`0x52ed60`) → the changed categories' cost, 0 when closed.
    set(lua, "GetBarberShopTotalCost", |lua, _| {
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        let state = &m.wrath_barber;
        let cost = state.shop.as_ref().map_or(0, |shop| total_cost(state, shop));
        Ok(MultiValue::from_vec(vec![Value::Integer(cost)]))
    })?;
    // CanAlterSkin() (`0x52e4f0`): the race has skin styles.
    set(lua, "CanAlterSkin", |lua, _| {
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        let can = m.wrath_barber.shop.as_ref().is_some_and(|s| !s.skin_styles.is_empty());
        Ok(MultiValue::from_vec(vec![Value::Boolean(can)]))
    })?;
    // GetHairCustomization() / GetFacialHairCustomization() (`0x52e6d0`/`0x52e730`): ChrRaces'
    // tokens, "NORMAL" without them.
    for (global, facial) in [("GetHairCustomization", false), ("GetFacialHairCustomization", true)] {
        set(lua, global, move |lua, _| {
            let m = lua.app_data_ref::<Model>().expect("model app_data");
            let token = m
                .wrath_barber
                .shop
                .as_ref()
                .map(|s| if facial { &s.facial_customization } else { &s.hair_customization })
                .filter(|t| !t.is_empty())
                .map_or("NORMAL", String::as_str);
            Ok(MultiValue::from_vec(vec![text(lua, token)?]))
        })?;
    }
    // BarberShopReset() (`0x52f900`, labelled: `0x52f7d0` unread): the preview returns to the
    // current appearance.
    set(lua, "BarberShopReset", |lua, _| {
        let mut m = lua.app_data_mut::<Model>().expect("model app_data");
        let state = &mut m.wrath_barber;
        if let Some(s) = &state.shop {
            state.preview = (s.hair_style, s.hair_color, s.facial_hair, s.skin);
        }
        Ok(MultiValue::new())
    })?;
    // CancelBarberShop() (`0x52ed80`, labelled: `0x52e9f0` unread): the preview resets and the
    // player leaves the chair.
    set(lua, "CancelBarberShop", |lua, _| {
        let mut m = lua.app_data_mut::<Model>().expect("model app_data");
        let state = &mut m.wrath_barber;
        if let Some(s) = &state.shop {
            state.preview = (s.hair_style, s.hair_color, s.facial_hair, s.skin);
            state.requests.push(WrathBarberRequest::Cancel);
        }
        Ok(MultiValue::new())
    })?;
    // ApplyBarberShopStyle() (`0x52eef0`, labelled: `0x52eda0` unread): the selected rows and
    // color go out.
    set(lua, "ApplyBarberShopStyle", |lua, _| {
        let mut m = lua.app_data_mut::<Model>().expect("model app_data");
        let state = &mut m.wrath_barber;
        let Some(shop) = state.shop.clone() else {
            return Ok(MultiValue::new());
        };
        let id = |cat| selected(state, &shop, cat).map_or(0, |r| r.id);
        let request = WrathBarberRequest::Apply {
            hair: id(0),
            hair_color: state.preview.1,
            facial_hair: id(2),
            skin_color: if is_current(state, &shop, 3) { 0 } else { id(3) },
        };
        state.requests.push(request);
        Ok(MultiValue::new())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::UiScript;

    fn style(id: u32, name: &str, cost_modifier: f32, data: u32) -> WrathBarberStyle {
        WrathBarberStyle { id, name: name.into(), description: format!("{name} desc"), cost_modifier, data }
    }

    #[test]
    fn styles_costs_cycling_and_verbs() {
        let mut s = UiScript::new().unwrap();
        assert!(s.eval::<bool>("return select('#', GetBarberShopStyleInfo(1)) == 0 and GetBarberShopTotalCost() == 0 and CanAlterSkin() == false and GetHairCustomization() == 'NORMAL'").unwrap());
        s.set_wrath_barber_shop(Some(WrathBarberShop {
            base_cost: 101.0,
            hair_styles: vec![style(1, "Short", 1.0, 0), style(2, "Long", 1.5, 1)],
            facial_styles: vec![style(10, "None", 0.75, 0), style(11, "Beard", 0.75, 1)],
            hair_colors: 3,
            hair_style: 0,
            hair_color: 2,
            facial_hair: 0,
            skin: 4,
            hair_customization: "HORNS".into(),
            ..Default::default()
        }));
        for check in [
            "select(1, GetBarberShopStyleInfo(1)) == 'Short' and select(3, GetBarberShopStyleInfo(1)) == 101 and select(4, GetBarberShopStyleInfo(1)) == 1",
            "select(1, GetBarberShopStyleInfo(2)) == nil and select(3, GetBarberShopStyleInfo(2)) == 50 and select('#', GetBarberShopStyleInfo(4)) == 0",
            "GetBarberShopTotalCost() == 0 and CanAlterSkin() == false and GetHairCustomization() == 'HORNS' and GetFacialHairCustomization() == 'NORMAL'",
        ] {
            assert!(s.eval::<bool>(&format!("return {check}")).unwrap(), "{check}");
        }
        // A new color costs half the base; a new style replaces that, and facial hair adds.
        s.run("SetNextBarberShopStyle(2)").unwrap();
        assert_eq!(s.wrath_barber_preview(), Some((0, 0, 0, 4)));
        assert_eq!(s.eval::<i64>("return GetBarberShopTotalCost()").unwrap(), 50);
        s.run("SetNextBarberShopStyle(1) SetNextBarberShopStyle(3, true)").unwrap();
        assert!(s.eval::<bool>("return select(1, GetBarberShopStyleInfo(1)) == 'Long' and select(4, GetBarberShopStyleInfo(1)) == nil").unwrap());
        assert_eq!(s.eval::<i64>("return GetBarberShopTotalCost()").unwrap(), 152 + 76);
        s.run("ApplyBarberShopStyle() BarberShopReset()").unwrap();
        assert_eq!(s.eval::<i64>("return GetBarberShopTotalCost()").unwrap(), 0);
        s.run("CancelBarberShop()").unwrap();
        assert_eq!(
            s.take_wrath_barber_requests(),
            vec![
                WrathBarberRequest::Apply { hair: 2, hair_color: 0, facial_hair: 11, skin_color: 0 },
                WrathBarberRequest::Cancel,
            ]
        );
        assert!(s.run("GetBarberShopStyleInfo()").is_err() && s.run("SetNextBarberShopStyle()").is_err());
        s.set_wrath_barber_shop(None);
        assert_eq!(s.wrath_barber_preview(), None);
    }
}
