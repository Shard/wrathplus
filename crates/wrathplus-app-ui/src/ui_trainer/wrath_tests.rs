//! Build 12340's trainer list builder (`0x596450`) and `GetTrainerServiceDescription`
//! (`005959d0`), on synthetic catalogs.

use std::collections::HashMap;

use wrathplus_formats::{
    SkillLineCatalog, SpellCatalog, SpellDisplay, SPELL_ATTR_IS_TRADESKILL,
    SPELL_EFFECT_LEARN_SPELL, SPELL_EFFECT_SKILL_STEP,
};

use super::describe::service_description;
use super::{service_group, TRAINER_TYPE_TRADESKILL};
use crate::app_core::state::Spells;

fn labels(key: &str) -> Option<String> {
    Some(format!("<{key}>"))
}

/// `(spell, line, name)` rows as `SkillLineAbility` and `SkillLine` would join them.
fn skill_lines(rows: &[(u32, u32, &str)]) -> SkillLineCatalog {
    let mut cat = SkillLineCatalog::from_spell_lines(rows.iter().map(|&(s, l, _)| (s, l)));
    for &(_, line, name) in rows {
        cat.set_line(
            line,
            wrathplus_formats::SkillLineInfo {
                name: name.into(),
                category_id: 7,
                icon: None,
                description: String::new(),
                can_link: false,
                alternate_verb: String::new(),
            },
        );
    }
    cat
}

fn catalog(rows: Vec<(u32, SpellDisplay)>) -> SpellCatalog {
    SpellCatalog::from_displays(rows.into_iter().collect::<HashMap<_, _>>())
}

/// At a tradeskill trainer a passive spell keys 3 (`Attributes & 0x40`) ahead of the step test,
/// a step 1, anything else 2, and the header is the `[0xa13e08]` key string.
#[test]
fn a_tradeskill_trainer_keys_passive_step_and_learn() {
    let passive = SpellDisplay {
        attributes: 0x40,
        effects: [SPELL_EFFECT_SKILL_STEP, 0, 0],
        ..Default::default()
    };
    let step = SpellDisplay {
        effects: [0, SPELL_EFFECT_SKILL_STEP, 0],
        ..Default::default()
    };
    let spells = catalog(vec![(1, passive), (2, step), (3, SpellDisplay::default())]);
    let group = |id| service_group(id, TRAINER_TYPE_TRADESKILL, &spells, None, &labels);
    assert_eq!(group(1), (3, "<TRADESKILL_SERVICE_PASSIVE>".into()));
    assert_eq!(group(2), (1, "<TRADESKILL_SERVICE_STEP>".into()));
    assert_eq!(group(3), (2, "<TRADESKILL_SERVICE_LEARN>".into()));
    assert_eq!(
        group(99),
        (2, "<TRADESKILL_SERVICE_LEARN>".into()),
        "no record: 2"
    );
}

/// Elsewhere the key is the skill line of the first `LEARN_SPELL` trigger not aimed at the pet
/// (`0x594ae0`), else of the service spell; an unplaced spell keys 0, and no type keys a
/// known-services group (1.12's `-1`).
#[test]
fn other_trainers_key_on_the_first_non_pet_learn_trigger() {
    let mut wrapper = SpellDisplay {
        effects: [SPELL_EFFECT_LEARN_SPELL, SPELL_EFFECT_LEARN_SPELL, 0],
        effect_trigger_spell: [20, 30, 0],
        ..Default::default()
    };
    wrapper.effect_implicit_target_a[0] = 5; // TARGET_UNIT_PET: skipped
    let spells = catalog(vec![(10, wrapper), (40, SpellDisplay::default())]);
    let lines = skill_lines(&[(30, 26, "Arms"), (40, 256, "Fury")]);
    for trainer_type in [0, 1, 3] {
        let group = |id| service_group(id, trainer_type, &spells, Some(&lines), &labels);
        assert_eq!(group(10), (26, "Arms".into()), "type {trainer_type}");
        assert_eq!(
            group(40),
            (256, "Fury".into()),
            "no learn effect: the spell itself"
        );
        assert_eq!(group(77), (0, String::new()), "unplaced: dropped");
    }
}

/// `005959d0`: the service spell's own description, tokens filled; a creating trade spell with
/// none answers its item's description once cached; anything else answers nothing.
#[test]
fn the_description_is_the_spells_own_else_a_trade_items() {
    let deps = crate::items::TestDeps::default();
    let mut items = deps.items;
    let mut item = crate::items::test_template("Copper Bracers");
    item.description = "Sturdy.".into();
    items.insert_template(2853, Some(item));
    let spells = Spells {
        catalog: catalog(vec![
            (
                1,
                SpellDisplay {
                    description: Some("Deals $s1 damage.".into()),
                    effect_base_points: [9, 0, 0],
                    effect_die_sides: [1, 0, 0],
                    ..Default::default()
                },
            ),
            (
                2,
                SpellDisplay {
                    attributes: SPELL_ATTR_IS_TRADESKILL,
                    effects: [24, 0, 0],
                    effect_item_type: [2853, 0, 0],
                    ..Default::default()
                },
            ),
            (
                3,
                SpellDisplay {
                    effects: [24, 0, 0],
                    effect_item_type: [2853, 0, 0],
                    ..Default::default()
                },
            ),
        ]),
        ..Spells::empty_for_tests()
    };
    let lines = skill_lines(&[]);
    let script = wrathplus_ui::script::UiScript::new().unwrap();
    let describe =
        |id| service_description(id, (&spells, &lines, None, &script), &items, &deps.commands);
    assert_eq!(describe(1), "Deals 9 damage.");
    assert_eq!(describe(2), "Sturdy.");
    assert_eq!(describe(3), "", "not a trade spell: no item leg");
    assert_eq!(describe(99), "");
}

