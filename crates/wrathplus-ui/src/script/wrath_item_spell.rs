//! The item verbs over an item's use spell: GetItemSpell (installed ChatFrame.lua:684,844) and
//! the predicates IsHelpfulItem, IsHarmfulItem, IsConsumableItem and ItemHasRange, each the spell
//! routine of its spell verb on that spell's row. Every one resolves its item with the shared
//! resolver ([`super::item_arg`]) and never queries the server.
use super::binding_abi::flag;
use super::{Model, SpellInfoView};
use mlua::{Lua, Value};

/// The use spell, `0x510ef0`'s template leg `0x706b90`: the first nonzero trigger-0 spell; a
/// missing row does not promote a later entry. Labelled: the equipped copy's enchant use spell
/// (`0x5a86b0` → `0x707c60`), tried first by the reference, is not read.
fn use_spell(model: &Model, id: u32) -> Option<&SpellInfoView> {
    let spells = &model.item_templates.get(&id)?.item_spells;
    let &(_, spell_id, _, _) = spells.iter().find(|s| s.0 == 0 && s.1 != 0)?;
    model.spell_catalog.get(&spell_id)
}

/// A `1`/nil predicate of the item's use-spell row: nil with no row (one value).
fn set_use_flag(
    lua: &Lua,
    verb: &str,
    test: fn(&Model, &SpellInfoView) -> bool,
) -> mlua::Result<()> {
    let verb_fn = lua.create_function(move |lua, item: Value| {
        let model = lua.app_data_ref::<Model>().expect("model");
        let id = super::item_arg::item_id_arg(lua, &model, &item);
        Ok(flag(
            use_spell(&model, id).is_some_and(|row| test(&model, row)),
        ))
    })?;
    lua.globals().set(verb, verb_fn)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // IsHelpfulItem (`0x5171f0`) / IsHarmfulItem (`0x5172e0`): the classifier `0x7fe1b0` on the
    // use spell answers 1 / 2 (`0x517288`).
    set_use_flag(lua, "IsHelpfulItem", |_, row| row.helpful)?;
    set_use_flag(lua, "IsHarmfulItem", |_, row| row.harmful)?;
    // ItemHasRange (`0x51c870`): `0x801650` for the player with no target, a nonzero bound
    // (`0x51c93b`); no player is nil. Labelled as SpellHasRange: no range mod.
    set_use_flag(lua, "ItemHasRange", |model, row| {
        model.unit_guids.player != 0 && row.has_range
    })?;
    // IsConsumableItem (`0x5173e0`), template only: ammo or a thrown weapon (InventoryType 24,
    // 25, `0x51745b`), else a trigger-0 spell slot whose row exists (`0x5174a1`) with negative
    // charges or a reagent (`0x5174c8`); a miss is nil.
    lua.globals().set(
        "IsConsumableItem",
        lua.create_function(|lua, item: Value| {
            let model = lua.app_data_ref::<Model>().expect("model");
            let id = super::item_arg::item_id_arg(lua, &model, &item);
            let consumable = model.item_templates.get(&id).is_some_and(|t| {
                matches!(t.inventory_type, 24 | 25)
                    || t.item_spells
                        .iter()
                        .any(|&(trigger, spell_id, _, charges)| {
                            trigger == 0
                                && model
                                    .spell_catalog
                                    .get(&spell_id)
                                    .is_some_and(|row| charges < 0 || !row.reagents.is_empty())
                        })
            });
            Ok(flag(consumable))
        })?,
    )?;
    lua.globals().set(
        "GetItemSpell",
        lua.create_function(|lua, query: Value| {
            let result = {
                let mut model = lua.app_data_mut::<Model>().expect("model");
                // The shared item resolver (`0x51711e`); a negative id names no record to ask for.
                let id = super::item_arg::item_id_arg(lua, &model, &query);
                if (id as i32) > 0 && !model.item_templates.contains_key(&id) {
                    model.item_stat_asks.insert(id);
                }
                let spell = use_spell(&model, id);
                spell.map(|row| (row.name.clone(), row.rank.clone().unwrap_or_default()))
            };
            Ok(match result {
                Some((name, rank)) => (Some(name), Some(rank)),
                None => (None, None),
            })
        })?,
    )
}

