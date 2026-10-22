//! The spell catalog's column pins, checked end to end against the real 5875 `Spell.dbc`.

use super::*;

#[test]
fn real_spell_catalog_reads_rank_and_passive() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_catalog(&mut chain).expect("load Spell/SpellIcon");

    // Fireball's first three ranks; even rank 1 carries the literal "Rank 1".
    assert_eq!(cat.get(133).unwrap().rank.as_deref(), Some("Rank 1"));
    assert_eq!(cat.get(143).unwrap().rank.as_deref(), Some("Rank 2"));
    assert_eq!(cat.get(145).unwrap().rank.as_deref(), Some("Rank 3"));
    assert_eq!(cat.get(168).unwrap().rank.as_deref(), Some("Rank 1")); // Frost Armor
    assert_eq!(cat.get(172).unwrap().rank.as_deref(), Some("Rank 1")); // Corruption
    assert_eq!(cat.get(2136).unwrap().rank.as_deref(), Some("Rank 1")); // Fire Blast

    // None of the above is passive; a weapon skill (One-Handed Swords, 201) is.
    assert!(!cat.get(133).unwrap().passive);
    assert!(
        cat.get(201).unwrap().passive,
        "One-Handed Swords is passive"
    );
}

#[test]
fn real_spell_catalog_gates_the_spellbook() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_catalog(&mut chain).expect("load Spell/SpellIcon");

    // Shown: Fireball's ranks, Frostbolt, Polymorph (0x80 clear, castUI 0).
    for id in [133, 143, 145, 116, 118] {
        let d = cat.get(id).unwrap_or_else(|| panic!("spell {id}"));
        assert!(
            d.in_spellbook(),
            "spell {id} should show ({:#x})",
            d.attributes
        );
        assert_eq!(d.attributes & 0x80, 0, "spell {id} is not DO_NOT_DISPLAY");
        assert_eq!(d.cast_ui, 0, "an ordinary spell reads castUI 0");
    }

    // Hidden: a language, armor and weapon proficiencies, each `0xC0 = PASSIVE | DO_NOT_DISPLAY`.
    for (id, what) in [
        (668u32, "Language: Common"),
        (9078, "Cloth"),
        (9077, "Leather"),
        (196, "One-Handed Axes"),
    ] {
        let d = cat.get(id).unwrap_or_else(|| panic!("spell {id} {what}"));
        assert_eq!(
            d.attributes & 0xC0,
            0xC0,
            "{what} ({id}) is PASSIVE|DO_NOT_DISPLAY"
        );
        assert!(!d.in_spellbook(), "{what} ({id}) is hidden from the book");
    }
}

/// The two `Effect[0]` values the client latches at learn time (`0x4b25e0` into `[0xb700e4]` and
/// `[0xb700e8]`); the cursor's skin leg shows the knife only when one of them is in the book.
#[test]
fn real_spell_catalog_pins_the_skin_latch_effects() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_catalog(&mut chain).expect("load Spell/SpellIcon");

    // Skinning (8613): `0x4b2623: cmp [esi+0xf4], 0x5f`.
    assert_eq!(
        cat.get(8613).unwrap().effects[0],
        crate::SPELL_EFFECT_SKINNING,
        "Skinning carries SPELL_EFFECT_SKINNING (95 == 0x5f)"
    );
    // Remove Insignia (22027), the `[0xb700e8]` half: `0x4b2632: cmp [esi+0xf4], 0x74`.
    assert_eq!(
        cat.get(22027).unwrap().effects[0],
        0x74,
        "Remove Insignia carries SPELL_EFFECT_SKIN_PLAYER_CORPSE (116 == 0x74)"
    );
    // Nothing an ordinary caster starts with does, so a non-skinner's latch stays empty.
    assert_ne!(
        cat.get(133).unwrap().effects[0],
        crate::SPELL_EFFECT_SKINNING
    );
    assert_ne!(
        cat.get(6247).unwrap().effects[0],
        crate::SPELL_EFFECT_SKINNING
    );
}

