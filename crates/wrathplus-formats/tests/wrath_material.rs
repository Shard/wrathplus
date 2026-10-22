//! Effective enUS chain assertions, not stock completeness or live audio parity.
use wrathplus_formats::{load_material_catalog, Chain};

#[test]
fn installed_wrath_material() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
    let raw = chain.read_file("DBFilesClient\\Material.dbc").unwrap();
    let word = |i: usize| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
    assert_eq!((word(4), word(8), word(12)), (8, 5, 20));
    // Native decoder 0x8b8ba0 retains all five serialized words in order.
    // Appended words are checked as raw data, not assigned new semantics.
    let plate = raw[20..180]
        .as_chunks::<20>()
        .0
        .iter()
        .find(|r| u32::from_le_bytes(r[..4].try_into().unwrap()) == 6)
        .unwrap();
    let words: Vec<_> = plate
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect();
    assert_eq!(words, [6, 3, 1004, 696, 701]);

    let c = load_material_catalog(&mut chain).unwrap();
    assert_eq!(c.len(), 8);
    assert!(!c.is_empty());
    // Independently inspected installed rows: first three columns feed the
    // existing flags/foley projection; do not shift these for appended words.
    for (id, foley, metal, armor_slot) in [
        (1, None, true, 0),
        (2, None, false, 0),
        (3, None, false, 0),
        (4, None, true, 0),
        (5, Some(1005), true, 1),
        (6, Some(1004), true, 2),
        (7, None, false, 0),
        (8, Some(1003), false, 0),
    ] {
        assert_eq!(c.foley_kit(id), foley, "foley {id}");
        assert_eq!(c.is_metal(id), metal, "metal {id}");
        assert_eq!(c.armor_impact_slot(id), armor_slot, "armor slot {id}");
    }
    for absent in [0, 9, u32::MAX] {
        assert_eq!(c.foley_kit(absent), None);
        assert!(!c.is_metal(absent));
        assert_eq!(c.armor_impact_slot(absent), 0);
    }
}
