//! Effective enUS known rows, not a stock-table or live-animation certification.
use benilla_formats::{load_anim_data_catalog, Chain};

#[test]
fn installed_wrath_animation_data() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
    let raw = chain.read_file("DBFilesClient\\AnimationData.dbc").unwrap();
    let word = |i: usize| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
    assert_eq!((word(4), word(8), word(12)), (506, 8, 32));
    // Decoder 0x8a93e0: disk columns map directly to runtime offsets. Native fallback
    // walker 0x717814 reads row+0x14, distinct from row+0x18 in 0x7176b0.
    let row = raw[20..20 + 506 * 32]
        .chunks_exact(32)
        .find(|r| u32::from_le_bytes(r[..4].try_into().unwrap()) == 247)
        .unwrap();
    assert_eq!(u32::from_le_bytes(row[20..24].try_into().unwrap()), 246);
    assert_eq!(u32::from_le_bytes(row[24..28].try_into().unwrap()), 18);
    let c = load_anim_data_catalog(&mut chain).unwrap();
    assert_eq!(c.len(), 506);
    for (id, name, flags, fallback) in [
        (0, "Stand", 0, Some(147)),
        (17, "Attack1H", 32, Some(16)),
        (18, "Attack2H", 32, Some(17)),
        (100, "Sleep", 4, Some(99)),
        (143, "Sprint", 0, Some(5)),
        (187, "JumpLandRun", 0, Some(5)),
        (246, "FlyAttack1H", 32, Some(245)),
        (247, "FlyAttack2H", 32, Some(246)),
    ] {
        assert_eq!(c.name(id), Some(name));
        assert_eq!(c.weapon_flags(id), flags, "{name}");
        assert_eq!(c.fallback(id), fallback, "{name}");
    }
    assert_eq!(c.fallback(4), None);
    assert_eq!(c.name(u16::MAX), None);
    assert_eq!(c.weapon_flags(u16::MAX), 0);
    assert_eq!(c.fallback(u16::MAX), None);
}
