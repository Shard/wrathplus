use super::*;

fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
    let mut b = b"WDBC".to_vec();
    for n in [
        rows.len() as u32,
        N as u32,
        N as u32 * 4,
        strings.len() as u32,
    ] {
        b.extend_from_slice(&n.to_le_bytes());
    }
    for row in rows {
        for n in row {
            b.extend_from_slice(&n.to_le_bytes());
        }
    }
    b.extend_from_slice(strings);
    b
}

#[test]
fn dungeons_keep_signed_words_locales_masks_and_duplicates_in_source_order() {
    let mut a = [0; 49];
    a[0] = 9;
    a[1] = 1;
    a[3] = 9;
    a[17] = 0xabcdef;
    for (i, n) in a.iter_mut().enumerate().take(32).skip(18) {
        *n = i as u32;
    }
    a[23] = u32::MAX;
    a[27] = u32::MAX;
    a[28] = 18;
    a[30] = (-2i32) as u32;
    a[31] = (-3i32) as u32;
    a[32] = 26;
    a[48] = 0x123456;
    let mut b = a;
    b[18] = 99;
    let bytes = dbc(&[a, b], b"\0English\0Francais\0Texture\0Description\0");
    let catalog = parse_lfg_dungeons(&bytes, "frFR").unwrap();
    let r = catalog.get(9).unwrap();
    assert_eq!(r.raw_columns(), &a);
    assert_eq!(
        (&*r.name, &*r.description, &*r.texture),
        ("Francais", "", "Texture")
    );
    assert_eq!(
        (r.min_level(), r.max_level(), r.target_level()),
        (18, 19, 20)
    );
    assert_eq!((r.target_min_level(), r.target_max_level()), (21, 22));
    assert_eq!(
        (r.map_id(), r.faction(), r.order(), r.group_id()),
        (-1, -1, -2, -3)
    );
    assert_eq!(
        (r.difficulty(), r.flags(), r.dungeon_type(), r.expansion()),
        (24, 25, 26, 29)
    );
    assert_eq!(catalog.rows()[1].min_level(), 99);
    assert_eq!(catalog.len(), 2);
    assert!(!catalog.is_empty());
    assert!(catalog.get(123).is_none());
    assert_eq!(
        parse_lfg_dungeons(&bytes, "enGB")
            .unwrap()
            .get(9)
            .unwrap()
            .description,
        "Description"
    );
}

#[test]
fn groups_preserve_priority_and_unknown_header_word() {
    let mut r = [0; 21];
    r[0] = 12;
    r[1] = 1;
    r[17] = 0xff;
    r[18] = (-7i32) as u32;
    r[19] = 0xfefefefe;
    r[20] = 6;
    let c = parse_lfg_dungeon_groups(&dbc(&[r], b"\0Group\0"), "enUS").unwrap();
    let row = c.get(12).unwrap();
    assert_eq!(row.raw_columns(), &r);
    assert_eq!(
        (
            row.sort_priority(),
            row.raw_header_word(),
            row.dungeon_type()
        ),
        (-7, 0xfefefefe, 6)
    );
    assert_eq!(row.name, "Group");
    assert_eq!(c.rows().len(), c.len());
    assert!(!c.is_empty());
}

#[test]
fn expansion_join_only_searches_first_contiguous_dungeon_run() {
    let rows = [
        [9, 10, 0, 1234, 11, 22, 33, 44],
        [8, 10, 1, 999, 55, 66, 77, 88],
        [7, 11, 0, 0, 0, 0, 0, 0],
        [6, 10, 2, 0, 0, 0, 0, 0],
    ];
    let c = parse_lfg_dungeon_expansions(&dbc(&rows, b"\0")).unwrap();
    assert_eq!(c.rows()[0].raw_columns(), &rows[0]);
    let r = c.find(10, 1).unwrap();
    assert_eq!(
        (
            r.id(),
            r.min_level(),
            r.max_level(),
            r.target_min_level(),
            r.target_max_level()
        ),
        (8, 55, 66, 77, 88)
    );
    assert!(c.find(10, 2).is_none());
    assert!(c.find(123, 0).is_none());
    assert_eq!(c.get(9), c.rows().first());
    assert_eq!(c.len(), 4);
    assert!(!c.is_empty());
}

