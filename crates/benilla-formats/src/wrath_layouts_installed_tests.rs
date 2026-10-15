//! The build-12340 layouts against the installed client: known rows read their known values.

use crate::Chain;

fn chain() -> Option<Chain> {
    let data = crate::wow_data()?;
    Chain::open_wrath(&data, "enUS").ok()
}

#[test]
fn installed_wrath_spells_read_through_the_mapped_layout() {
    let Some(mut chain) = chain() else { return };
    let spells = crate::load_spell_catalog(&mut chain).expect("Spell.dbc");
    let fireball = spells.get(133).expect("Fireball rank 1");
    eprintln!(
        "133: {:?} {:?} icon={:?} school={} family={} mana={} level={} range={}",
        fireball.name,
        fireball.rank,
        fireball.icon,
        fireball.school,
        fireball.spell_family,
        fireball.mana_cost,
        fireball.spell_level,
        fireball.range_index
    );
    assert_eq!(fireball.name, "Fireball");
    assert_eq!(fireball.rank.as_deref(), Some("Rank 1"));
    assert_eq!(fireball.school, 2, "fire");
    assert_eq!(fireball.spell_family, 3, "mage");
    assert_eq!(
        fireball.icon.as_deref(),
        Some("Interface\\Icons\\Spell_Fire_FlameBolt")
    );
    assert_eq!(fireball.effect_base_points[0], 13);
    assert_eq!(fireball.effect_die_sides[0], 9);
    assert_eq!(fireball.effect_base_dice[0], 1);
    let durations = crate::load_spell_durations(&mut chain).unwrap();
    let radii = crate::load_spell_radii(&mut chain).unwrap();
    let context = crate::TokenContext {
        durations: &durations,
        radii: &radii,
        ranges: None,
        skill: &|_| 0,
        lookup: &|id| spells.get(id),
        mods: None,
        unmodified_points: false,
        home_area: &|| None,
        global: &|key| (key == "INT_SPELL_POINTS_SPREAD_TEMPLATE").then(|| "%d to %d".into()),
        printf: &|template, args| {
            let mut text = template.to_owned();
            for arg in args {
                if let crate::TokenNumber::Int(n) = arg {
                    text = text.replacen("%d", &n.to_string(), 1);
                }
            }
            text
        },
    };
    assert_eq!(
        crate::substitute("$s1 damage", fireball, &context),
        "14 to 22 damage"
    );
    let pick_lock = spells.get(1804).expect("Pick Lock");
    assert_eq!(pick_lock.effect_base_points, [4, 0, 0]);
    assert_eq!(pick_lock.effect_die_sides, [1, 0, 0]);
    assert_eq!(pick_lock.effect_base_dice, [1, 0, 0]);
    for skill in [150, 225, 300] {
        assert_eq!(pick_lock.open_lock_skill(skill), Some(skill as i32));
    }
    let heroic = spells.get(78).expect("Heroic Strike rank 1");
    eprintln!(
        "78: {:?} power={} cost={}",
        heroic.name, heroic.power_type, heroic.mana_cost
    );
    assert_eq!(heroic.name, "Heroic Strike");
}

// AzerothCore DBCStructure.h: SkillLine/SkillLineAbility, TalentTab/Talent,
// CreatureFamily and SpellShapeshiftForm; verified against the effective installed rows.
#[test]
fn installed_wrath_skill_and_talent_known_rows() {
    let Some(mut chain) = chain() else { return };
    let skills = crate::load_skill_line_catalog(&mut chain).expect("skill lines");
    let smithing = skills.line(164).expect("Blacksmithing");
    assert_eq!(
        (smithing.name.as_str(), smithing.category_id),
        ("Blacksmithing", 11)
    );
    assert_eq!(
        smithing.icon.as_deref(),
        Some(r"Interface\Icons\Trade_BlackSmithing")
    );
    assert!(smithing
        .description
        .starts_with("Higher smithing skill allows you"));
    let frost = skills.line(6).expect("Frost skill line");
    assert_eq!((frost.name.as_str(), frost.category_id), ("Frost", 7));
    assert_eq!(
        frost.icon.as_deref(),
        Some(r"Interface\Icons\Spell_Frost_FrostBolt02")
    );
    assert!(frost.description.is_empty());
    let apprentice = skills
        .ability(2018)
        .expect("Apprentice Blacksmithing ability");
    assert_eq!(
        (
            apprentice.skill_id,
            apprentice.req_skill_value,
            apprentice.forward_spell_id
        ),
        (164, 1, 3100)
    );
    assert_eq!((apprentice.trivial_low, apprentice.trivial_high), (0, 0));

    let talents = crate::load_talent_catalog(&mut chain).expect("talents");
    let tabs = talents.tabs_for_class(1, 8);
    assert_eq!(tabs.iter().map(|t| t.id).collect::<Vec<_>>(), [81, 41, 61]);
    let fire = tabs[1];
    assert_eq!(
        (fire.name.as_str(), fire.background.as_str()),
        ("Fire", "MageFire")
    );
    assert_eq!((fire.race_mask, fire.class_mask), (2047, 128));
    let impact = talents.talent(41, 27).expect("Impact in Fire tab");
    assert_eq!((impact.row, impact.col), (0, 0));
    assert_eq!(impact.ranks, [11078, 11080, 0, 0, 0]);
    assert_eq!(impact.max_rank(), 2);
    assert_eq!(
        (
            impact.prereq_talent,
            impact.prereq_rank,
            impact.required_spell
        ),
        (0, 0, 0)
    );
    assert!(!impact.exceptional);
}

