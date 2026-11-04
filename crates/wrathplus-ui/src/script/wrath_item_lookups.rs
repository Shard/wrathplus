//! Build 12340's GetItemUniqueness and GetItemGem, over two host tables the engine cannot read
//! itself: each cached template's `ItemLimitCategory` row and `SpellItemEnchantment`'s source gem.

use std::collections::HashMap;

use mlua::{Lua, MultiValue, Value};

use super::{Model, UiScript};

/// Item entry → `(ItemLimitCategory id, its row's Quantity)`, for templates whose category
/// (`+0x1ec`) names a row.
#[derive(Default)]
struct ItemLimits(HashMap<u32, (u32, u32)>);

/// Socket enchant id → the gem item it comes from (`SpellItemEnchantment` `+0x44`).
#[derive(Default)]
struct GemEnchants(HashMap<u32, u32>);

impl UiScript {
    /// Publish the limit categories of the cached templates that carry one with a row.
    pub fn set_item_limit_categories(&mut self, limits: HashMap<u32, (u32, u32)>) {
        self.lua.set_app_data(ItemLimits(limits));
    }

    /// Publish `SpellItemEnchantment`'s gem enchants, enchant id → gem item.
    pub fn set_gem_enchants(&mut self, gems: HashMap<u32, u32>) {
        self.lua.set_app_data(GemEnchants(gems));
    }
}

