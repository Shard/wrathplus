//! The spell and item verbs that read live game state, answered in the call through the host's
//! [`GameQuery`](super::GameQuery): IsSpellInRange, IsUsableSpell, IsUsableItem and
//! IsItemInRange.

use mlua::{Lua, MultiValue, Value};

use super::binding_abi::flag;
use super::query::ask;
use super::spell_arg::{set_slot_verb, spell_arg_typed};
use super::spellbook::{book_slot, is_pet_book};
use super::Model;

/// The unit a verb names at Lua index `at` through `0x60abf0` with its default-to-target flag set
/// (`0x60b029`): no string, or an empty one, is the current target `[0xbd07b0]`; a token that does
/// not parse is `None` (nil); one that parses but names nobody is guid 0, which no unit holds.
fn unit_arg(lua: &Lua, model: &Model, args: &MultiValue, at: usize) -> Option<u64> {
    let token = match args.get(at - 1) {
        Some(v @ (Value::String(_) | Value::Integer(_) | Value::Number(_))) => lua
            .coerce_string(v.clone())
            .ok()
            .flatten()
            .map(|s| s.to_string_lossy()),
        _ => None,
    };
    match token.as_deref() {
        None | Some("") => Some(model.unit_guids.target),
        Some(token) => model.unit_guids.resolve(token).ok().map(|g| g.unwrap_or(0)),
    }
}

/// What an item verb asks about item `entry`: its use spell, the template's first trigger-0 spell
/// (`0x510ef0` → `0x706b90`, as `0x80ac10` scans `+0x100`/`+0x114`); `None` with no template or
/// none. Labelled: the equipped copy's enchant use spell (`0x5a86b0`) is not read.
pub(super) fn item_ask(model: &Model, entry: u32) -> Option<super::ItemAsk> {
    let spells = &model.item_templates.get(&entry)?.item_spells;
    let &(_, spell, _, _) = spells.iter().find(|s| s.0 == 0 && s.1 != 0)?;
    Some(super::ItemAsk { entry, spell })
}

/// A number, as the reference pushes `fld1`/`fldz` (`0x541d91`, `0x541daa`): 0 is truthy in Lua.
fn number(b: bool) -> Value {
    Value::Integer(i64::from(b))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // IsSpellInRange(spell[, unit]) (`0x541c60`): the shared resolver (no values on a miss), the
    // slot's Spell.dbc row and its caster, the player or the pet for the pet book; then the unit
    // at the resolver's `argType` (`0x541d34`) and `0x809610`: 1 in range, 0 out, nil when any
    // step fails.
    lua.globals().set(
        "IsSpellInRange",
        lua.create_function(|lua, args: MultiValue| {
            let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Nil);
            let ask_for = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let at = spell_arg_typed(lua, &model, &arg(0), &arg(1), "IsSpellInRange", true)?;
                let Some((id, book, arg_type)) = at else {
                    return Ok(MultiValue::new());
                };
                let spell = book_slot(&model, id, &book).map_or(0, |s| s.spell_id);
                let unit = unit_arg(lua, &model, &args, arg_type);
                unit.filter(|_| spell != 0)
                    .map(|unit| (spell, is_pet_book(&book), unit))
            };
            let answer = ask_for
                .and_then(|(spell, pet, unit)| ask(lua, |q| q.spell_in_range(spell, pet, unit)));
            Ok(MultiValue::from_iter([answer.map_or(Value::Nil, number)]))
        })?,
    )?;
    // IsUsableSpell(spell) (`0x541680`): the shared resolver (no values on a miss), then two
    // values: 1, nil when the cast legs (`0x80b5d0`) and the power check (`0x8017e0`) pass; nil, 1
    // when only the power falls short (`0x541783`); nil, nil when the slot is empty, its row or
    // its caster is missing (`0x54170f`, `0x54174f`), or a cast leg fails.
    set_slot_verb(lua, "IsUsableSpell", |lua, id, book| {
        let spell = {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            book_slot(&model, id, book).map_or(0, |s| s.spell_id)
        };
        let pet = is_pet_book(book);
        let pair = (spell != 0)
            .then(|| ask(lua, |q| q.spell_usable(spell, pet)))
            .flatten();
        let (usable, no_mana) = pair.unwrap_or((false, false));
        Ok((flag(usable), flag(no_mana)))
    })?;
    // IsUsableItem(item) (`0x51c450`): the shared item resolver, then two values as
    // IsUsableSpell's; nil, nil without a player or for id 0 (`0x51c4c5`, `0x51c4cd`). Its cooldown
    // test asks for an uncached template (`0x80ac26` passes the request callback).
    lua.globals().set(
        "IsUsableItem",
        lua.create_function(|lua, arg: Value| {
            let item = {
                let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                let id = super::item_arg::item_id_arg(lua, &model, &arg);
                if (id as i32) > 0 && !model.item_templates.contains_key(&id) {
                    model.item_stat_asks.insert(id);
                }
                let bare = super::ItemAsk {
                    entry: id,
                    spell: 0,
                };
                (id != 0).then(|| item_ask(&model, id).unwrap_or(bare))
            };
            let pair = item.and_then(|item| ask(lua, |q| q.item_usable(item)));
            let (usable, no_mana) = pair.unwrap_or((false, false));
            Ok((flag(usable), flag(no_mana)))
        })?,
    )?;
    // IsItemInRange(item[, unit]) (`0x51c9c0`): the item's use spell and its row, the player,
    // the unit at argument 2 through `0x60abf0` (an absent one is the target), then `0x809610`:
    // 1, 0 or nil, as IsSpellInRange.
    lua.globals().set(
        "IsItemInRange",
        lua.create_function(|lua, args: MultiValue| {
            let ask_for = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let item = args.front().cloned().unwrap_or(Value::Nil);
                let id = super::item_arg::item_id_arg(lua, &model, &item);
                let spell = item_ask(&model, id).map(|a| a.spell);
                spell.zip(unit_arg(lua, &model, &args, 2))
            };
            let answer = ask_for
                .and_then(|(spell, unit)| ask(lua, |q| q.spell_in_range(spell, false, unit)));
            Ok(answer.map_or(Value::Nil, number))
        })?,
    )?;
    Ok(())
}
