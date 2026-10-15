//! Installed effective Wrath archive metadata; no player completion state required.
use benilla_formats::{
    achievements::{load_achievement_categories, load_achievement_criteria, load_achievements},
    Chain,
};
use std::path::Path;

#[test]
fn installed_wrath_achievement_metadata() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        eprintln!("SKIP installed achievement test: select WOW_CLIENT_PROFILE=wrath");
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(Path::new(&data), "enUS").unwrap();
    for (table, shape) in [
        ("Achievement", [1817, 62, 248]),
        ("Achievement_Criteria", [7655, 31, 124]),
        ("Achievement_Category", [86, 20, 80]),
    ] {
        let b = chain
            .read_file(&format!("DBFilesClient\\{table}.dbc"))
            .unwrap();
        assert_eq!(&b[..4], b"WDBC");
        for (i, n) in shape.into_iter().enumerate() {
            assert_eq!(
                u32::from_le_bytes(b[4 + i * 4..8 + i * 4].try_into().unwrap()),
                n,
                "{table}"
            );
        }
    }
    let achievements = load_achievements(&mut chain).unwrap();
    let criteria = load_achievement_criteria(&mut chain).unwrap();
    let categories = load_achievement_categories(&mut chain).unwrap();
    assert_eq!(
        (achievements.len(), criteria.len(), categories.len()),
        (1817, 7655, 86)
    );
    let level10 = achievements.get(6).unwrap();
    assert_eq!(
        (&*level10.name, &*level10.description),
        ("Level 10", "Reach level 10.")
    );
    assert_eq!((level10.required_faction, level10.map_id), (-1, -1));
    assert_eq!(
        (
            level10.category_id,
            level10.points,
            level10.order,
            level10.flags,
            level10.icon_id
        ),
        (92, 10, 1, 4, 3268)
    );
    assert_eq!(achievements.get(7).unwrap().parent_achievement_id, 6);
    let stat = achievements.get(95).unwrap();
    assert_eq!((stat.flags, stat.reference_achievement_id), (73, 98));
    let c = criteria.get(34).unwrap();
    assert_eq!(
        (
            c.achievement_id,
            c.criteria_type,
            c.asset_id,
            c.required_quantity,
            c.flags,
            c.order
        ),
        (6, 5, 0, 10, 2, 1)
    );
    assert_eq!(c.description, "Reach level 10");
    assert_eq!(c.additional_requirements, [(0, 0), (0, 0)]);
    assert_eq!(
        criteria
            .for_achievement(6)
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        [34]
    );
    assert_eq!(criteria.get(35).unwrap().required_quantity, 20);
    let general = categories.get(92).unwrap();
    assert_eq!(
        (&*general.name, general.parent_id, general.order),
        ("General", -1, 1)
    );
    assert!(achievements.get(u32::MAX).is_none());
    assert!(criteria.get(u32::MAX).is_none());
    assert!(categories.get(u32::MAX).is_none());
}