/// GetItemGem's by-name search (`0x754b90`, default walk `0x247`): the first carried or worn item
/// whose template has the name, as `(bag, slot)` in the `wrath_item_gems` numbering. Labelled: the
/// name compares without case; the walk is backpack, bags, keyring, then equipment (`0xa37b14`).
fn named_item(model: &Model, name: &str) -> Option<(i64, u32)> {
    let named = |id: u32| {
        model
            .item_templates
            .get(&id)
            .is_some_and(|t| t.name.eq_ignore_ascii_case(name))
    };
    let carried = [0, 1, 2, 3, 4, -2].into_iter().find_map(|bag| {
        let c = model.containers.get(&bag)?;
        let mut slots: Vec<_> = c.slots.iter().filter(|(_, s)| named(s.item_id)).collect();
        slots.sort_by_key(|(slot, _)| **slot);
        slots.first().map(|(&slot, _)| (bag, slot))
    });
    carried.or_else(|| {
        (1..=19u32).find_map(|slot| {
            let s = model.inventory_slots[slot as usize].as_ref()?;
            named(s.item_id).then_some((super::cursor::EQUIPMENT_BAG, slot))
        })
    })
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.set_app_data(ItemLimits::default());
    lua.set_app_data(GemEnchants::default());
    let g = lua.globals();
    // GetItemUniqueness(item) (`0x516ed0`) → for a cached template flagged `0x80000`, -1 and 1;
    // else its limit category and that row's quantity; else nothing. The argument is
    // GetItemInfo's shape: truncated, an item string, a name, or the usage error (`0x516ffd`).
    g.set(
        "GetItemUniqueness",
        lua.create_function(|lua, item: Value| {
            const USAGE: &str = "Usage: GetItemUniqueness(itemID|\"name\"|\"itemlink\")";
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let id = super::item_arg::info_item_id(lua, &model, &item, USAGE)?;
            let Some(t) = model.item_templates.get(&id) else {
                return Ok(MultiValue::new());
            };
            let pair = if t.flags & 0x8_0000 != 0 {
                Some((-1.0, 1.0))
            } else {
                let limits = lua.app_data_ref::<ItemLimits>();
                let row = limits.and_then(|l| l.0.get(&id).copied());
                row.map(|(category, quantity)| (f64::from(category), f64::from(quantity)))
            };
            Ok(pair.map_or_else(MultiValue::new, |(a, b)| {
                MultiValue::from_vec(vec![Value::Number(a), Value::Number(b)])
            }))
        })?,
    )?;
    // GetItemGem(item, index) (`0x51c150`) → the gem in socket 1-3 as name and link, or nothing:
    // an item string's gem field (`0x50f630`), else a carried item found by name, its socket
    // enchant (`0x518b30`); the enchant's gem (`0x65c290`, `+0x44`) answers only from the template
    // cache (no query, `0x51c25c`), its link built as GetItemInfo's (`0x61e360` → `0x61e290`).
    g.set(
        "GetItemGem",
        lua.create_function(|lua, (item, index): (Value, Value)| {
            const USAGE: &str = "Usage: GetItemGem(\"name\"|\"itemlink\", index)";
            let text = match &item {
                Value::String(_) | Value::Integer(_) | Value::Number(_) => {
                    lua.coerce_string(item)?.map(|s| s.to_string_lossy())
                }
                _ => None,
            };
            let (Some(text), Some(index)) = (text, lua.coerce_number(index)?) else {
                return Err(mlua::Error::runtime(USAGE));
            };
            let index = index.trunc() as i64;
            if !(1..=3).contains(&index) {
                return Ok(MultiValue::new());
            }
            let gem = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                match text.find("item:") {
                    Some(at) => {
                        // `0x50f630` fails, leaving the gem fields 0, for an id that is not > 0.
                        let fields: Vec<i64> = text[at + 5..]
                            .split(':')
                            .map(super::item_stats::reference_atoi)
                            .collect();
                        let enchant = match fields.first() {
                            Some(&id) if id > 0 => fields.get(index as usize + 1).copied(),
                            _ => None,
                        }
                        .map_or(0, |f| f as u32);
                        let gems = lua.app_data_ref::<GemEnchants>();
                        gems.and_then(|g| g.0.get(&enchant).copied())
                    }
                    None if model.unit_guids.player != 0 => {
                        named_item(&model, &text).and_then(|(bag, slot)| {
                            let row = model
                                .wrath_item_gems
                                .iter()
                                .find(|r| (r.0, r.1) == (bag, slot));
                            row.and_then(|r| r.2[index as usize - 1])
                        })
                    }
                    None => None,
                }
                .filter(|gem| model.item_templates.contains_key(gem))
            };
            let Some(gem) = gem else {
                return Ok(MultiValue::new());
            };
            let info: mlua::Function = lua.globals().get("GetItemInfo")?;
            let values: MultiValue = info.call(gem)?;
            Ok(values.into_iter().take(2).collect())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{ContainerSlot, ContainerState, ItemTemplateView, UiScript, EQUIPMENT_BAG};

    fn gem(s: &UiScript, args: &str) -> (Option<String>, Option<String>) {
        s.eval(&format!("return GetItemGem({args})")).unwrap()
    }

    #[test]
    fn uniqueness_and_gems_read_the_host_tables() {
        let mut s = UiScript::new().unwrap();
        s.model_mut().unit_guids.player = 1;
        let named = |name: &str, flags: u32| ItemTemplateView {
            name: name.into(),
            flags,
            quality: 3,
            ..Default::default()
        };
        s.set_item_template(1, named("Unique Ring", 0x8_0000));
        s.set_item_template(2, named("Limited Gem Box", 0));
        s.set_item_template(3, named("Plain", 0));
        s.set_item_template(40112, named("Bold Scarlet Ruby", 0));
        s.set_item_template(50, named("Helm", 0));
        s.set_item_limit_categories([(2, (7, 3))].into());
        let pair = |arg: &str| {
            s.eval::<(f64, f64)>(&format!("return GetItemUniqueness({arg})"))
                .unwrap()
        };
        assert_eq!(pair("1"), (-1.0, 1.0));
        assert_eq!(pair("'item:2:0'"), (7.0, 3.0));
        assert_eq!(pair("2.9"), (7.0, 3.0), "truncated");
        assert_eq!(s.arity("GetItemUniqueness(3)").unwrap(), 0);
        assert_eq!(s.arity("GetItemUniqueness(99)").unwrap(), 0);
        assert!(s.run("GetItemUniqueness(nil)").is_err());

        s.set_gem_enchants([(3312, 40112)].into());
        let ruby = Some("Bold Scarlet Ruby".to_string());
        assert_eq!(
            gem(
                &s,
                "'|cff0070dd|Hitem:50:0:3312:0:0:0:0:0:80|h[Helm]|h|r', 1"
            )
            .0,
            ruby
        );
        assert!(gem(&s, "'item:50:0:3312:0:0', 1")
            .1
            .unwrap()
            .contains("|Hitem:40112:"));
        assert_eq!(
            gem(&s, "'item:50:0:0:3312:0', 1").0,
            None,
            "socket 1 is empty"
        );
        assert_eq!(gem(&s, "'item:50:0:0:3312:0', 2").0, ruby);
        // By name: the first carried or worn item, its socketed gem.
        s.set_container(
            0,
            Some(ContainerState {
                num_slots: 16,
                slots: [(
                    4,
                    ContainerSlot {
                        item_id: 50,
                        count: 1,
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            }),
        );
        s.set_wrath_item_gems(vec![
            (0, 4, [None, None, Some(40112)]),
            (EQUIPMENT_BAG, 1, [Some(40112), None, None]),
        ]);
        assert_eq!(gem(&s, "'helm', 3").0, ruby);
        assert_eq!(
            gem(&s, "'Helm', 1").0,
            None,
            "the backpack copy is found first"
        );
        assert_eq!(s.arity("GetItemGem('Helm', 4)").unwrap(), 0);
        assert!(s.run("GetItemGem({}, 1)").is_err() && s.run("GetItemGem('Helm')").is_err());
    }
}
