use super::*;
fn dbc(rows: &[[u32; 20]], strings: &[u8]) -> Vec<u8> {
    let mut b = b"WDBC".to_vec();
    for word in [rows.len() as u32, 20, 80, strings.len() as u32] {
        b.extend(word.to_le_bytes());
    }
    for row in rows {
        for word in row {
            b.extend(word.to_le_bytes());
        }
    }
    b.extend(strings);
    b
}
#[test]
fn lossless_selected_locale_and_native_last_duplicate_lookup() {
    let mut a = [0; 20];
    a[0] = 7;
    a[1] = 1;
    a[3] = 5;
    a[17] = 0xabcdef;
    a[18] = u32::MAX;
    a[19] = 0x80000001;
    let mut b = a;
    b[18] = 0;
    let c = parse_item_limit_categories(&dbc(&[a, b], b"\0One\0Deux\0"), "frFR").unwrap();
    assert_eq!(c.len(), 2);
    assert!(!c.is_empty());
    assert_eq!(c.rows()[0].raw_columns(), &a);
    assert_eq!(
        (
            &*c.rows()[0].name,
            c.rows()[0].quantity(),
            c.rows()[0].flags()
        ),
        ("Deux", u32::MAX, 0x80000001)
    );
    assert_eq!(c.get(7).unwrap().quantity(), 0);
    assert!(c.get(8).is_none());
    let c = parse_item_limit_categories(&dbc(&[a], b"\0One\0Deux\0"), "deDE").unwrap();
    assert_eq!(c.get(7).unwrap().name, "");
}
#[test]
fn rejects_shape_size_and_locale_errors() {
    let good = dbc(&[[0; 20]], b"\0");
    for index in [0, 8, 12, 16] {
        let mut b = good.clone();
        b[index] ^= 1;
        assert!(parse_item_limit_categories(&b, "enUS").is_err());
    }
    for len in [0, 19, 20, good.len() - 1] {
        assert!(parse_item_limit_categories(&good[..len], "enUS").is_err());
    }
    let mut extra = good.clone();
    extra.push(0);
    assert!(parse_item_limit_categories(&extra, "enUS").is_err());
    assert!(parse_item_limit_categories(&good, "unknown").is_err());
    assert!(parse_item_limit_categories(&dbc(&[], b"\0"), "enUS")
        .unwrap()
        .is_empty());
}
#[test]
fn rejects_invalid_strings_even_in_inactive_locale() {
    for (offset, strings) in [(99, &b"\0"[..]), (1, &b"\0bad"[..]), (1, &b"\0\xff\0"[..])] {
        let mut row = [0; 20];
        row[16] = offset;
        assert!(parse_item_limit_categories(&dbc(&[row], strings), "enUS").is_err());
    }
}
