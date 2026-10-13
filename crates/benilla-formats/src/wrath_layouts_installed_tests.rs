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
        fireball.name, fireball.rank, fireball.icon, fireball.school, fireball.spell_family,
        fireball.mana_cost, fireball.spell_level, fireball.range_index
    );
    assert_eq!(fireball.name, "Fireball");
    assert_eq!(fireball.rank.as_deref(), Some("Rank 1"));
    assert_eq!(fireball.school, 2, "fire");
    assert_eq!(fireball.spell_family, 3, "mage");
    assert_eq!(fireball.icon.as_deref(), Some("Interface\\Icons\\Spell_Fire_FlameBolt"));
    let heroic = spells.get(78).expect("Heroic Strike rank 1");
    eprintln!("78: {:?} power={} cost={}", heroic.name, heroic.power_type, heroic.mana_cost);
    assert_eq!(heroic.name, "Heroic Strike");
}

#[test]
fn installed_wrath_catalogs_load() {
    let Some(mut chain) = chain() else { return };
    let skills = crate::load_skill_line_catalog(&mut chain);
    eprintln!("skill lines: {:?}", skills.as_ref().err());
    let talents = crate::load_talent_catalog(&mut chain);
    eprintln!("talents: {:?}", talents.as_ref().err());
    let classes = crate::load_chr_classes(&mut chain);
    eprintln!("classes: {:?}", classes.as_ref().err());
    let families = crate::load_creature_families(&mut chain);
    eprintln!("families: {:?}", families.as_ref().err());
    let forms = crate::load_shapeshift_forms(&mut chain).expect("forms");
    eprintln!("form 1: {:?}", forms.get(&1));
    assert!(skills.is_ok() && talents.is_ok() && classes.is_ok() && families.is_ok());
}
