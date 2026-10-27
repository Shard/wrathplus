//! The corpse gather trades: the learn-time latches (`0x542030`), one spell per trade keyed by its
//! `EffectMiscValue[0]` so a second gather profession never replaces the first, and the tooltip's
//! gather line (`0x620fc3`).

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::app_core::state::{LearnedAbilities, PlayerActions, Spells};

fn gather_spell(misc: i32) -> wrathplus_formats::SpellDisplay {
    let mut d = wrathplus_formats::SpellDisplay::default();
    d.effects[0] = wrathplus_formats::SPELL_EFFECT_SKINNING;
    d.effect_misc_value[0] = misc;
    d
}

fn latched(
    book: &[u32],
    displays: HashMap<u32, wrathplus_formats::SpellDisplay>,
) -> [Option<u32>; 4] {
    let mut world = World::new();
    world.insert_resource(Spells {
        catalog: wrathplus_formats::SpellCatalog::from_displays(displays),
        ..Spells::empty_for_tests()
    });
    world.insert_resource(PlayerActions {
        spells: book.iter().copied().collect(),
        ..Default::default()
    });
    world.init_resource::<LearnedAbilities>();
    world
        .run_system_once(super::track_learned_abilities)
        .unwrap();
    world.resource::<LearnedAbilities>().gather
}

/// Session 162807's book: Mining's corpse spell alone fills the mining slot, never Skinning's.
#[test]
fn each_gather_trade_latches_its_own_spell() {
    let displays = || {
        HashMap::from([
            (8613, gather_spell(0)),
            (32605, gather_spell(1)),
            (32606, gather_spell(2)),
            (49383, gather_spell(3)),
        ])
    };
    assert_eq!(
        latched(&[32606], displays()),
        [None, None, Some(32606), None],
        "a miner knows no Skinning"
    );
    assert_eq!(
        latched(&[8613, 32605, 32606, 49383], displays()),
        [Some(8613), Some(32605), Some(32606), Some(49383)]
    );
}

/// The real records: Skinning, Herb Gathering and Mining's corpse spells carry trades 0, 1, 2.
#[test]
fn the_gather_spells_name_their_trades_on_real_data() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let catalog = wrathplus_formats::load_spell_catalog(&mut chain).expect("Spell.dbc");
    for (spell, kind) in [(8613, 0), (32605, 1), (32606, 2)] {
        let d = catalog.get(spell).expect("the spell is in Spell.dbc");
        assert_eq!(
            d.effects[0],
            wrathplus_formats::SPELL_EFFECT_SKINNING,
            "{spell}"
        );
        assert_eq!(d.effect_misc_value[0], kind, "{spell}");
    }
}

/// The tooltip's gather line follows the corpse's trade (`0x620fc3` on `0x715e50`): a
/// `MINING_LOOT` creature's line is `UNIT_SKINNABLE_ROCK`, and an uncached one stays Skinnable.
#[test]
fn the_tooltip_key_follows_the_creatures_trade() {
    use wrathplus_protocol::types::Field;
    const ENTRY: u32 = 700;
    let fields = |flags: u32| {
        crate::app_core::session::ObjectStore(wrathplus_protocol::ObjectFields::from_pairs(&[
            (Field::ObjectType.index(), 0x09),
            (Field::ObjectEntry.index(), ENTRY),
            (Field::UnitFlags.index(), flags),
        ]))
    };
    let skinnable = fields(0x0400_0000);
    let mut names = crate::names::NameCache::default();
    assert_eq!(super::gather_line_key(&fields(0), Some(&names)), None);
    assert_eq!(
        super::gather_line_key(&skinnable, Some(&names)),
        Some(("UNIT_SKINNABLE_LEATHER", 4)),
        "no record yet"
    );
    names.insert_creature(
        ENTRY,
        Some(crate::names::CreatureRecord {
            name: "Rock Elemental".into(),
            subname: None,
            creature_type: 4,
            pet_family: 0,
            rank: 0,
            type_flags: 0x200,
            civilian: false,
            racial_leader: false,
            display_id: 0,
            modifiers: None,
        }),
    );
    assert_eq!(
        super::gather_line_key(&skinnable, Some(&names)),
        Some(("UNIT_SKINNABLE_ROCK", 4))
    );
}

