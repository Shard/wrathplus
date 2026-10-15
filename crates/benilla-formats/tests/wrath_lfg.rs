//! Effective installed enUS metadata; no queue/eligibility or stock-table certification.
use benilla_formats::{lfg::load_lfg_catalogs, Chain};

#[test]
fn installed_wrath_lfg_catalogs() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").expect("Wrath chain");
    // Independent raw WDBC inspection below locks the file words to the known
    // values; production typed accessors must agree with native decoder offsets.
    let raw = chain.read_file("DBFilesClient\\LFGDungeons.dbc").unwrap();
    let word = |col: usize| u32::from_le_bytes(raw[20 + col * 4..24 + col * 4].try_into().unwrap());
    assert_eq!(
        (word(0), word(18), word(19), word(23), word(27)),
        (1, 15, 25, 43, u32::MAX)
    );
    let raw_expansion = chain
        .read_file("DBFilesClient\\LFGDungeonExpansion.dbc")
        .unwrap();
    let words: Vec<_> = raw_expansion[20..52]
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(words, [38, 2, 0, 258, 57, 60, 59, 60]);

    let c = load_lfg_catalogs(&mut chain).expect("native LFG catalogs");
    assert_eq!(
        (
            c.dungeons.len(),
            c.groups.len(),
            c.expansions.len(),
            c.map_difficulties.len()
        ),
        (195, 10, 29, 187)
    );
    let r = c.dungeons.get(1).expect("Wailing Caverns");
    assert_eq!(
        (&*r.name, &*r.texture, &*r.description),
        ("Wailing Caverns", "WAILINGCAVERNS", "")
    );
    assert_eq!(
        (
            r.min_level(),
            r.max_level(),
            r.target_level(),
            r.target_min_level(),
            r.target_max_level()
        ),
        (15, 25, 19, 17, 20)
    );
    assert_eq!(
        (
            r.map_id(),
            r.difficulty(),
            r.flags(),
            r.dungeon_type(),
            r.faction(),
            r.expansion(),
            r.order(),
            r.group_id()
        ),
        (43, 0, 3, 1, -1, 0, 0, 1)
    );
    assert_eq!(
        (r.raw_columns()[17], r.raw_columns()[48]),
        (16712190, 16712188)
    );
    let r = c.dungeons.get(4).expect("Ragefire Chasm");
    assert_eq!(
        (&*r.name, r.map_id(), r.faction()),
        ("Ragefire Chasm", 389, 0)
    );
    // Source order is not ID-sorted: native groups begin 5,4,3.
    assert_eq!(
        c.groups.rows()[..3]
            .iter()
            .map(|r| r.id())
            .collect::<Vec<_>>(),
        [5, 4, 3]
    );
    let g = c.groups.get(5).unwrap();
    assert_eq!(
        (
            &*g.name,
            g.sort_priority(),
            g.raw_header_word(),
            g.dungeon_type()
        ),
        ("Wrath of the Lich King Heroic", 1, 0, 5)
    );
    let e = c
        .expansions
        .find(2, 0)
        .expect("Scholomance expansion override");
    assert_eq!(e.raw_columns(), &[38, 2, 0, 258, 57, 60, 59, 60]);
    assert_eq!(
        (
            e.min_level(),
            e.max_level(),
            e.target_min_level(),
            e.target_max_level()
        ),
        (57, 60, 59, 60)
    );
    assert!(c.expansions.find(u32::MAX, 0).is_none());
    let m = c.map_difficulties.find(0, 0).expect("map zero row");
    assert_eq!(
        (m.id(), m.max_players(), &*m.message, &*m.raw_string),
        (1, 0, "", "")
    );
    assert!(c.map_difficulties.find(-1, 0).is_none());
    assert!(c.dungeons.get(u32::MAX).is_none());
}
