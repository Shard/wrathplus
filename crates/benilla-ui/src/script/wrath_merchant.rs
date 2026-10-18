//! Wrath merchant extras (`MerchantFrame.lua`): extended costs and the buyback link, from build
//! 12340's bindings. `GetMerchantItemInfo` (`0x584e10`) gains its seventh return, `extendedCost`,
//! and its `isUsable` also tests the cost's personal arena rating (`0x5a37b0`);
//! `GetMerchantItemCostInfo` (`0x5846e0`) and `GetMerchantItemCostItem` (`0x584820`) read the
//! row's `ItemExtendedCost.dbc` record; `GetBuybackItemLink` (`0x5852c0`) links a buyback slot.

use mlua::{Lua, MultiValue, Value};

use super::Model;

/// One item an extended cost asks for, resolved by the app.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CostItemView {
    pub item_id: u32,
    pub count: u32,
    /// The template's icon path; `None` while it is in flight (the getter answers nil).
    pub texture: Option<String>,
    pub link: Option<String>,
}

/// A vendor row's `ItemExtendedCost.dbc` record, as the bindings answer it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtendedCostView {
    pub honor: u32,
    pub arena: u32,
    /// The record's nonzero item slots, in slot order.
    pub items: Vec<CostItemView>,
    /// `0x5a37b0`'s answer for the record's personal rating; true when it asks none.
    pub rating_ok: bool,
}

fn index_arg(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(f) => Some(f.round() as i64),
        Value::String(s) => s
            .to_str()
            .ok()?
            .trim()
            .parse::<f64>()
            .ok()
            .map(|f| f.round() as i64),
        _ => None,
    }
}

