use super::*;

/// A stand-in string table, not the shipped wording: the tests check which key is reached and
/// what fills it.
fn table(key: &str) -> Option<String> {
    Some(
        match key {
            "ITEM_ENCHANT_TIME_LEFT_DAYS" => "<%s :: %d D>",
            "ITEM_ENCHANT_TIME_LEFT_DAYS_P1" => "<%s :: %d DD>",
            "ITEM_ENCHANT_TIME_LEFT_HOURS" => "<%s :: %d H>",
            "ITEM_ENCHANT_TIME_LEFT_HOURS_P1" => "<%s :: %d HH>",
            "ITEM_ENCHANT_TIME_LEFT_MIN" => "<%s :: %d M>",
            "ITEM_ENCHANT_TIME_LEFT_SEC" => "<%s :: %d S>",
            "ITEM_SPELL_CHARGES" => "<%d chg>",
            "ITEM_SPELL_CHARGES_P1" => "<%d chgs>",
            _ => return None,
        }
        .to_string(),
    )
}

/// The `0x52fa50` ladder: minutes and seconds have no `_P1` twin, so each reaches one key.
#[test]
fn enchant_countdown_buckets_and_rounding() {
    let t = |ms| enchant_time_left("Rockbiter", ms, &table);
    // Seconds truncate: 1900 ms is 1, not 2.
    assert_eq!(t(1_900).as_deref(), Some("<Rockbiter :: 1 S>"));
    assert_eq!(t(59_999).as_deref(), Some("<Rockbiter :: 59 S>"));
    // Minutes ceil: one second past 4 minutes already reads 5.
    assert_eq!(t(60_000).as_deref(), Some("<Rockbiter :: 1 M>"));
    assert_eq!(t(241_000).as_deref(), Some("<Rockbiter :: 5 M>"));
    // Hours ceil, and only the non-singular count takes the `_P1` key.
    assert_eq!(t(3_600_000).as_deref(), Some("<Rockbiter :: 1 H>"));
    assert_eq!(t(3_600_001).as_deref(), Some("<Rockbiter :: 2 HH>"));
    // Days ceil.
    assert_eq!(t(86_400_000).as_deref(), Some("<Rockbiter :: 1 D>"));
    assert_eq!(t(86_400_001).as_deref(), Some("<Rockbiter :: 2 DD>"));
    // The app drops an expired timer, but zero must not panic; it takes the plural arm,
    // `GetPluralIndex`'s rule.
    assert_eq!(t(0).as_deref(), Some("<Rockbiter :: 0 S>"));
}

#[test]
fn a_missing_countdown_family_draws_nothing() {
    assert_eq!(enchant_time_left("Rockbiter", 60_000, &|_| None), None);
    assert_eq!(charges_phrase(3, &|_| None), None);
}

#[test]
fn charges_phrase_picks_the_plural() {
    assert_eq!(charges_phrase(1, &table).as_deref(), Some("<1 chg>"));
    assert_eq!(charges_phrase(5, &table).as_deref(), Some("<5 chgs>"));
    assert_eq!(charges_phrase(0, &table), None);
}