/// A green wire row with no gates.
fn wire(spell: u32) -> wrathplus_protocol::types::TrainerSpell {
    wrathplus_protocol::types::TrainerSpell {
        spell,
        state: wrathplus_protocol::types::trainer_spell_state::GREEN,
        cost: 10,
        can_learn_primary_prof: false,
        is_primary_prof_first_rank: false,
        wrath_point_cost: None,
        req_level: 1,
        req_skill: 0,
        req_skill_value: 0,
        req_spells: [0, 0, 0],
    }
}

/// The stock `ClassTrainerFrame` fed this builder's rows off the shipped DBCs: `(name, type)` per
/// row, the description `ClassTrainer_SetSelection` shows for `select`, and the feed's own text.
fn open_on_real_data(
    trainer_type: u32,
    spell_ids: &[u32],
    items: &crate::items::Items,
    select: u32,
) -> (Vec<(String, String)>, String, String) {
    let data = std::path::PathBuf::from(std::env::var("WOW_DATA").unwrap());
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let spells = Spells {
        catalog: wrathplus_formats::load_spell_catalog(&mut chain).expect("Spell.dbc"),
        ..Spells::empty_for_tests()
    };
    let lines = wrathplus_formats::load_skill_line_catalog(&mut chain).expect("SkillLine");
    let mut s = crate::ui_script::trainer_tests::trainer_script();
    let commands = crate::items::TestDeps::default().commands;
    let get = |key: &str| wrathplus_ui::strings::global(s.lua(), key);
    let services: Vec<_> = spell_ids
        .iter()
        .map(|&id| {
            let mut svc = super::resolve_service(
                &wire(id),
                trainer_type,
                &spells.catalog,
                Some(&lines),
                &std::collections::BTreeSet::new(),
                None,
                items,
                &commands,
                &get,
            );
            svc.description =
                service_description(id, (&spells, &lines, None, &s), items, &commands);
            svc
        })
        .collect();
    let chosen = services
        .iter()
        .find(|svc| svc.spell_id == select)
        .expect("selected");
    let (expected, name) = (
        chosen.description.clone(),
        chosen.name.clone().unwrap_or_default(),
    );
    s.set_trainer(Some(wrathplus_ui::script::TrainerState {
        greeting: String::new(),
        trainer_type,
        groups: Vec::new(),
        services,
    }));
    s.fire_event("TRAINER_SHOW", vec![]);
    assert!(s.errors().is_empty(), "{:?}", s.errors());
    let rows: Vec<(String, String)> = s
        .eval::<Vec<String>>(
            "local t = {} for i = 1, GetNumTrainerServices() do \
             local n, _, k = GetTrainerServiceInfo(i) t[i] = (n or '') .. '\\t' .. k end return t",
        )
        .unwrap()
        .into_iter()
        .map(|r| {
            let (n, k) = r.split_once('\t').unwrap_or((r.as_str(), ""));
            (n.to_string(), k.to_string())
        })
        .collect();
    let index = 1 + rows
        .iter()
        .position(|(n, k)| *n == name && k != "header")
        .expect("row");
    s.run(&format!("ClassTrainer_SetSelection({index})"))
        .unwrap();
    let shown = s
        .eval::<Option<String>>("return ClassTrainerSkillDescription:GetText()")
        .unwrap()
        .unwrap_or_default();
    (rows, shown, expected)
}

/// A warrior trainer's rows on the shipped data: each header is its services' skill line, the
/// headers in name order, and the selected row shows the spell's own description with its tokens
/// filled. Skips without client data.
#[test]
fn the_stock_trainer_shows_skill_line_groups_and_filled_descriptions() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let items = crate::items::TestDeps::default().items;
    // Heroic Strike, Battle Shout, Charge, Rend.
    let (rows, shown, expected) = open_on_real_data(0, &[78, 6673, 100, 772], &items, 78);
    let headers: Vec<&str> = rows
        .iter()
        .filter(|(_, k)| k == "header")
        .map(|(n, _)| n.as_str())
        .collect();
    let mut sorted = headers.clone();
    sorted.sort_by_key(|h| h.to_lowercase());
    assert!(headers.len() >= 2, "{rows:?}");
    assert_eq!(headers, sorted, "the headers are name-ordered (`0x5941b0`)");
    assert!(
        !expected.is_empty() && !expected.contains('$'),
        "{expected:?}"
    );
    assert_eq!(
        shown, expected,
        "the stock frame shows `GetTrainerServiceDescription`"
    );
}

/// A Blacksmithing trainer on the shipped data: the step row (2020) heads the list under
/// `TRADESKILL_SERVICE_STEP`, the recipe (2663, Copper Bracers) under `_LEARN`; with no spell
/// description the recipe shows its item's (`005959d0`). Skips without client data.
#[test]
fn the_stock_trainer_groups_a_tradeskill_and_shows_a_recipes_item_description() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut items = crate::items::TestDeps::default().items;
    let mut bracers = crate::items::test_template("Copper Bracers");
    bracers.description = "Sturdy test bracers.".into();
    items.insert_template(2853, Some(bracers));
    let (rows, shown, expected) =
        open_on_real_data(TRAINER_TYPE_TRADESKILL, &[2663, 2020], &items, 2663);
    let headers: Vec<&str> = rows
        .iter()
        .filter(|(_, k)| k == "header")
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(headers.len(), 2, "{rows:?}");
    assert_eq!(
        rows[1].0, "Apprentice Blacksmith",
        "the step group leads: {rows:?}"
    );
    assert_eq!(shown, expected);
    assert!(!shown.is_empty() && !shown.contains('$'), "{shown:?}");
}
