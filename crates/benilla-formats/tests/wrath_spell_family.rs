//! Installed build-12340 `Spell.dbc`: the 96-bit `SpellFamilyFlags` and the per-effect
//! `EffectSpellClassMask` (AzerothCore `flag96 SpellFamilyFlags` 209-211,
//! `std::array<flag96, 3> EffectSpellClassMask` 122-130).
use benilla_formats::{load_spell_catalog, Chain};
use std::path::Path;

#[test]
fn installed_wrath_spell_family_mask_is_96_bits() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let cat = load_spell_catalog(&mut chain).unwrap();

    let mut high_word = 0usize;
    let mut example = None;
    let (mut masked, mut single_effect_masked, mut single_effect_spill) = (0usize, 0usize, 0usize);
    let mut word_major_spill = 0usize;
    let mut single_example = None;
    for (id, d) in cat.iter() {
        if d.spell_family_flags_2 != 0 {
            high_word += 1;
            example.get_or_insert((id, d.name.clone(), d.spell_family, d.spell_family_flags_2));
            let bit = 64 + d.spell_family_flags_2.trailing_zeros();
            assert!(d.has_family_bit(bit), "{id}: bit {bit}");
        }
        if d.effect_spell_class_mask.iter().flatten().any(|&w| w != 0) {
            masked += 1;
            // A spell with only effect 0 keeps its mask in effect 0's three words if the file
            // stores flag96 per effect; a word-major layout would put words 1-2 in slots 3 and 6.
            if d.effects[1] == 0 && d.effects[2] == 0 {
                single_effect_masked += 1;
                let raw: Vec<u32> = d
                    .effect_spell_class_mask
                    .iter()
                    .flatten()
                    .copied()
                    .collect();
                let outside = |keep: [usize; 3]| {
                    raw.iter()
                        .enumerate()
                        .any(|(i, &w)| w != 0 && !keep.contains(&i))
                };
                if outside([0, 1, 2]) {
                    single_effect_spill += 1;
                }
                if outside([0, 3, 6]) {
                    word_major_spill += 1;
                }
                if outside([0, 1, 2]) && single_example.is_none() {
                    single_example = Some((id, d.name.clone(), d.effects, raw.clone()));
                }
            }
        }
    }
    eprintln!(
        "FAMILY high_word_spells={high_word} e.g. {example:?} class_masked={masked} \
         single_effect={single_effect_masked} spill={single_effect_spill} \
         word_major_spill={word_major_spill} e.g. {single_example:?}"
    );
    assert!(high_word > 0, "build 12340 sets family bits 64-95");
    assert!(masked > 0, "modifier auras carry class masks");
    // Blizzard leaves stale masks in effect slots whose `Effect` is 0 (Empowered Imp 54278: effect 1
    // word 1), so neither reading is spotless; the per-effect one AzerothCore loads leaves fewer.
    assert!(
        single_effect_spill < word_major_spill,
        "flag96 per effect fits better than word-major: {single_effect_spill} vs {word_major_spill}"
    );

    // Starfall 53197 (druid family 7) sets family bit 72, in the third word only.
    let starfall = cat.get(53197).expect("Starfall 53197");
    assert_eq!(
        (starfall.name.as_str(), starfall.spell_family),
        ("Starfall", 7)
    );
    assert_eq!(starfall.spell_family_flags_2, 1 << 8);
    assert!(starfall.has_family_bit(72));
    assert!(!starfall.has_family_bit(8));

    // The masks name family bits that spells in the same family set: the high word too.
    let families_high: std::collections::HashSet<(u32, u32)> = cat
        .iter()
        .flat_map(|(_, d)| {
            (64..96)
                .filter(|&b| d.has_family_bit(b))
                .map(move |b| (d.spell_family, b))
        })
        .collect();
    let mask_hits_high = cat
        .iter()
        .filter(|(_, d)| {
            d.effect_spell_class_mask.iter().any(|m| {
                (0..32).any(|b| {
                    m[2] >> b & 1 == 1 && families_high.contains(&(d.spell_family, 64 + b))
                })
            })
        })
        .count();
    eprintln!("FAMILY masks_naming_high_bits={mask_hits_high}");
    assert!(
        mask_hits_high > 0,
        "a modifier names a bit-64+ spell of its own family"
    );
}