/// The tooltip draws the key's text, red; with the strings missing it reads Skinnable.
#[test]
fn the_gather_line_names_the_corpses_trade() {
    let mut s = wrathplus_ui::script::UiScript::new().unwrap();
    s.set_screen_size(800.0, 600.0);
    s.run(r#"UNIT_SKINNABLE_ROCK = "Requires Mining"; UNIT_SKINNABLE_HERB = "Requires Herbalism""#)
        .unwrap();
    for (key, text) in [
        ("UNIT_SKINNABLE_ROCK", "Requires Mining"),
        ("UNIT_SKINNABLE_HERB", "Requires Herbalism"),
        ("UNIT_SKINNABLE_BOLTS", "Skinnable"),
    ] {
        s.set_unit(
            "target",
            Some(wrathplus_ui::script::UnitState {
                exists: true,
                guid: 0xF130_0000_4500_0001,
                name: Some("Corpse".into()),
                skinnable: Some((key, 4)),
                ..Default::default()
            }),
        );
        s.run(&format!(
            r#"
            local a = UF1 or CreateFrame("Button", "UF1"); a:SetPoint("CENTER", 0, 0)
            local tt = TT or CreateFrame("GameTooltip", "TT")
            tt:SetOwner(a, "ANCHOR_RIGHT")
            tt:SetUnit("target")
            local last = TTTextLeft1
            for i = 2, tt:NumLines() do last = _G["TTTextLeft" .. i] end
            assert(last:GetText() == "{text}", "{key}: " .. tostring(last:GetText()))
        "#
        ))
        .unwrap();
    }
}

/// `0x620ee0`'s colour leg over `0x61a1f0`: no trade spell or no skill reads red; otherwise the
/// level's required skill (1, then `10×level − 100`, then `5×level`) against the player's skill
/// in the spell's line.
#[test]
fn the_gather_line_shades_by_the_players_skill() {
    use wrathplus_protocol::types::Field;
    const SKINNING_LINE: u32 = 393;
    const SKINNING: u32 = 8613;
    let boar = |level: u32| {
        crate::app_core::session::ObjectStore(wrathplus_protocol::ObjectFields::from_pairs(&[
            (Field::ObjectType.index(), 0x09),
            (Field::UnitLevel.index(), level),
            (Field::UnitFlags.index(), 0x0400_0000),
        ]))
    };
    let skill_lines =
        wrathplus_formats::SkillLineCatalog::from_spell_lines([(SKINNING, SKINNING_LINE)]);
    // The first `PLAYER_SKILL_INFO` slot: the line, then value | max << 16.
    let me = |value: u32| {
        let base = Field::PlayerSkillInfo11.index();
        crate::app_core::session::ObjectStore(wrathplus_protocol::ObjectFields::from_pairs(&[
            (base, SKINNING_LINE),
            (base + 1, value | 450 << 16),
        ]))
    };
    let skinner = crate::app_core::state::LearnedAbilities {
        gather: [Some(SKINNING), None, None, None],
        ..Default::default()
    };
    let shade = |level: u32, value: u32, learned: &crate::app_core::state::LearnedAbilities| {
        super::gather_difficulty(
            &boar(level),
            None,
            Some(&me(value)),
            learned,
            Some(&skill_lines),
        )
    };
    let miner = crate::app_core::state::LearnedAbilities {
        gather: [None, None, Some(32606), None],
        ..Default::default()
    };
    assert_eq!(shade(5, 300, &miner), 4, "no Skinning: red");
    assert_eq!(shade(5, 0, &skinner), 4, "no skill in the line: red");
    // Level 5 needs 1: 101 grey, 51 green, 26 yellow, 1 orange.
    assert_eq!(
        [101, 100, 51, 26, 1].map(|v| shade(5, v, &skinner)),
        [0, 1, 1, 2, 3]
    );
    // Level 15 needs 50, level 30 needs 150.
    assert_eq!(shade(15, 49, &skinner), 4);
    assert_eq!(shade(15, 50, &skinner), 3);
    assert_eq!(shade(30, 149, &skinner), 4);
    assert_eq!(shade(30, 250, &skinner), 0);
}
