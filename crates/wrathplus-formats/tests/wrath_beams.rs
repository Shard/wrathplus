//! Installed build-12340 packed beam rows and SpellVisualKit references.
use wrathplus_formats::{load_spell_visual_catalog, Chain};
use std::path::Path;

#[test]
fn installed_wrath_beams_resolve() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let spells = wrathplus_formats::load_spell_catalog(&mut chain).unwrap();
    let cat = load_spell_visual_catalog(&mut chain).unwrap();
    assert_eq!(cat.chain_effect_len(), 935);
    let lightning = cat.chain_effect(1).unwrap();
    assert_eq!(
        (
            lightning.avg_seg_len,
            lightning.half_width,
            lightning.noise_scale,
            lightning.scroll_period_s,
            lightning.bolt_life_ms,
            lightning.bolt_stagger_ms
        ),
        (2.78, 0.5, 0.04, 1.0, 1000, 300)
    );
    assert_eq!(
        lightning.texture,
        r"Textures\SpellChainEffects\Lightning.blp"
    );
    assert_eq!(
        cat.chain_effect(8).unwrap().scroll_period_s,
        -2.0,
        "native row differs from Vanilla -0.5; do not fabricate a fallback"
    );
    let bytes = chain
        .read_file(r"DBFilesClient\SpellChainEffects.dbc")
        .unwrap();
    let u = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!((u(8), u(12)), (48, 177));
    for i in 0..u(4) as usize {
        let at = 20 + i * 177;
        assert_eq!(cat.chain_effect_raw(u(at)).unwrap(), &bytes[at..at + 177]);
    }
    // Resolve from the actual native spells, not Vanilla's Drain Life visual 177.
    for (spell_id, name, visual, kit_id, effect, channel, texture) in [
        (
            421,
            "Chain Lightning",
            36,
            321,
            115,
            false,
            "ManaBeam_Inverted.blp",
        ),
        (689, "Drain Life", 12655, 11762, 103, true, "HealBeam.blp"),
    ] {
        let spell = spells.get(spell_id).unwrap();
        assert_eq!((spell.name.as_str(), spell.visual), (name, visual));
        let stages = cat.stages(spell.visual).unwrap();
        let stage_kit = if channel { stages.channel } else { stages.cast };
        assert_eq!(stage_kit, kit_id);
        let proc = cat.kit(stage_kit).unwrap().chain_proc().unwrap();
        assert_eq!(
            (proc.effect_id, proc.beams, proc.flag),
            (effect, 1, channel)
        );
        let beam = cat.chain_effect(proc.effect_id).unwrap();
        assert_eq!(
            beam.texture,
            format!("Textures\\SpellChainEffects\\{texture}")
        );
        assert!(beam.avg_seg_len > 0.0 && beam.half_width > 0.0);
    }
}