#[cfg(test)]
mod tests {
    use super::super::{ItemTemplateView, SpellInfoView, UiScript};
    fn script() -> UiScript {
        let mut s = UiScript::new().unwrap();
        s.set_spell_catalog([
            SpellInfoView {
                spell_id: 10,
                name: "First Use".into(),
                rank: Some("Rank 2".into()),
                ..Default::default()
            },
            SpellInfoView {
                spell_id: 20,
                name: "Later Use".into(),
                ..Default::default()
            },
        ]);
        s.set_item_template(
            100,
            ItemTemplateView {
                name: "Test Trinket".into(),
                item_spells: vec![
                    (1, 20, None, 0),
                    (5, 20, None, 0),
                    (6, 20, None, 0),
                    (0, 0, None, 0),
                    (0, 10, None, 0),
                    (0, 20, None, 0),
                ],
                ..Default::default()
            },
        );
        s
    }
    #[test]
    fn template_order_returns_two_strings_from_catalog() {
        let s = script();
        s.run(r#"for _, q in ipairs({100, '100', 'item:100:0:0', '|cffffffff|Hitem:100:0|h[Test Trinket]|h|r', 'test trinket'}) do
            assert(select('#',GetItemSpell(q))==2)
            local name, rank=GetItemSpell(q); assert(name=='First Use' and rank=='Rank 2')
        end"#).unwrap();
    }
    #[test]
    fn missing_first_row_does_not_promote_and_empty_rank_is_string() {
        let mut s = script();
        s.set_item_template(
            101,
            ItemTemplateView {
                item_spells: vec![
                    (0, 999, Some("Not a catalog row".into()), 0),
                    (0, 20, None, 0),
                ],
                ..Default::default()
            },
        );
        s.set_item_template(
            102,
            ItemTemplateView {
                item_spells: vec![(0, 20, None, 0)],
                ..Default::default()
            },
        );
        s.run("local a,b=GetItemSpell(101); assert(a==nil and b==nil); assert(select('#',GetItemSpell(101))==2); local c,d=GetItemSpell(102); assert(c=='Later Use' and d==''); assert(GetItemSpell(999)==nil)").unwrap();
        // The predicates read the use spell's row: the classifier, the range and, for
        // IsConsumableItem, negative charges or a reagent on a row that exists, or ammo.
        s.set_spell_catalog([
            SpellInfoView {
                spell_id: 20,
                helpful: true,
                has_range: true,
                ..Default::default()
            },
            SpellInfoView {
                spell_id: 30,
                harmful: true,
                reagents: vec![(5, 1)],
                ..Default::default()
            },
        ]);
        for (id, spells, inventory_type) in [
            (103, vec![(0, 30, None, 0)], 0),
            (104, vec![(1, 30, None, 0), (0, 20, None, -1)], 0),
            (105, vec![], 24),
            (106, vec![(0, 999, None, -1)], 0),
        ] {
            let item_spells = spells;
            s.set_item_template(
                id,
                ItemTemplateView {
                    item_spells,
                    inventory_type,
                    ..Default::default()
                },
            );
        }
        s.run(
            r#"
            assert(IsHelpfulItem(102) == 1 and IsHarmfulItem(102) == nil and IsHarmfulItem(103) == 1)
            assert(select('#', IsHelpfulItem(999)) == 1 and IsHelpfulItem(999) == nil)
            assert(ItemHasRange(102) == nil)
            assert(IsConsumableItem(103) == 1 and IsConsumableItem(104) == 1)
            assert(IsConsumableItem(105) == 1 and IsConsumableItem(102) == nil)
            assert(IsConsumableItem(106) == nil and IsConsumableItem(999) == nil)
        "#,
        )
        .unwrap();
        s.model_mut().unit_guids.player = 1;
        s.run("assert(ItemHasRange('item:102') == 1 and ItemHasRange(103) == nil)")
            .unwrap();
    }
}
