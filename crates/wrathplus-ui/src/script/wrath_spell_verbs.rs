//! Build 12340's static spell predicates over the shared resolver ([`super::spell_arg`]) and the
//! catalog row of the slot's spell: IsAttackSpell, IsHelpfulSpell, IsHarmfulSpell,
//! IsConsumableSpell, GetSpellCount, SpellHasRange and IsAutoRepeatSpell; and the book query
//! FindSpellBookSlotByID. GetShapeshiftForm is upstream's, in [`super::shapeshift`].

use mlua::{Lua, MultiValue, Value};

use super::binding_abi::{bool_or_default, flag};
use super::spell_arg::set_slot_verb;
use super::spellbook::{book_slot, is_pet_book};
use super::{Model, SpellInfoView};

/// The slot's spell (0 for an empty one) and its catalog row, which every predicate reads
/// through `0x4cfd20`.
fn slot_row<'a>(model: &'a Model, id: u32, book: &str) -> (u32, Option<&'a SpellInfoView>) {
    let spell_id = book_slot(model, id, book).map_or(0, |s| s.spell_id);
    (spell_id, model.spell_catalog.get(&spell_id))
}

/// A `1`/nil predicate of the slot's row; no row answers nil.
fn set_row_flag(
    lua: &Lua,
    verb: &'static str,
    test: fn(&SpellInfoView) -> bool,
) -> mlua::Result<()> {
    set_slot_verb(lua, verb, move |lua, id, book| {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        Ok(flag(slot_row(&model, id, book).1.is_some_and(test)))
    })
}

