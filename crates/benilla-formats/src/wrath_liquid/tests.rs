use super::*;

fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
    let mut b = b"WDBC".to_vec();
    for w in [
        rows.len() as u32,
        N as u32,
        N as u32 * 4,
        strings.len() as u32,
    ] {
        b.extend(w.to_le_bytes());
    }
    for row in rows {
        for w in row {
            b.extend(w.to_le_bytes());
        }
    }
    b.extend(strings);
    b
}
fn row(id: u32, material: u32) -> [u32; 45] {
    let mut r = std::array::from_fn(|i| 0xf1230000 + i as u32);
    r[0] = id;
    r[1] = 1;
    r[14] = material;
    r[15..21].copy_from_slice(&[0, 6, 6, 0, 6, 0]);
    r
}

#[test]
fn lossless_rows_strings_and_native_last_duplicate_lookup() {
    let rows = [row(2, 0), row(5, 7), row(2, 9)];
    let c = parse_liquid_catalogs(
        &dbc(&rows, b"\0name\0path\0"),
        &dbc(&[[0, 2, 99], [9, 0, 42], [9, 1, 43]], b"\0"),
    )
    .unwrap();
    assert_eq!(c.types().len(), 3);
    assert_eq!(c.materials().len(), 3);
    for (r, raw) in c.types().iter().zip(rows) {
        assert_eq!(*r.raw_columns(), raw);
        assert_eq!(r.flags(), raw[2]);
        assert_eq!(r.name, "name");
        assert_eq!(r.strings, ["", "path", "path", "", "path", ""]);
    }
    let last = c.liquid_type(2).unwrap();
    assert_eq!(last.material_id(), 9);
    assert_eq!(c.material_for(last).unwrap().raw_columns(), &[9, 1, 43]);
    assert_eq!(c.material(0).unwrap().raw_render_selector(), 2);
    assert!(c.material_for(c.liquid_type(5).unwrap()).is_none());
    for absent in [0, 3, u32::MAX] {
        assert!(c.liquid_type(absent).is_none());
    }
}

#[test]
fn empty_tables_are_not_synthesized() {
    let c = parse_liquid_catalogs(&dbc::<45>(&[], b"\0"), &dbc::<3>(&[], b"\0")).unwrap();
    assert!(c.types().is_empty());
    assert!(c.materials().is_empty());
    assert!(c.material(0).is_none());
}

#[test]
fn malformed_headers_lengths_and_strings_fail() {
    let good = dbc(&[row(2, 0)], b"\0name\0path\0");
    let mat = dbc(&[[0, 2, 0]], b"\0");
    for len in [0, 4, 19, good.len() - 1] {
        assert!(parse_liquid_catalogs(&good[..len], &mat).is_err());
    }
    for offset in [0, 8, 12, 16] {
        let mut bad = good.clone();
        bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_liquid_catalogs(&bad, &mat).is_err());
    }
    for column in [1, 15, 16, 17, 18, 19, 20] {
        let mut bad = row(2, 0);
        bad[column] = 9999;
        assert!(parse_liquid_catalogs(&dbc(&[bad], b"\0name\0path\0"), &mat).is_err());
    }
    assert!(parse_liquid_catalogs(&good, &mat[..mat.len() - 1]).is_err());
    assert!(parse_liquid_catalogs(&dbc(&[row(2, 0)], b"\0name\0path"), &mat).is_err());
    assert!(parse_liquid_catalogs(&dbc(&[row(2, 0)], b"\0name\0\xff\0"), &mat).is_err());
}

#[test]
fn semantic_kind_uses_type_word_not_id_or_render_selector() {
    use crate::LiquidKind;
    for (category, expected) in [
        (0, Some(LiquidKind::Still)),
        (1, Some(LiquidKind::Ocean)),
        (2, Some(LiquidKind::Magma)),
        (3, Some(LiquidKind::Slime)),
        (4, None),
        (u32::MAX, None),
    ] {
        let mut raw = row(0x1239, 7);
        raw[3] = category;
        let catalogs =
            parse_liquid_catalogs(&dbc(&[raw], b"\0name\0path\0"), &dbc(&[[7, 2, 0]], b"\0"))
                .unwrap();
        let liquid = catalogs.liquid_type(0x1239).unwrap();
        assert_eq!(liquid.kind(), expected);
        assert_eq!(
            catalogs.material_for(liquid).unwrap().raw_render_selector(),
            2
        );
        assert!(catalogs.liquid_type(0x1238).is_none());
    }
}
