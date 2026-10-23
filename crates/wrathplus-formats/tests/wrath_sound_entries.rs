//! Effective enUS chain known rows; not stock completeness or live audio parity.
use wrathplus_formats::{load_sound_kit_catalog, Chain};

#[test]
fn installed_wrath_sound_entries() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
    let raw = chain.read_file("DBFilesClient\\SoundEntries.dbc").unwrap();
    let word = |i: usize| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
    assert_eq!((word(4), word(8), word(12)), (12941, 30, 120));
    // Native decoder 0x8b69c0 preserves these offsets, with strings relocated.
    let row = raw[20..20 + 12941 * 120]
        .as_chunks::<120>()
        .0
        .iter()
        .find(|r| u32::from_le_bytes(r[..4].try_into().unwrap()) == 850)
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(row[96..100].try_into().unwrap()),
        0.5f32.to_bits()
    );
    assert_eq!(u32::from_le_bytes(row[112..116].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(row[116..120].try_into().unwrap()), 0);
    let c = load_sound_kit_catalog(&mut chain).unwrap();
    assert_eq!(c.len(), 12941);
    for (id, name, kind, volume, eax, path) in [
        (
            850,
            "igMainMenuOpen",
            2,
            0.5f32,
            2,
            "Sound\\Interface\\uEscapeScreenOpen.wav",
        ),
        (
            4160,
            "Ghost (DONOTRENAME)",
            50,
            0.5,
            0,
            "Sound\\Ambience\\GhostState.wav",
        ),
        (
            4209,
            "Underwater (DONOTRENAME)",
            50,
            0.69,
            0,
            "Sound\\Ambience\\UndwaterLoop.wav",
        ),
    ] {
        let k = c.get(id).unwrap();
        assert_eq!(k.name, name);
        assert_eq!(k.sound_type, kind);
        assert_eq!(k.volume.to_bits(), volume.to_bits());
        assert_eq!((k.flags, k.eax_def), (0, eax));
        assert_eq!((k.min_distance, k.distance_cutoff), (8.0, 45.0));
        assert_eq!(k.files, vec![(path.to_owned(), 1)]);
        assert_eq!(c.by_name(&name.to_ascii_uppercase()).unwrap().id, id);
    }
    let blizzard = c.get(7).unwrap();
    assert_eq!(blizzard.name, "BlizzardImpactVariations");
    assert_eq!(blizzard.volume.to_bits(), 0.6f32.to_bits());
    assert_eq!(
        blizzard.files,
        ('a'..='f')
            .map(|v| (format!("Sound\\Spells\\BlizzardImpact1{v}.wav"), 1))
            .collect::<Vec<_>>()
    );
    let ash = c.get(8940).unwrap();
    assert_eq!(ash.name, "Ashbringer");
    assert_eq!(
        ash.files,
        (1..=10)
            .map(|i| (
                format!("\\Sound\\Creature\\Ashbringer\\ASH_SPEAK_{i:02}.wav"),
                1
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(c.ghost_bed(), Some(4160));
    assert_eq!(c.underwater_bed(), Some(4209));
    assert!(c.get(u32::MAX).is_none());
    assert!(c.by_name("not an installed sound kit").is_none());
}