/// `0x7ffa30`: the casts the carried reagents allow, the least over the reagents of
/// `floor(GetItemCount(reagent) / count)` (`0x754d00`, default walk), skipping id -2 (`0x7ffa75`);
/// 0 with no reagent or no player.
pub(super) fn reagent_casts(model: &Model, row: &SpellInfoView) -> u32 {
    if model.unit_guids.player == 0 {
        return 0;
    }
    (row.reagents.iter())
        .filter(|&&(id, _)| id != 0xffff_fffe)
        .map(|&(id, count)| super::wrath_item_queries::item_count(model, id, 0) / count)
        .min()
        .unwrap_or(0)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // IsAttackSpell (`0x541420`): `Effect[0]` is ATTACK (78, `0x5414ac`).
    set_row_flag(lua, "IsAttackSpell", |row| row.is_attack)?;
    // IsHelpfulSpell (`0x541800`) / IsHarmfulSpell (`0x5418f0`): the classifier `0x7fe1b0`.
    set_row_flag(lua, "IsHelpfulSpell", |row| row.helpful)?;
    set_row_flag(lua, "IsHarmfulSpell", |row| row.harmful)?;
    // IsConsumableSpell (`0x5419f0`): any reagent with a positive count (`0x541a80`).
    set_row_flag(lua, "IsConsumableSpell", |row| !row.reagents.is_empty())?;

    // GetSpellCount (`0x540df0` → `0x801770`): one number, 0 for an empty slot or no row.
    set_slot_verb(lua, "GetSpellCount", |lua, id, book| {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let casts = slot_row(&model, id, book)
            .1
            .map_or(0, |row| reagent_casts(&model, row));
        Ok(f64::from(casts))
    })?;

    // SpellHasRange (`0x541af0`): `0x801650` for the player, or the pet for the pet book, with no
    // target, then 1 if either bound is nonzero (`0x541be2`); no unit answers nil (`0x541bba`).
    // Labelled: the row's range, no range spell mod (op 5), so a mod that lends range to a
    // rangeless spell is not seen.
    set_slot_verb(lua, "SpellHasRange", |lua, id, book| {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let unit = match is_pet_book(book) {
            true => model.unit_guids.pet,
            false => model.unit_guids.player,
        };
        let row = slot_row(&model, id, book).1;
        Ok(flag(unit != 0 && row.is_some_and(|row| row.has_range)))
    })?;

    // IsAutoRepeatSpell (`0x5415d0`): a player-book spell (`0x541631`) that is the auto-repeat
    // spell `[0xd397d0]` (`0x7fe130`) or `[0xd397cc]` (`0x7fe180`). Labelled: one auto-repeat
    // spell is carried, the running one.
    set_slot_verb(lua, "IsAutoRepeatSpell", |lua, id, book| {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let (spell_id, _) = slot_row(&model, id, book);
        let auto_repeat = model.current_spells.auto_repeat;
        Ok(flag(
            spell_id != 0 && !is_pet_book(book) && auto_repeat == Some(spell_id),
        ))
    })?;

    // FindSpellBookSlotByID(spellID [, isPet]) (`0x53b6c0`): the id rounds (`lua_tointeger`) and
    // must be positive, else the usage error (`0xa0aeb4`); `isPet` reads as `GetBoolOrDefault`.
    // The answer is `0x53b4e0`'s raw 0-based index of the last slot holding it (pushed with no
    // +1, `0x53b726`), or nil.
    lua.globals().set(
        "FindSpellBookSlotByID",
        lua.create_function(|lua, args: MultiValue| {
            let usage = || mlua::Error::runtime("Usage: FindSpellBookSlotByID(spellID[, isPet])");
            let arg = args.front().cloned().unwrap_or(Value::Nil);
            let id = lua.coerce_number(arg)?.ok_or_else(usage)?;
            let id = super::item_arg::tointeger(id);
            if (id as i32) <= 0 {
                return Err(usage());
            }
            let pet = bool_or_default(args.get(1), false);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let slots = match pet {
                true => &model.pet_book.slots,
                false => &model.spellbook.slots,
            };
            Ok(slots
                .iter()
                .rposition(|s| s.spell_id == id)
                .map(|i| i as i64))
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{
        ContainerSlot, ContainerState, PetBookState, SpellBookState, SpellInfoView, SpellSlotView,
        UiScript,
    };

    #[test]
    fn the_static_predicates_read_the_slots_row() {
        let mut s = UiScript::new().unwrap();
        let slot = |spell_id, name: &str| SpellSlotView {
            spell_id,
            name: name.into(),
            ..Default::default()
        };
        let row = |spell_id| SpellInfoView {
            spell_id,
            ..Default::default()
        };
        s.set_spell_catalog([
            SpellInfoView {
                is_attack: true,
                harmful: true,
                has_range: true,
                ..row(6603)
            },
            SpellInfoView {
                helpful: true,
                ..row(2050)
            },
            // Two reagents and a -2 one, which the count skips.
            SpellInfoView {
                reagents: vec![(17020, 2), (17021, 1), (0xffff_fffe, 1)],
                ..row(698)
            },
            SpellInfoView {
                has_range: true,
                ..row(2649)
            },
            row(75),
        ]);
        s.set_spellbook(SpellBookState {
            tabs: Vec::new(),
            slots: vec![
                slot(6603, "Attack"),
                slot(2050, "Heal"),
                slot(698, "Ritual"),
                slot(75, "Auto Shot"),
                slot(2050, "Heal"),
            ],
        });
        s.set_pet_book(PetBookState {
            token: None,
            slots: vec![slot(2649, "Growl")],
        });
        let bag = ContainerState {
            num_slots: 16,
            slots: (1..)
                .zip(
                    [(17020, 7), (17021, 5)].map(|(item_id, count)| ContainerSlot {
                        item_id,
                        count,
                        ..Default::default()
                    }),
                )
                .collect(),
            ..Default::default()
        };
        s.set_container(0, Some(bag));
        // No player: no count, no range.
        s.run("assert(GetSpellCount('Ritual') == 0 and SpellHasRange('Attack') == nil)")
            .unwrap();
        s.model_mut().unit_guids.player = 1;
        s.set_current_spells(Vec::new(), Some(75));
        s.run(
            r#"
            assert(IsAttackSpell('Attack') == 1 and IsAttackSpell(2, 'spell') == nil)
            assert(IsHarmfulSpell('Attack') == 1 and IsHelpfulSpell('Attack') == nil)
            assert(IsHelpfulSpell('Heal') == 1 and IsHarmfulSpell('Heal') == nil)
            assert(IsConsumableSpell('Ritual') == 1 and IsConsumableSpell('Heal') == nil)
            -- floor(7 / 2) and floor(5 / 1): the least is 3; no reagents is 0.
            assert(GetSpellCount('Ritual') == 3 and GetSpellCount('Heal') == 0)
            assert(GetSpellCount(9, 'spell') == 0 and select('#', GetSpellCount('Nope')) == 0)
            assert(SpellHasRange('Attack') == 1 and SpellHasRange('Heal') == nil)
            -- The pet book's range needs a pet.
            assert(SpellHasRange('Growl') == nil)
            assert(IsAutoRepeatSpell('Auto Shot') == 1 and IsAutoRepeatSpell('Attack') == nil)
            -- 0-based, the last slot holding it; the pet book by flag.
            assert(FindSpellBookSlotByID(2050) == 4 and FindSpellBookSlotByID(6603) == 0)
            assert(FindSpellBookSlotByID(2649) == nil and FindSpellBookSlotByID(2649, 1) == 0)
            assert(not pcall(FindSpellBookSlotByID, 0) and not pcall(FindSpellBookSlotByID, 'x'))
        "#,
        )
        .unwrap();
        s.model_mut().unit_guids.pet = 2;
        s.run("assert(SpellHasRange('Growl') == 1)").unwrap();

        // IsSpellInRange asks the provider for the slot's spell, its book and the unit; with
        // none installed every ask is nil.
        s.model_mut().unit_guids.target = 7;
        s.model_mut().unit_guids.focus = 8;
        s.run("assert(IsSpellInRange('Heal') == nil)").unwrap();
        s.set_game_query(Box::new(Fake));
        s.run(
            r#"
            -- No unit argument is the current target; a 0 is a number, so truthy.
            assert(IsSpellInRange('Heal') == 1 and IsSpellInRange('Heal', '') == 1)
            local out = IsSpellInRange('Heal', 'focus')
            assert(out == 0 and type(out) == 'number')
            -- A slot and book put the token at argument 3; the pet book asks for the pet.
            assert(IsSpellInRange(2, 'spell', 'focus') == 0)
            assert(IsSpellInRange(2, 'spell') == 1)
            assert(IsSpellInRange('Growl', 'target') == 0)
            -- Nobody in the slot, a token that does not parse, an empty slot: nil.
            assert(IsSpellInRange('Heal', 'party1') == nil)
            assert(IsSpellInRange('Heal', 'bogus') == nil)
            assert(IsSpellInRange(40, 'spell') == nil)
            -- A name that resolves to no slot answers nothing.
            assert(select('#', IsSpellInRange('Nope')) == 0)
        "#,
        )
        .unwrap();
    }

    /// Unit 7 in range of a player spell, 8 out of it, the pet's the other way round; the spell
    /// must be Heal's or Growl's.
    struct Fake;
    impl crate::script::GameQuery for Fake {
        fn spell_in_range(&self, spell: u32, pet: bool, unit: u64) -> Option<bool> {
            assert!(matches!((spell, pet), (2050, false) | (2649, true)));
            match unit {
                7 => Some(!pet),
                8 => Some(pet),
                _ => None,
            }
        }
    }
}
