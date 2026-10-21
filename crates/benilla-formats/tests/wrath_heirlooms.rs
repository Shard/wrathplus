//! Read-only build-12340 rows for heirloom scaling: `ScalingStatDistribution` and
//! `ScalingStatValues`, through the client's level and mask selection (`0x61e740`, `0x61f550`).
use benilla_formats::heirlooms::{load_heirloom_catalog, HeirloomCatalog};
use benilla_formats::Chain;
use std::path::Path;

#[test]
fn installed_wrath_heirloom_scaling() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let cat = load_heirloom_catalog(&mut chain).unwrap();
    // Row i is level i + 1 (the client indexes by position; ids are not levels: level 80 is 200).
    for level in 1..=80 {
        assert_eq!(cat.values_at(level).unwrap()[1], level, "level column");
    }
    assert_eq!(cat.values_at(80).unwrap()[0], 200);
    assert!(cat.values_at(81).is_none());

    let d = cat.distribution(1).unwrap();
    assert_eq!(d.stats, [(4, 5259), (7, 7888), (32, 5259)]);
    assert_eq!(d.max_level, 80);
    assert_eq!(HeirloomCatalog::scaling_level(d, 85, 1), 80);

    // Shoulder budget (0x1), cloth-shoulder armor (0x20) and spell power at 80 and 60.
    let s = cat.scale(1, 0x1 | 0x20 | 0x8000, 80).unwrap();
    assert_eq!(s.stats, [(4, 51), (7, 76), (32, 51)]);
    assert_eq!((s.armor, s.spell_power), (Some(210), Some(343)));
    let s = cat.scale(1, 0x1, 60).unwrap();
    assert_eq!(s.stats, [(4, 19), (7, 29), (32, 19)]);
    // Melee DPS (0x200): 120 at 80, ± 30%.
    let (lo, hi) = cat.scale(2, 0x200, 80).unwrap().dps.unwrap();
    assert!((lo - 84.0).abs() < 1e-3 && (hi - 156.0).abs() < 1e-3);
}