/// The left side of the client's lock test (`0x5f850f`), with the player's skill as the level
/// term (`0x5ea690`). Its columns (27, 28, 64, 67, 70, 73, 76) are pinned by result, against
/// values known from the game; the below-cap rows are the ones a caster-level term would miss.
#[test]
fn real_spell_catalog_computes_the_lock_skill_an_opener_provides() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_catalog(&mut chain).expect("load Spell/SpellIcon");

    // Pick Lock (1804): `4 + 1 + 5.0×(skill/5 − 1)`, the skill itself.
    assert_eq!(cat.get(1804).unwrap().open_lock_skill(300), Some(300));
    assert_eq!(cat.get(1804).unwrap().open_lock_skill(225), Some(225));
    assert_eq!(cat.get(1804).unwrap().open_lock_skill(150), Some(150));
    // Mining (2575), Herb Gathering (2366): `−1 + 1 + 5.0×(skill/5)`; 1 Mining provides 0.
    assert_eq!(cat.get(2575).unwrap().open_lock_skill(300), Some(300));
    assert_eq!(cat.get(2575).unwrap().open_lock_skill(100), Some(100));
    assert_eq!(cat.get(2575).unwrap().open_lock_skill(1), Some(0));
    assert_eq!(cat.get(2366).unwrap().open_lock_skill(300), Some(300));
    // Seaforium Charges (4056, 4075): a flat 150 and 250; lock 92 asks for Blasting 150.
    assert_eq!(cat.get(4056).unwrap().open_lock_skill(0), Some(150));
    assert_eq!(cat.get(4056).unwrap().open_lock_skill(300), Some(150));
    assert_eq!(cat.get(4075).unwrap().open_lock_skill(0), Some(250));
    // The "Opening"/"Closing" family every character knows is a flat 100, so the Action gate,
    // not the value test, keeps them off a padlocked door.
    for id in [3365, 6233, 6246, 6247, 6477, 6478, 21651, 21652] {
        assert_eq!(
            cat.get(id).unwrap().open_lock_skill(0),
            Some(100),
            "spell {id} is a flat-100 opener"
        );
        assert_eq!(cat.get(id).unwrap().open_lock_skill(300), Some(100));
    }
    assert_eq!(cat.get(133).unwrap().open_lock_skill(300), None);
}

#[test]
fn real_spell_catalog_reads_tooltip_columns() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_catalog(&mut chain).expect("load Spell/SpellIcon");
    let cast_times = load_spell_cast_times(&mut chain).expect("load SpellCastTimes");
    let durations = load_spell_durations(&mut chain).expect("load SpellDuration");

    // Fireball r1 has a real 4 s damage-over-time tail ("over $d"), a 2 s tick in effect slot 1.
    let fireball = cat.get(133).unwrap();
    assert!(
        fireball
            .description
            .as_deref()
            .unwrap()
            .starts_with("Hurls a fiery ball that causes"),
        "Fireball description: {:?}",
        fireball.description
    );
    assert_eq!(fireball.casting_time_index, 16);
    assert_eq!(
        cast_times.get(16).unwrap().base_ms,
        1500,
        "Fireball's real cast time"
    );
    assert_eq!(fireball.duration_index, 35);
    assert_eq!(
        durations.get(35).unwrap().base_ms,
        4000,
        "Fireball's DoT-tail duration, not zero — it genuinely has one"
    );
    assert_eq!(
        fireball.proc_chance, 101,
        "vmangos's always-triggers sentinel"
    );
    assert_eq!(
        (fireball.effect_base_points[0], fireball.effect_die_sides[0]),
        (13, 9),
        "Fireball r1's direct-damage roll: 14-22"
    );
    assert_eq!(
        (fireball.effect_apply_aura[1], fireball.effect_amplitude[1]),
        (3, 2000),
        "Fireball's periodic-damage tail: SPELL_AURA_PERIODIC_DAMAGE ticking every 2s"
    );

    // Frost Armor: 30 minutes, instant, its description naming the chill proc 6136.
    let frost_armor = cat.get(168).unwrap();
    assert!(frost_armor
        .aura_description
        .as_deref()
        .is_some_and(|s| !s.is_empty()));
    assert_eq!(frost_armor.duration_index, 30);
    assert_eq!(durations.get(30).unwrap().base_ms, 1_800_000, "30 minutes");
    assert_eq!(frost_armor.casting_time_index, 1);
    assert_eq!(cast_times.get(1).unwrap().base_ms, 0, "instant");
    assert_eq!(
        cast_times.get(18).unwrap().resolved_ms(60, 1),
        -1_000_000,
        "the hunter shots' row reads signed and stays negative"
    );
    assert_eq!(
        frost_armor.effect_apply_aura[0], 22,
        "SPELL_AURA_MOD_RESISTANCE"
    );
    assert_eq!(
        frost_armor.effect_trigger_spell[1], 6136,
        "the chill proc the description text itself names"
    );

    // Fire Blast has no aura: no aura text and no duration row.
    let fire_blast = cat.get(2136).unwrap();
    assert_eq!(fire_blast.aura_description, None);
    assert_eq!(fire_blast.duration_index, 0);
    assert!(durations.get(0).is_none(), "no row 0 in SpellDuration.dbc");

    // Auto Shot and Feign Death: the signed EffectBasePoints sentinel -1, no fixed roll.
    assert_eq!(cat.get(75).unwrap().effect_base_points[0], -1, "Auto Shot");
    assert_eq!(
        cat.get(5384).unwrap().effect_base_points[0],
        -1,
        "Feign Death"
    );
}

