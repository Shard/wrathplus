//! The spell and item verbs that read live game state, answered in the call through the host's
//! [`GameQuery`](super::GameQuery): IsSpellInRange and IsUsableSpell.

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
    Ok(())
}
