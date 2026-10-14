//! Read-only build-12340 known rows for the remaining mapped catalogs.
use benilla_formats::Chain;
use std::path::Path;

#[test]
fn installed_wrath_remaining_mapped_catalogs() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    // WoWDBDefs AreaPOI: nine icon states, position at 12, localized name at 18.
    let poi = benilla_formats::load_area_poi_catalog(&mut chain).unwrap();
    assert_eq!(poi.len(), 738);
    let p = poi.get(792).unwrap();
    assert_eq!(p.name, "Aerie Peak");
    assert_eq!((p.icon, p.faction_id, p.area_id, p.flags), (5, 0, 47, 525));
    for (actual, expected) in p.pos.into_iter().zip([234.85, -2127.76, 118.095]) {
        assert!((actual - expected).abs() < 0.001);
    }
    // CreatureModelData SoundID remains column 13; fifth fidget shifts custom attacks.
    let voices = benilla_formats::load_creature_voice_catalog(&mut chain).unwrap();
    assert_eq!(voices.len(), 1306);
    let v = voices.for_model(1).unwrap();
    assert_eq!(v.death, 569);
    assert_eq!(v.fidget, [0, 0, 571, 572]);
    assert_eq!(v.custom_attack, [0, 7374, 0, 0]);
    let pets = benilla_formats::load_pet_personalities(&mut chain).unwrap();
    assert_eq!(pets.len(), 2);
    let p = pets.for_pet(Some(1)).unwrap();
    assert_eq!(p.thresholds, [0, 333000, 666000]);
    assert_eq!(p.damage, [0.75, 1.0, 1.25]);
    assert_eq!(p.loyalty_rate, [0.0; 3], "loyalty removed in Wrath");
}

#[test]
fn installed_wrath_locale_categories_and_english_profanity() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let cats = benilla_formats::load_realm_categories(&mut chain, 1).unwrap();
    let other = benilla_formats::load_realm_categories(&mut chain, 0).unwrap();
    assert_eq!(cats, other, "Region irrelevant in Wrath");
    assert_eq!(cats.len(), 36);
    assert!(!cats.iter().any(|c| c.id == 12), "non-English locale mask");
    assert_eq!((cats[1].id, cats[1].name.as_str()), (2, "United States"));
    assert_eq!((cats[2].id, cats[2].name.as_str()), (3, "Oceanic"));
    let patterns = benilla_formats::load_chat_profanity(&mut chain).unwrap();
    let bytes = chain.read_file("DBFilesClient\\ChatProfanity.dbc").unwrap();
    let u = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let mut english = Vec::new();
    let mut korean = Vec::new();
    for i in 0..u(4) as usize {
        let at = 20 + i * 12;
        match u(at + 8) {
            0 | u32::MAX => english.push(u(at)),
            1 => korean.push(u(at)),
            _ => {}
        }
    }
    assert_eq!(patterns.len(), 305);
    assert_eq!(patterns.iter().map(|p| p.id).collect::<Vec<_>>(), english);
    assert!(
        patterns.iter().all(|p| !korean.contains(&p.id)),
        "Korean excluded"
    );
    assert_eq!(
        patterns[0].pattern, r"\<fagg[aeiouy]t",
        "known first English pattern"
    );
    assert!(patterns.iter().all(|p| p.pattern != "씨 퐁 자 지"));
    assert!(
        patterns.iter().any(|p| p.pattern == "transexual"),
        "known all-languages pattern"
    );
}
