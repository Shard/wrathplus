//! Build-12340 socketed gems of bag and worn items: GetContainerItemGems (`0x5d9300`) and
//! GetInventoryItemGems (`0x5ea4f0`) over the host's gem table (Wire B fills it from the items'
//! socket enchant slots and SpellItemEnchantment's source item).

use mlua::{Lua, MultiValue, Value};

use super::cursor::EQUIPMENT_BAG;
use super::Model;

impl super::UiScript {
    /// The socketed gems by slot, whole: `(bag, slot, gem item ids per socket)`, the bag in the
    /// Lua numbering or [`EQUIPMENT_BAG`] with a 1-based inventory slot id.
    pub fn set_wrath_item_gems(&mut self, gems: Vec<(i64, u32, [Option<u32>; 3])>) {
        self.model_mut().wrath_item_gems = gems;
    }
}

fn gems(lua: &Lua, bag: i64, slot: u32) -> mlua::Result<MultiValue> {
    let model = lua.app_data_ref::<Model>().expect("model app_data");
    let row = model
        .wrath_item_gems
        .iter()
        .find(|(b, s, _)| *b == bag && *s == slot)
        .map_or([None; 3], |(_, _, g)| *g);
    Ok(MultiValue::from_vec(
        row.iter()
            .map(|g| g.map_or(Value::Nil, |id| Value::Integer(id.into())))
            .collect(),
    ))
}

fn number(lua: &Lua, v: Value) -> Option<i64> {
    lua.coerce_number(v)
        .ok()
        .flatten()
        .map(|n| n.trunc() as i64)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // GetContainerItemGems(bag, slot) → three gem item ids, nil per empty socket. Labelled: an
    // empty slot answers three nils here.
    lua.globals().set(
        "GetContainerItemGems",
        lua.create_function(|lua, (bag, slot): (Value, Value)| {
            let (Some(bag), Some(slot)) = (number(lua, bag), number(lua, slot)) else {
                return Err(mlua::Error::runtime(
                    "Usage: GetContainerItemGems(bag, slot)",
                ));
            };
            gems(lua, bag, slot as u32)
        })?,
    )?;
    // GetInventoryItemGems(slot) → the worn item's three gem item ids; nothing for an empty slot.
    lua.globals().set(
        "GetInventoryItemGems",
        lua.create_function(|lua, slot: Value| {
            let Some(slot) = number(lua, slot) else {
                return Err(mlua::Error::runtime("Usage: GetInventoryItemGems(slot)"));
            };
            let worn = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                model.unit_guids.player != 0
                    && usize::try_from(slot)
                        .ok()
                        .and_then(|s| model.inv_slot("player", s))
                        .is_some_and(|s| s.item_id != 0)
            };
            if !worn {
                return Ok(MultiValue::new());
            }
            gems(lua, EQUIPMENT_BAG, slot as u32)
        })?,
    )
}

#[cfg(test)]
mod tests {
    use crate::script::{InvSlotView, UiScript, UnitState, EQUIPMENT_BAG};

    #[test]
    fn gems_read_the_host_table() {
        let mut s = UiScript::new().unwrap();
        s.model_mut().unit_guids.player = 1;
        s.set_unit(
            "player",
            Some(UnitState {
                guid: 1,
                exists: true,
                ..Default::default()
            }),
        );
        let mut slots: crate::script::InventorySlots = Default::default();
        slots[1] = Some(InvSlotView {
            item_id: 5,
            ..Default::default()
        });
        s.set_inventory_slots(slots);
        s.set_wrath_item_gems(vec![
            (0, 3, [Some(40111), None, Some(40112)]),
            (EQUIPMENT_BAG, 1, [Some(41398), None, None]),
        ]);
        assert!(s
            .eval::<bool>("local a, b, c = GetContainerItemGems(0, 3) return a == 40111 and b == nil and c == 40112 and select('#', GetContainerItemGems(0, 4)) == 3 and GetInventoryItemGems(1) == 41398 and select('#', GetInventoryItemGems(2)) == 0")
            .unwrap());
        assert!(
            s.run("GetContainerItemGems(0)").is_err() && s.run("GetInventoryItemGems()").is_err()
        );
    }
}
