//! Read-only build-12340 rows for the native `CreatureSoundData` columns and `BarberShopStyle`.
use wrathplus_formats::{barber_kind, Chain};
use std::path::Path;

fn chain() -> Option<Chain> {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return None;
    }
    Some(Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap())
}

/// The fifth fidget (column 18) and the `$BRT`/`$SCD`/`$SMG`/`$SMD` kits (33-36) read where
/// 12340's `0x7462e0` and `0x732650` read them, and every later column stays in place.
#[test]
fn installed_wrath_creature_voice_native_columns() {
    let Some(mut chain) = chain() else { return };
    let voices = wrathplus_formats::load_creature_voice_catalog(&mut chain).unwrap();
    assert_eq!(voices.len(), 1306);
    let gryphon = voices.for_model(147).unwrap(); // Creature\Gryphon\Gryphon.mdx, the taxi
    assert_eq!(gryphon.fidget, [10083, 0, 0, 0, 10889]);
    let wyvern = voices.for_model(85).unwrap();
    assert_eq!(wyvern.fidget, [0, 0, 764, 765, 10891]);
    let well = voices.for_model(2549).unwrap(); // Creature\WellofSouls\WellofSouls.mdx
    assert_eq!(
        (
            well.birth,
            well.spell_cast_directed,
            well.submerge,
            well.submerged
        ),
        (10971, 10972, 10973, 10974)
    );
    let ghoul = voices.for_model(45).unwrap();
    assert_eq!((ghoul.fidget, ghoul.birth), ([407, 408, 0, 0, 0], 12945));
    // Model 1's row: custom attacks at 19-22, nothing shifted into the fidgets.
    let v = voices.for_model(1).unwrap();
    assert_eq!(v.fidget, [0, 0, 571, 572, 0]);
    assert_eq!(v.custom_attack, [0, 7374, 0, 0]);
    // Warlock pets' barks at 28-30.
    let imp = voices.for_model(371).unwrap(); // Creature\Imp\Imp.mdx
    assert_eq!(
        (imp.pet_attack, imp.pet_order, imp.pet_dismiss),
        (9097, 9098, 9096)
    );
    let void = voices.for_model(162).unwrap();
    assert_eq!(
        (void.pet_attack, void.pet_order, void.pet_dismiss),
        (9107, 9108, 9106)
    );
}

#[test]
fn installed_wrath_barber_styles() {
    let Some(mut chain) = chain() else { return };
    let styles = wrathplus_formats::load_barber_styles(&mut chain).unwrap();
    assert_eq!(styles.len(), 491);
    let hair: Vec<_> = styles
        .for_look(barber_kind::HAIR, 1, 0)
        .map(|s| (s.id, s.name.as_str(), s.data))
        .collect();
    assert_eq!(hair.len(), 17, "human male hair styles");
    assert_eq!(hair[0], (63, "Bald", 0));
    assert_eq!(hair[16], (1016, "Slicked", 16));
    assert!(styles
        .for_look(barber_kind::HAIR, 1, 0)
        .all(|s| s.cost_modifier == 1.0 && s.description.is_empty()));
    // Skin rows (type 3) are unnamed and carry the skin index in Data.
    let skins: Vec<_> = styles
        .for_look(barber_kind::SKIN, 6, 0)
        .map(|s| (s.id, s.data))
        .take(3)
        .collect();
    assert_eq!(skins, [(1120, 0), (1121, 1), (1122, 2)]);
    // Draenei male facial hair costs three quarters.
    let beard = styles
        .for_look(barber_kind::FACIAL_HAIR, 11, 0)
        .next()
        .unwrap();
    assert_eq!((beard.id, beard.cost_modifier, beard.data), (19, 0.75, 0));
    let costs = wrathplus_formats::load_barber_cost_base(&mut chain).unwrap();
    assert!(costs.at_level(80).unwrap() > costs.at_level(1).unwrap());
}
