//! Installed build-12340 glyph, vehicle and quest-faction-reward tables (AzerothCore layouts).
use wrathplus_formats::{
    glyphs::load_glyph_catalog, load_quest_faction_reward, vehicles::load_vehicle_catalog, Chain,
};
use std::path::Path;

fn header(chain: &mut Chain, name: &str) -> [u32; 3] {
    let b = chain
        .read_file(&format!("DBFilesClient\\{name}.dbc"))
        .unwrap();
    assert_eq!(&b[..4], b"WDBC");
    std::array::from_fn(|i| u32::from_le_bytes(b[4 + 4 * i..8 + 4 * i].try_into().unwrap()))
}

#[test]
fn installed_wrath_glyph_vehicle_and_reward_tables() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    // Rows, fields, record size: the field counts are AzerothCore's formats.
    for (name, expected) in [
        ("GlyphProperties", [362, 4, 16]),
        ("GlyphSlot", [10, 3, 12]),
        ("Vehicle", [412, 40, 160]),
        ("VehicleSeat", [720, 58, 232]),
        ("QuestFactionReward", [2, 11, 44]),
    ] {
        assert_eq!(header(&mut chain, name), expected, "{name}");
    }

    // The talent frame's six sockets: GlyphSlot 21-26, ordered 1-6, three major (0) and three
    // minor (1).
    let glyphs = load_glyph_catalog(&mut chain).unwrap();
    let sockets: Vec<(u32, u32, u32)> = glyphs
        .slots()
        .filter(|s| s.order > 0)
        .map(|s| (s.id, s.type_flags, s.order))
        .collect();
    assert_eq!(
        sockets,
        [
            (21, 0, 1),
            (22, 1, 2),
            (23, 1, 3),
            (24, 0, 4),
            (25, 1, 5),
            (26, 0, 6)
        ]
    );
    assert_eq!(glyphs.properties().count(), 362);
    let g = glyphs.property(2).unwrap();
    assert_eq!((g.spell_id, g.type_flags, g.icon_id), (52084, 0, 3098));

    let vehicles = load_vehicle_catalog(&mut chain).unwrap();
    assert_eq!(
        (vehicles.vehicles().count(), vehicles.seats().count()),
        (412, 720)
    );
    let v = vehicles.vehicle(1).unwrap();
    assert_eq!(v.seats, [121, 201, 0, 0, 0, 0, 0, 0]);
    assert!((v.turn_speed - 1.047).abs() < 1e-3 && (v.pitch_max - 1.396).abs() < 1e-3);
    let s = vehicles.seat_of(1, 0).unwrap();
    assert_eq!((s.id, s.attachment_id), (121, 0));
    assert!((s.attachment_offset[2] + 0.38).abs() < 1e-4);
    assert!(s.can_enter_or_exit());
    // Vehicle 6's one seat (flags 0x6000880f) drives, but neither attacks nor switches seats.
    let s = vehicles.seat_of(6, 0).unwrap();
    assert_eq!((s.id, s.attachment_id, s.flags), (441, 13, 0x6000_880f));
    assert!(s.can_control() && !s.can_attack() && !s.can_switch());
    assert!(s.flags & wrathplus_formats::vehicles::SEAT_FLAG_HIDE_PASSENGER == 0);

    let rewards = load_quest_faction_reward(&mut chain).unwrap();
    assert_eq!(
        rewards.get(1),
        Some(&[0, 10, 25, 75, 150, 250, 350, 500, 1000, 5])
    );
    assert_eq!(
        (rewards.amount(5), rewards.amount(-8)),
        (Some(250), Some(-1000))
    );
    assert_eq!(rewards.amount(0), Some(0));
}
