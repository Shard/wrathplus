//! Effective enUS chain known rows, not stock completeness or live roll certification.
use benilla_formats::{item_limit_category::load_item_limit_categories, Chain};
#[test]
fn installed_wrath_item_limit_categories() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
    let raw = chain
        .read_file("DBFilesClient\\ItemLimitCategory.dbc")
        .unwrap();
    let word = |i: usize| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
    assert_eq!((word(4), word(8), word(12)), (83, 20, 80));
    assert_eq!(
        (
            word(20),
            word(20 + 17 * 4),
            word(20 + 18 * 4),
            word(20 + 19 * 4)
        ),
        (2, 16712190, 3, 1)
    );
    let c = load_item_limit_categories(&mut chain).unwrap();
    assert_eq!(c.len(), 83);
    for (id, name, quantity, flags) in [
        (2, "Jeweler's Gems", 3, 1),
        (3, "Healthstone", 1, 0),
        (4, "Mana Gem", 1, 0),
        (6, "Stormjewel", 1, 1),
    ] {
        let r = c.get(id).unwrap();
        assert_eq!((&*r.name, r.quantity(), r.flags()), (name, quantity, flags));
        assert_eq!(r.raw_columns()[17], 16712190);
    }
    assert!(c.get(0).is_none());
    assert!(c.get(u32::MAX).is_none());
}
