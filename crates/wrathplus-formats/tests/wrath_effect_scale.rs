//! Read-only build-12340 rows for the sizes a spell effect model takes: `SpellVisualEffectName`
//! fields 4-6 and `CreatureModelData`'s world and attached effect scales (fields 23/24).
use wrathplus_formats::{Chain, EffectScale};
use std::path::Path;

#[test]
fn installed_wrath_effect_scales() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let visuals = wrathplus_formats::load_spell_visual_catalog(&mut chain).unwrap();
    let row = |scale, max| EffectScale {
        scale,
        min: 0.01,
        max,
    };
    // Field 4 sizes the model; 3,657 rows bound the world size to [0.01, 100], 288 to 10.
    assert_eq!(visuals.effect_scale(1), row(1.0, 100.0)); // SeedOfCorruption_State
    assert_eq!(visuals.effect_scale(95), row(3.0, 100.0)); // Meteor_Ball_Missile
    assert_eq!(visuals.effect_scale(281), row(0.35, 100.0)); // LightningBolt_Impact_Chest
    assert_eq!(
        visuals.effect_path(281),
        Some(r"Spells\LightningBolt_Impact_Chest.mdx")
    );
    assert_eq!(visuals.effect_scale(u32::MAX), EffectScale::default());
    // The paths and engine rows still load off the seven-column layout.
    assert_eq!(
        visuals.loot_art_effect(),
        Some((14, r"Particles\LootFX.mdl"))
    );

    let creatures = wrathplus_formats::load_creature_catalog(&mut chain).unwrap();
    let close = |got: [f32; 2], want: [f32; 2]| {
        assert!(
            got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6),
            "{got:?}"
        );
    };
    close(creatures.effect_scales(170).unwrap(), [0.8, 0.75]); // SeaGiant
    close(creatures.effect_scales(69).unwrap(), [0.07, 0.9]); // ElementalEarth
    close(creatures.effect_scales(328).unwrap(), [1.0, 0.5]); // Rabbit
}