#[test]
fn map_difficulty_is_optional_without_fallback_and_retains_unknowns() {
    let mut r = [0; 23];
    r[0] = 2;
    r[1] = 33;
    r[2] = 1;
    r[3] = 1;
    r[19] = 0x1234;
    r[20] = 0xabcdef;
    r[21] = 5;
    r[22] = 9;
    let mut other = r;
    other[0] = 3;
    other[1] = 34;
    let mut disjoint = r;
    disjoint[0] = 4;
    disjoint[2] = 2;
    let c = parse_lfg_map_difficulties(&dbc(&[r, other, disjoint], b"\0Message\0Raw\0"), "enUS")
        .unwrap();
    let row = c.find(33, 1).unwrap();
    assert_eq!(row.raw_columns(), &r);
    assert_eq!(
        (&*row.message, &*row.raw_string, row.max_players()),
        ("Message", "Raw", 5)
    );
    assert!(c.find(33, 0).is_none());
    assert!(c.find(33, 2).is_none());
    assert!(c.find(-1, 1).is_none());
    assert_eq!(c.get(2), Some(row));
    assert_eq!(c.rows().len(), c.len());
    assert!(!c.is_empty());
}

#[test]
fn malformed_headers_lengths_and_all_string_offsets_fail_closed() {
    let valid = dbc(&[[0; 49]], b"\0");
    for n in [0, 4, 19, 20, valid.len() - 1] {
        assert!(parse_lfg_dungeons(&valid[..n], "enUS").is_err());
    }
    for (offset, value) in [(0, 0), (4, u32::MAX), (8, 48), (12, 192), (16, 999)] {
        let mut b = valid.clone();
        b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(parse_lfg_dungeons(&b, "enUS").is_err());
    }
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(parse_lfg_dungeons(&trailing, "enUS").is_err());
    for col in [1, 16, 28, 32, 47] {
        let mut r = [0; 49];
        r[col] = 2;
        assert!(parse_lfg_dungeons(&dbc(&[r], b"\0"), "enUS").is_err());
        r[col] = 1;
        assert!(parse_lfg_dungeons(&dbc(&[r], b"\0x"), "enUS").is_err());
        assert!(parse_lfg_dungeons(&dbc(&[r], b"\0\xff\0"), "enUS").is_err());
    }
    assert!(parse_lfg_dungeons(&valid, "unknown").is_err());
    assert!(parse_lfg_dungeon_groups(&valid, "enUS").is_err());
    assert!(parse_lfg_dungeon_expansions(&valid).is_err());
    assert!(parse_lfg_map_difficulties(&valid, "enUS").is_err());
}

#[test]
fn empty_tables_are_valid_and_still_require_exact_shapes() {
    assert!(parse_lfg_dungeons(&dbc::<49>(&[], b"\0"), "enUS")
        .unwrap()
        .is_empty());
    assert!(parse_lfg_dungeon_groups(&dbc::<21>(&[], b"\0"), "enUS")
        .unwrap()
        .is_empty());
    assert!(parse_lfg_dungeon_expansions(&dbc::<8>(&[], b"\0"))
        .unwrap()
        .is_empty());
    assert!(parse_lfg_map_difficulties(&dbc::<23>(&[], b"\0"), "enUS")
        .unwrap()
        .is_empty());
}

#[test]
fn encounters_read_map_difficulty_bit_name_and_icon_id() {
    let strings = b"\0Patchwerk\0";
    let mut row = [0u32; 23];
    row[0] = 465;
    row[1] = 533;
    row[2] = 1;
    row[3] = 0;
    row[4] = 12;
    row[5] = 1;
    row[22] = 77;
    let rows = parse_dungeon_encounters(&dbc(&[row], strings), "enUS").unwrap();
    let e = &rows[0];
    assert_eq!(
        (e.id(), e.map_id(), e.difficulty(), e.bit(), e.name.as_str(), e.spell_icon()),
        (465, 533, 1, 12, "Patchwerk", 77)
    );
    assert_eq!(e.icon, None);
}