/// The WDBC schema must decode the new float arrays before `f32_at` can pass them to token
/// expansion. An integer schema silently turns Chain Heal's 0.5 into the loader's 0.0 fallback.
#[test]
fn spell_schema_decodes_token_float_columns() {
    let mut raw = Vec::new();
    raw.extend_from_slice(b"WDBC");
    for n in [1u32, SPELL_FIELDS as u32, (SPELL_FIELDS * 4) as u32, 1] {
        raw.extend_from_slice(&n.to_le_bytes());
    }
    let mut row = vec![0u8; SPELL_FIELDS * 4];
    row[COL_DAMAGE_MULTIPLIER_1 * 4..COL_DAMAGE_MULTIPLIER_1 * 4 + 4]
        .copy_from_slice(&0.5f32.to_le_bytes());
    row[COL_EFFECT_POINTS_PER_COMBO_POINT_1 * 4..COL_EFFECT_POINTS_PER_COMBO_POINT_1 * 4 + 4]
        .copy_from_slice(&20.0f32.to_le_bytes());
    raw.extend_from_slice(&row);
    raw.push(0);
    let set = parse(&raw, spell_schema(), "Spell.dbc").expect("synthetic Spell.dbc");
    let spell = set.records().first().unwrap();
    assert_eq!(f32_at(spell, COL_DAMAGE_MULTIPLIER_1), Some(0.5));
    assert_eq!(
        f32_at(spell, COL_EFFECT_POINTS_PER_COMBO_POINT_1),
        Some(20.0)
    );
}

/// TryCast's attack pick (`0x6e4edf`) is built for a spell press alone, so no form, recipe
/// (`SPELL_EFFECT_CREATE_ITEM` 24, `SPELL_EFFECT_ENCHANT_ITEM` 53) or `modalNextSpell` chain may
/// carry predicate `0x6e5200`. No item's use spell does either (vmangos `item_template`).
#[test]
fn real_spell_catalog_keeps_the_attack_pick_to_spell_presses() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_catalog(&mut chain).expect("load Spell/SpellIcon");
    let (mut checked, mut chained) = (0, 0);
    for (id, d) in cat.iter() {
        let recipe = d.effects.iter().any(|&e| e == 24 || e == 53);
        if d.shapeshift_form.is_some() || recipe {
            checked += 1;
            assert!(
                !d.initiates_combat(),
                "{} ({id}) is a form or a recipe of predicate 0x6e5200",
                d.name
            );
        }
        if d.modal_next_spell != 0 {
            chained += 1;
            let next = cat.get(d.modal_next_spell);
            assert!(
                !next.is_some_and(|n| n.initiates_combat()),
                "{} ({id}) chains {} of predicate 0x6e5200",
                d.name,
                d.modal_next_spell
            );
        }
    }
    assert!(
        checked > 0 && chained > 0,
        "the forms, recipes and Auto Shot's chain are read"
    );
}

