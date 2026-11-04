//! Build 12340's item queries over the shared item resolver (`super::item_arg`): GetItemCount,
//! GetItemFamily, GetItemCooldown, IsEquippedItemType and the GetExtendedItemInfo stub.

use mlua::{Lua, MultiValue, Value};

use super::Model;

/// The walker's broken-item skip (`0x754792`-`0x7547c7`), lifted by flag `0x80`: an item whose
/// max durability is set and whose durability is 0, unless wrapped (`ITEM_FIELD_FLAGS & 0x8`), or
/// one flagged `0x10`.
fn broken(durability: Option<(u32, u32)>, flags: u32) -> bool {
    let worn_out = durability.is_some_and(|(cur, max)| max > 0 && cur == 0);
    (flags & 0x8 == 0 && worn_out) || flags & 0x10 != 0
}

/// Every item the player walker `0x7546f0` visits under `flags`, as `(entry, stack, broken)`.
/// The regions are `0x753a50`'s: `0x1` equipment, `0x2` the equipped bags, `0x4` the backpack,
/// `0x8` the bank and its bags, `0x40` the keyring, `0x200` the hidden currency-token bag; a bag
/// in a walked region is walked too. Labelled: the visit order is not the walker's (`0xa37b14`),
/// which no counting caller can observe.
fn walk(model: &Model, flags: u32) -> Vec<(u32, u32, bool)> {
    let mut out = Vec::new();
    let mut slots = |slots: &[Option<super::InvSlotView>]| {
        for s in slots.iter().flatten().filter(|s| s.item_id != 0) {
            out.push((s.item_id, s.count, broken(s.durability, s.flags)));
        }
    };
    if flags & 0x1 != 0 {
        slots(&model.inventory_slots[1..=19]); // 0 is the ammo slot's carried total
    }
    if flags & 0x2 != 0 {
        slots(&model.inventory_slots[20..=23]);
    }
    if flags & 0x8 != 0 {
        slots(&model.bank_bag_slots);
    }
    let bags: &[(u32, &[i64])] = &[
        (0x2, &[1, 2, 3, 4]),
        (0x4, &[0]),
        (0x8, &[-1, 5, 6, 7, 8, 9, 10, 11]),
        (0x40, &[-2]),
    ];
    for &(bit, ids) in bags {
        if flags & bit == 0 {
            continue;
        }
        for c in ids.iter().filter_map(|id| model.containers.get(id)) {
            for s in c.slots.values().filter(|s| s.item_id != 0) {
                out.push((s.item_id, s.count, broken(s.durability, s.flags)));
            }
        }
    }
    if flags & 0x200 != 0 {
        let tokens = &model.wrath_currency.token_stacks;
        out.extend(tokens.iter().map(|&(entry, count)| (entry, count, false)));
    }
    out
}

/// `0x754d00`: the summed stacks (`0x754560`, `ITEM_FIELD_STACK_COUNT`) of `id`, or of every item
/// for `0xffffffff`. A flag word missing all of `0x247` gains them (`0x7546ff`), so `0x8` adds the
/// bank to the default and `0x80` the broken items. Labelled: `0xfffffffe`'s own walk
/// (`0x754470`) is not modelled.
pub(super) fn item_count(model: &Model, id: u32, flags: u32) -> u32 {
    let flags = if flags & 0x247 == 0 {
        flags | 0x247
    } else {
        flags
    };
    walk(model, flags)
        .into_iter()
        .filter(|&(entry, _, broken)| {
            (entry == id || id == u32::MAX) && (flags & 0x80 != 0 || !broken)
        })
        .fold(0u32, |n, (_, count, _)| n.wrapping_add(count))
}

/// `GetItemCooldown`'s use spell (`0x809063`): the template's first trigger-0 spell slot.
fn has_use_spell(t: &super::ItemTemplateView) -> bool {
    t.item_spells
        .iter()
        .any(|&(trigger, id, _, _)| trigger == 0 && id != 0)
}

