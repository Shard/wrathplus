//! Effective enUS known rows, not stock completeness or native scatter parity.
use benilla_formats::{load_ground_effect_catalog, Chain};

#[test]
fn installed_wrath_ground_effects() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
    let raw = chain
        .read_file("DBFilesClient\\GroundEffectTexture.dbc")
        .unwrap();
    let word = |i: usize| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
    assert_eq!((word(4), word(8), word(12)), (24981, 11, 44));
    // Independently inspected raw rows: four extra words precede density.
    // Decoder 0x8afed0 preserves these spans without reordering. Checking the
    // extra words does not certify their interpretation by the scatter code.
    for expected in [
        [948, 116, 115, 114, 113, 1, 1, 1, 1, 8, 5],
        [760, 13, 0, 0, 0, 1, 1, 1, 1, 6, 2],
        [72982, 796, 795, 798, 797, 1, 2, 3, 1, 8, 5],
        [11939, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0],
    ] {
        let row = raw[20..20 + word(4) as usize * 44]
            .as_chunks::<44>()
            .0
            .iter()
            .find(|r| u32::from_le_bytes(r[..4].try_into().unwrap()) == expected[0])
            .unwrap();
        let actual: Vec<_> = row
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        assert_eq!(actual, expected);
    }

    // Wrath always joins doodad row IDs, regardless of the Vanilla A/B flag.
    for internal_id in [true, false] {
        let catalog = load_ground_effect_catalog(&mut chain, internal_id).unwrap();
        for (id, density, stems) in [
            (
                948,
                8,
                [
                    Some("DunBus01"),
                    Some("DunGra01"),
                    Some("DunGra02"),
                    Some("DunGra03"),
                ],
            ),
            (760, 6, [Some("ElwRoc02"), None, None, None]),
            (
                72982,
                8,
                [
                    Some("rsflo01"),
                    Some("rsgra01"),
                    Some("rsgra02"),
                    Some("rsbra01"),
                ],
            ),
        ] {
            let effect = catalog.effect(id).unwrap();
            assert_eq!(effect.density, density, "density {id}");
            let expected = stems.map(|stem| stem.map(|s| format!("World\\NoDXT\\Detail\\{s}.m2")));
            assert_eq!(effect.doodads, expected, "slots {id}");
        }
        // Empty source row and absent IDs resolve no effect; zero slots do not
        // accidentally join a model. No archive/model availability is asserted.
        for absent in [11939, 0, u32::MAX] {
            assert!(
                catalog.effect(absent).is_none(),
                "unexpected effect {absent}"
            );
        }
    }
}
