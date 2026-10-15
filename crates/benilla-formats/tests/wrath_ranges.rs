//! Known installed SpellRange rows; certifies the exposed first (hostile) pair only.
use benilla_formats::{load_spell_ranges, Chain};

#[test]
fn installed_wrath_range_pairs_and_melee_flag() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").expect("Wrath chain");
    // Independently inspected 64x40/160-byte table. Projection uses columns 1/3/5,
    // not adjacent min/max columns: columns 2/4 are the unexposed friendly pair.
    let ranges = load_spell_ranges(&mut chain).expect("SpellRange");
    assert_eq!(ranges.len(), 64);
    for (id, min, max, flags) in [
        (1, 0.0, 0.0, 0),
        (2, 0.0, 5.0, 1),
        (3, 0.0, 20.0, 0),
        (4, 0.0, 30.0, 0),
        (5, 0.0, 40.0, 0),
        (8, 10.0, 20.0, 0),
        (13, 0.0, 50000.0, 0),
    ] {
        let row = ranges.get(id).expect("known range");
        assert_eq!((row.min, row.max, row.flags), (min, max, flags));
        assert_eq!(row.is_melee(), flags & 1 != 0);
    }
    assert!(ranges.get(u32::MAX).is_none());
}