/// IsEquippedItemType's match (`0x51c7a0`): an `INVTYPE_*` token or its global string selects an
/// `InventoryType` (callback `0x5176f0`); any other text is an item class name or either subclass
/// name (`0x517740`, `+0x2c` and `+0x28`), all without case. Labelled: `0x517740`'s fourth table
/// (`0xad3f28`) is not decoded.
fn equipped_type(lua: &Lua, model: &Model, text: &str) -> bool {
    let same = |s: &str| s.eq_ignore_ascii_case(text);
    let global = |token: &str| -> Option<String> { lua.globals().get(token).ok().flatten() };
    let inventory_type = (0..29u32).find(|&i| {
        let token = super::item_stats::equip_loc_token(i);
        same(token) || global(token).is_some_and(|g| same(&g))
    });
    // Walker flags 1: the worn items alone, broken ones skipped (no `0x80`).
    model.inventory_slots[1..=19]
        .iter()
        .flatten()
        .filter(|s| s.item_id != 0 && !broken(s.durability, s.flags))
        .filter_map(|s| model.item_templates.get(&s.item_id))
        .any(|t| match inventory_type {
            Some(i) if i > 0 => t.inventory_type == i,
            _ => [&t.item_type, &t.item_sub_type, &t.sub_class_display]
                .into_iter()
                .any(|n| n.as_deref().is_some_and(same)),
        })
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // GetItemCount(item [, includeBank [, includeCharges]]) (`0x51c2e0`) → one number, never nil:
    // 0 without a player (`0x51c368`) or for an id nothing matches. The flags read as
    // `GetBoolOrDefault` (`0x815500`) and select bank `0x8` and broken items `0x80`. Labelled:
    // includeCharges's charge count for an unstackable item with a use spell (`0x51c3c4`-
    // `0x51c42b`, `0x754d50`/`0x801770`) is not modelled; its walk is.
    g.set(
        "GetItemCount",
        lua.create_function(|lua, args: MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let id = super::item_arg::item_id_arg(lua, &model, args.front().unwrap_or(&Value::Nil));
            let bank = super::binding_abi::bool_or_default(args.get(1), false);
            let charges = super::binding_abi::bool_or_default(args.get(2), false);
            if model.unit_guids.player == 0 {
                return Ok(0.0);
            }
            let flags = if bank { 0x8 } else { 0 } | if charges { 0x80 } else { 0 };
            Ok(f64::from(item_count(&model, id, flags)))
        })?,
    )?;
    // GetItemFamily(item) (`0x510f20`) → the cached template's `BagFamily` mask (`+0x1b8`), or
    // nothing; a miss queries nothing.
    g.set(
        "GetItemFamily",
        lua.create_function(|lua, item: Value| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let id = super::item_arg::item_id_arg(lua, &model, &item);
            let family = model
                .item_templates
                .get(&id)
                .map(|t| f64::from(t.bag_family));
            Ok(MultiValue::from_iter(family.map(Value::Number)))
        })?,
    )?;
    // GetItemCooldown(item) (`0x510fc0`) → start, duration, enable, always three: 0, 0, 0 for an
    // uncached template (asked for: `0x809046` passes the request callback) or one with no use
    // spell; else the use spell's record from the cooldown manager (`0x807980`), which answers
    // 0, 0, 1 when none runs. Labelled: read from a carried copy's cooldown, the bags' snapshot
    // (as `GetContainerItemCooldown`), so a worn or uncarried item reads 0, 0, 1; the
    // enchant-granted use spell of an equipped copy (`0x5a86b0`) is not read.
    g.set(
        "GetItemCooldown",
        lua.create_function(|lua, item: Value| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let now = model.now;
            let id = super::item_arg::item_id_arg(lua, &model, &item);
            let Some(t) = model.item_templates.get(&id) else {
                if (id as i32) > 0 {
                    model.item_stat_asks.insert(id);
                }
                return Ok((0.0, 0.0, 0));
            };
            if !has_use_spell(t) {
                return Ok((0.0, 0.0, 0));
            }
            let held = |&(bag, slot): &(i64, u32)| {
                model
                    .containers
                    .get(&bag)
                    .and_then(|c| c.slots.get(&slot))
                    .is_some_and(|s| s.item_id == id)
            };
            Ok(model
                .container_cooldowns
                .iter()
                .filter(|(at, _)| held(at))
                .find(|(_, &(start, duration, enabled))| start + duration > now || !enabled)
                .map_or((0.0, 0.0, 1), |(_, &(start, duration, enabled))| {
                    (start, duration, i32::from(enabled))
                }))
        })?,
    )?;
    // IsEquippedItemType(type) (`0x51c740`) → 1 when a worn item matches, else nil; a nil or
    // empty type raises the usage error (`0x51c856`).
    g.set(
        "IsEquippedItemType",
        lua.create_function(|lua, kind: Value| {
            let text = match &kind {
                Value::String(_) | Value::Integer(_) | Value::Number(_) => lua
                    .coerce_string(kind)?
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default(),
                _ => String::new(),
            };
            if text.is_empty() {
                return Err(mlua::Error::runtime("Usage: IsEquippedItemType(\"type\")"));
            }
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let worn = model.unit_guids.player != 0 && equipped_type(lua, &model, &text);
            Ok(worn.then_some(1))
        })?,
    )?;
    // GetExtendedItemInfo (`0x8e5250`) is the shared `xor eax, eax; ret` stub: nothing.
    g.set(
        "GetExtendedItemInfo",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "wrath_item_queries_tests.rs"]
mod tests;