/// `EffectItemType` (columns 103-105) and `RequiresSpellFocus` (15); values from vmangos.
#[test]
fn real_crafting_columns_read_created_item_and_focus() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_catalog(&mut chain).expect("load Spell/SpellIcon");

    // (recipe spell, created item, focus): Bolt of Linen Cloth, Minor Healing Potion,
    // Copper Axe, Crafted Light Shot, Charred Wolf Meat.
    for (spell, item, focus) in [
        (2963u32, 2996u32, 0u32),
        (2330, 118, 0),
        (2738, 2845, 1), // Blacksmithing needs the Anvil (focus 1)
        (3920, 8067, 0),
        (2538, 2679, 4), // Cooking needs a Cooking Fire (focus 4)
    ] {
        let d = cat.get(spell).expect("recipe in the catalog");
        assert_eq!(
            d.effects[0], SPELL_EFFECT_CREATE_ITEM,
            "spell {spell} creates"
        );
        assert_eq!(d.effect_item_type[0], item, "spell {spell} created item");
        assert_eq!(d.requires_spell_focus, focus, "spell {spell} focus");
    }

    // Crafted Light Shot makes 200 a craft: BasePoints 199 plus DieSides 1.
    let shots = cat.get(3920).expect("Crafted Light Shot");
    assert_eq!(shots.effect_base_points[0], 199);
    assert_eq!(shots.effect_die_sides[0], 1);

    // The openers carry effect 47 and no product: Tailoring 3908, Enchanting 7411.
    for opener in [3908u32, 7411] {
        let d = cat.get(opener).expect("opener in the catalog");
        assert_eq!(d.effects[0], SPELL_EFFECT_TRADE_SKILL, "opener {opener}");
        assert_eq!(
            d.effect_item_type, [0; 3],
            "opener {opener} creates nothing"
        );
    }
}

#[test]
fn the_tooltip_gates_read_effect_and_mask() {
    // `0x52eb15`: the cast and cooldown line is omitted for the passive bit or Effect[0] 47 or 78.
    let plain = SpellDisplay::default();
    assert!(!plain.tooltip_omits_cast_line());
    let attribute_passive = SpellDisplay {
        passive: true,
        ..Default::default()
    };
    assert!(attribute_passive.tooltip_omits_cast_line());
    // 6603 "Attack"'s shape: Effect[0] 78, attributes 0x10, not the passive bit.
    let auto_attack = SpellDisplay {
        effects: [78, 0, 0],
        attributes: 0x10,
        ..Default::default()
    };
    assert!(!auto_attack.passive, "the attribute bit is clear");
    assert!(auto_attack.tooltip_omits_cast_line(), "the Effect[0] leg");
    let trade_skill = SpellDisplay {
        effects: [47, 0, 0],
        ..Default::default()
    };
    assert!(trade_skill.tooltip_omits_cast_line());
}

/// [`SpellDisplay::is_harmful`], the client's `0x6ea280 == 2`: an enemy implicit target in slot A
/// (6), or only in slot B as Frost Nova's (A 22, the caster's spot; B 15, a source-area enemy).
#[test]
fn real_is_harmful_pins() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let spells = load_spell_catalog(&mut chain).expect("load Spell/SpellIcon");
    let row = |id: u32| {
        spells
            .get(id)
            .unwrap_or_else(|| panic!("spell {id} in the catalog"))
    };
    for (id, name) in [
        (133u32, "Fireball"),
        (100, "Charge"),
        (7386, "Sunder Armor"),
        (122, "Frost Nova"),
    ] {
        assert!(row(id).is_harmful(), "{name} ({id}) targets enemies");
    }
    assert_eq!(
        (
            row(122).effect_implicit_target_a[0],
            row(122).effect_implicit_target_b[0]
        ),
        (22, 15),
        "Frost Nova is harmful through its B slot, the A slot being the caster's own spot"
    );
    for (id, name) in [
        (139u32, "Renew"),
        (5185, "Healing Touch"),
        (1459, "Arcane Intellect"),
        (6673, "Battle Shout"),
    ] {
        assert!(
            !row(id).is_harmful(),
            "{name} ({id}) does not target enemies"
        );
    }
}

/// The mask rule alone: no mask admits everything, a masked spell refuses a unit of type 0, and
/// the bit is `type - 1`, wrapping at 32 like the hardware's shift.
#[test]
fn creature_type_mask_rule() {
    let masked = |mask| SpellDisplay {
        target_creature_type: mask,
        ..Default::default()
    };
    let open = masked(0);
    for t in [0u32, 1, 7, 32, 33, u32::MAX] {
        assert!(open.admits_creature_type(t), "no mask admits type {t}");
    }
    let beast = masked(1);
    assert!(beast.admits_creature_type(1));
    assert!(
        !beast.admits_creature_type(0),
        "type 0 is refused under a mask"
    );
    assert!(!beast.admits_creature_type(2));
    let top = masked(1 << 31);
    assert!(top.admits_creature_type(32), "type 32 is bit 31");
    assert!(
        !top.admits_creature_type(1),
        "type 1 is bit 0, not the wrapped bit 32"
    );
    assert!(masked(1).admits_creature_type(33), "bit 32 wraps to bit 0");
}