#[test]
fn installed_wrath_family_and_form_known_rows() {
    let Some(mut chain) = chain() else { return };
    let families = crate::load_creature_families(&mut chain).expect("families");
    assert_eq!(families.len(), 40);
    let wolf = families.get(1).expect("Wolf family");
    assert_eq!(
        (wolf.name.as_str(), wolf.icon.as_str(), wolf.pet_food_mask),
        ("Wolf", r"Interface\Icons\Ability_Hunter_Pet_Wolf", 1)
    );
    assert!((wolf.min_scale - 0.7).abs() < 1e-6);
    assert_eq!((wolf.min_scale_level, wolf.max_scale_level), (1, 60));
    assert_eq!(wolf.max_scale, 1.0);
    let imp = families.get(23).expect("Imp family");
    assert_eq!(
        (imp.name.as_str(), imp.icon.as_str(), imp.pet_food_mask),
        ("Imp", "", 0)
    );
    assert_eq!((imp.min_scale, imp.max_scale), (0.5, 0.5));
    let forms = crate::load_shapeshift_forms(&mut chain).expect("forms");
    assert_eq!(forms.len(), 32);
    let cat = forms.get(&1).expect("Cat Form");
    assert_eq!(
        (
            cat.name.as_str(),
            cat.bonus_bar,
            cat.flags,
            cat.creature_type
        ),
        ("Cat Form", 1, 760, 1)
    );
    assert_eq!(
        cat.attack_icon.as_deref(),
        Some(r"Interface\Icons\Ability_Druid_CatFormAttack")
    );
}

#[test]
fn installed_wrath_human_reaction_to_a_diseased_young_wolf() {
    let Some(mut chain) = chain() else { return };
    let factions = crate::load_faction_catalog(&mut chain).expect("factions");
    let human = factions.race_template(1).expect("human template");
    let wolf = factions.template(32).expect("template 32");
    assert_eq!(
        (
            human.faction,
            human.group_mask,
            human.friend_group_mask,
            human.enemy_group_mask
        ),
        (1, 3, 2, 12)
    );
    assert_eq!(
        (wolf.faction, wolf.group_mask, wolf.enemies),
        (29, 0, [28, 0, 0, 0])
    );
    assert_eq!(wolf.reaction_toward(human), crate::Reaction::Neutral);
    assert_eq!(human.reaction_toward(wolf), crate::Reaction::Neutral);
    // Faction's wider name/description blocks must not read the spillover columns.
    assert_eq!(factions.faction_name(72), Some("Stormwind"));
    assert_eq!(factions.faction_description(72), Some("One of the last bastions of human power, this Alliance capital is ruled by the prodigal king, Varian Wrynn."));
    let stormwind = factions
        .reputation_faction(72)
        .expect("Stormwind reputation");
    assert_eq!((stormwind.rep_index, stormwind.team), (19, 469));
    assert_eq!(stormwind.base_for(1, 1), 4000);
    assert_eq!(stormwind.base_for(2, 1), -42000);
    assert_eq!(stormwind.default_flags_for(1, 1), 17);
}

#[test]
fn installed_wrath_spell_visuals_load() {
    let Some(mut chain) = chain() else { return };
    let cat = crate::load_spell_visual_catalog(&mut chain).expect("SpellVisual/SpellVisualKit");
    eprintln!("spell visual kits: {}", cat.kit_len());
    assert!(cat.kit_len() > 8000);
}

#[test]
fn installed_wrath_particle_emitters_read() {
    let Some(mut chain) = chain() else { return };
    let bytes = chain
        .read_file("World\\Azeroth\\Elwynn\\PassiveDoodads\\Campfire\\ElwynnCampfire.m2")
        .expect("campfire");
    let emitters = crate::parse_m2_particle_emitters(&bytes).expect("emitters");
    for e in &emitters {
        eprintln!(
            "campfire emitter: blend={:?} tex={:?} rate={:?} color0={:?} scale={:?}",
            e.blend,
            e.texture,
            e.timing.constant_rate(),
            e.over_life.color[0],
            e.over_life.scale
        );
    }
    assert_eq!(emitters.len(), 2);
    assert!(emitters
        .iter()
        .all(|e| e.blend == crate::ParticleBlend::Add && e.texture.is_some()));
    assert!(emitters.iter().all(|e| e.timing.peak_rate() > 0.0));
    let kobold =
        crate::parse_m2_particle_emitters(&chain.read_file("Creature\\Kobold\\Kobold.m2").unwrap())
            .expect("kobold");
    assert_eq!(kobold.len(), 1, "the candle");
}

// Wrath UnitHasRelicSlot (0x6113a9) tests Flags bit 0x8, not nonzero Flags.
#[test]
fn installed_wrath_class_flags_family_and_pet_token() {
    let Some(mut chain) = chain() else { return };
    let classes = crate::load_chr_classes(&mut chain).expect("ChrClasses");
    for (id, family, relic) in [
        (1, 4, false),
        (2, 10, true),
        (3, 9, false),
        (4, 8, false),
        (5, 6, false),
        (6, 15, true),
        (7, 11, true),
        (8, 3, false),
        (9, 5, false),
        (11, 7, true),
    ] {
        assert_eq!(classes.spell_family(id), family, "class {id}");
        assert_eq!(classes.has_relic_slot(id), relic, "class {id}");
        assert_eq!(
            classes.pet_name_token(id),
            if id == 9 { "DEMON" } else { "PET" }
        );
    }
    assert_eq!(classes.damage_bonus_stat(3), Some(1));
    assert_eq!(classes.damage_bonus_stat(1), Some(0));
    // Preserve the native enum value, not a guessed Strength fallback.
    assert_eq!(classes.damage_bonus_stat(6), Some(9));
    assert!(!classes.has_relic_slot(0));
}
