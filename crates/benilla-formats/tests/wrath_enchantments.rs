//! Selected build-12340 enchant names, visuals and row flags from installed data.
use benilla_formats::{load_enchant_catalog, Chain};

#[test]
fn installed_wrath_enchantment_names_visuals_and_flags() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").expect("Wrath chain");
    // Raw SpellItemEnchantment rows have 38 fields: name starts at 14,
    // ItemVisual is 31 and flags 32 (not the preceding locale flags).
    let enchants = load_enchant_catalog(&mut chain).expect("SpellItemEnchantment");
    for (id, name, visual, binds) in [
        (1, "Rockbiter 3", 61, true),
        (29, "Rockbiter 1", 61, true),
        (803, "Fiery Weapon", 25, false),
        (1897, "+5 Weapon Damage", 2, false),
        (1900, "Crusader", 103, false),
        (2563, "+15 Strength", 125, false),
        (2673, "Mongoose", 155, false),
        (3789, "Berserking", 178, false),
    ] {
        assert!(enchants.has_row(id));
        assert_eq!(enchants.name(id), Some(name));
        assert_eq!(enchants.visual(id), Some(visual));
        assert_eq!(enchants.binds_the_item(id), binds);
        assert!(!enchants.tooltip_hides_name(id));
    }
    assert!(!enchants.has_row(u32::MAX));
    assert_eq!(enchants.name(u32::MAX), None);
    assert_eq!(enchants.visual(u32::MAX), None);
    assert!(!enchants.binds_the_item(u32::MAX));
    assert!(!enchants.tooltip_hides_name(u32::MAX));
}