fn text(lua: &Lua, s: &Option<String>) -> mlua::Result<Value> {
    s.as_deref()
        .map_or(Ok(Value::Nil), |s| lua.create_string(s).map(Value::String))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // name, texture, price, quantity, numAvailable, isUsable, extendedCost; the invalid-row
    // seven-tuple is nil, nil, 0, 1, 0, 1, nil.
    g.set(
        "GetMerchantItemInfo",
        lua.create_function(|lua, v: Value| {
            let index = index_arg(&v)
                .ok_or_else(|| mlua::Error::runtime("Usage: GetMerchantItemInfo(index)"))?;
            let (item, usable) = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let item = usize::try_from(index - 1)
                    .ok()
                    .and_then(|i| model.merchant.as_ref()?.items.get(i))
                    .cloned();
                let usable = item.as_ref().is_none_or(|it| {
                    super::item_stats::item_usable_by_id(&model, it.item_id)
                        && it.extended_cost.as_ref().is_none_or(|c| c.rating_ok)
                });
                (item, usable)
            };
            let Some(it) = item else {
                return Ok(MultiValue::from_vec(vec![
                    Value::Nil,
                    Value::Nil,
                    Value::Number(0.0),
                    Value::Number(1.0),
                    Value::Number(0.0),
                    Value::Number(1.0),
                    Value::Nil,
                ]));
            };
            Ok(MultiValue::from_vec(vec![
                text(lua, &it.name)?,
                text(lua, &it.texture)?,
                Value::Integer(i64::from(it.price)),
                Value::Integer(i64::from(it.quantity)),
                Value::Integer(i64::from(it.num_available)),
                if usable {
                    Value::Integer(1)
                } else {
                    Value::Nil
                },
                if it.extended_cost.is_some() {
                    Value::Integer(1)
                } else {
                    Value::Nil
                },
            ]))
        })?,
    )?;
    // honorPoints, arenaPoints, itemCount; 0, 0, 0 without a record.
    g.set(
        "GetMerchantItemCostInfo",
        lua.create_function(|lua, v: Value| {
            let index = index_arg(&v)
                .ok_or_else(|| mlua::Error::runtime("Usage: GetMerchantItemCostInfo(index)"))?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let cost = usize::try_from(index - 1)
                .ok()
                .and_then(|i| model.merchant.as_ref()?.items.get(i)?.extended_cost.clone());
            Ok(cost.map_or((0, 0, 0), |c| (c.honor, c.arena, c.items.len() as u32)))
        })?,
    )?;
    // itemTexture, itemValue, itemLink for the record's n-th nonzero item; a template in flight
    // answers (nil, count), a missing one (nil, 0).
    g.set(
        "GetMerchantItemCostItem",
        lua.create_function(|lua, (a, b): (Value, Value)| {
            let (Some(index), Some(n)) = (index_arg(&a), index_arg(&b)) else {
                return Err(mlua::Error::runtime(
                    "Usage: GetMerchantItemCostItem(index, itemIndex)",
                ));
            };
            let item = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                usize::try_from(index - 1).ok().and_then(|i| {
                    let cost = model
                        .merchant
                        .as_ref()?
                        .items
                        .get(i)?
                        .extended_cost
                        .as_ref()?;
                    cost.items.get(usize::try_from(n - 1).ok()?).cloned()
                })
            };
            Ok(match item {
                Some(c) if c.texture.is_some() => MultiValue::from_vec(vec![
                    text(lua, &c.texture)?,
                    Value::Integer(i64::from(c.count)),
                    text(lua, &c.link)?,
                ]),
                Some(c) => {
                    MultiValue::from_vec(vec![Value::Nil, Value::Integer(i64::from(c.count))])
                }
                None => MultiValue::from_vec(vec![Value::Nil, Value::Integer(0)]),
            })
        })?,
    )?;
    g.set(
        "GetBuybackItemLink",
        lua.create_function(|lua, v: Value| {
            let slot = index_arg(&v)
                .ok_or_else(|| mlua::Error::runtime("Usage: GetBuybackItemLink(slot)"))?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let link = usize::try_from(slot - 1)
                .ok()
                .and_then(|i| model.merchant.as_ref()?.buyback.get(i)?.link.clone());
            Ok(match link {
                Some(l) => MultiValue::from_vec(vec![Value::String(lua.create_string(l)?)]),
                None => MultiValue::new(),
            })
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{MerchantItem, MerchantState, ScriptDialect, UiScript};

    #[test]
    fn cost_bindings_follow_the_record() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let gold = MerchantItem {
            name: Some("Bread".into()),
            price: 25,
            quantity: 1,
            num_available: -1,
            ..Default::default()
        };
        let badge = MerchantItem {
            name: Some("Helm".into()),
            extended_cost: Some(ExtendedCostView {
                honor: 0,
                arena: 1500,
                rating_ok: false,
                items: vec![
                    CostItemView {
                        item_id: 40752,
                        count: 75,
                        texture: Some("Interface\\Icons\\Badge".into()),
                        link: Some("|Hitem:40752|h[Emblem]|h".into()),
                    },
                    CostItemView {
                        item_id: 99,
                        count: 2,
                        texture: None,
                        link: None,
                    },
                ],
            }),
            ..Default::default()
        };
        let sold = MerchantItem {
            link: Some("|Hitem:6948|h[Hearthstone]|h".into()),
            ..Default::default()
        };
        s.set_merchant(Some(MerchantState {
            items: vec![gold, badge],
            buyback: vec![sold],
            can_repair: false,
        }));
        s.run(
            r##"
            local _, _, price, _, _, usable, ext = GetMerchantItemInfo(1)
            assert(price == 25 and usable == 1 and ext == nil)
            _, _, _, _, _, usable, ext = GetMerchantItemInfo(2)
            assert(usable == nil and ext == 1, "an unmet rating reds the row")
            assert(select("#", GetMerchantItemInfo(9)) == 7 and select(7, GetMerchantItemInfo(9)) == nil)
            local honor, arena, n = GetMerchantItemCostInfo(2)
            assert(honor == 0 and arena == 1500 and n == 2)
            honor, arena, n = GetMerchantItemCostInfo(1)
            assert(honor == 0 and arena == 0 and n == 0)
            local tex, count, link = GetMerchantItemCostItem(2, 1)
            assert(tex:find("Badge") and count == 75 and link:find("40752"))
            assert(select("#", GetMerchantItemCostItem(2, 2)) == 2 and select(2, GetMerchantItemCostItem(2, 2)) == 2)
            assert(select(2, GetMerchantItemCostItem(2, 3)) == 0)
            assert(not pcall(GetMerchantItemCostItem, 2))
            assert(GetBuybackItemLink(1):find("6948") and GetBuybackItemLink(2) == nil)
            "##,
        )
        .unwrap();
    }
}
