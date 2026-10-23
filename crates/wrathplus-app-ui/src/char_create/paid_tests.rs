use super::*;

fn target(race: u8, flags: u32) -> PaidTarget {
    PaidTarget {
        guid: 7,
        name: "Arthas".into(),
        race,
        class: 1,
        sex: 0,
        dials: [1, 2, 3, 4, 5],
        flags,
        service: PaidService::offered(flags).expect("a service"),
    }
}

/// One button per row: faction over race over customize.
#[test]
fn the_row_offers_faction_over_race_over_customize() {
    assert_eq!(PaidService::offered(0), None);
    assert_eq!(PaidService::offered(0x1), Some(PaidService::Customize));
    assert_eq!(
        PaidService::offered(0x10_0001),
        Some(PaidService::RaceChange)
    );
    assert_eq!(
        PaidService::offered(0x11_0001),
        Some(PaidService::FactionChange)
    );
}

/// `CharacterChangeFixup`: a customize keeps the race, a race change its faction, a faction
/// change the other one, each only where the class is valid.
#[test]
fn the_races_narrow_by_service() {
    let any = |_: u8, _: u8| true;
    let no_dwarf = |race: u8, _: u8| race != 3;
    let human = target(1, 0x1);
    assert!(human.race_allowed(1, any) && !human.race_allowed(3, any));
    let race = target(1, 0x10_0000);
    assert!(race.race_allowed(3, any) && !race.race_allowed(2, any));
    assert!(!race.race_allowed(3, no_dwarf));
    let faction = target(1, 0x1_0000);
    assert!(faction.race_allowed(2, any) && faction.race_allowed(10, any));
    assert!(!faction.race_allowed(3, any), "Dwarf is the same faction");
    assert!(
        faction.race_allowed(1, no_dwarf),
        "the character's own race"
    );
}

/// The current name passes unchecked; the flags pick race change, then faction change, then
/// customize, with the appearance in the request's order.
#[test]
fn the_request_follows_the_characters_flags() {
    let (cmd, status) = target(1, 0x11_0001)
        .request("arthas", 3, 1, [1, 2, 3, 4, 5])
        .unwrap();
    assert_eq!(status, "RACE_CHANGE_IN_PROGRESS");
    assert_eq!(
        cmd,
        Command::ChangeRace {
            faction: false,
            guid: 7,
            name: "arthas".into(),
            appearance: Appearance {
                gender: 1,
                skin: 1,
                hair_color: 4,
                hair_style: 3,
                facial_hair: 5,
                face: 2,
            },
            race: 3,
        }
    );
    assert_eq!(
        target(1, 0x1_0000)
            .request("Arthas", 2, 0, [0; 5])
            .unwrap()
            .1,
        "FACTION_CHANGE_IN_PROGRESS"
    );
    assert!(matches!(
        target(1, 0x1).request("Arthas", 1, 0, [0; 5]).unwrap().0,
        Command::Customize { .. }
    ));
    assert_eq!(
        target(1, 0x1).request("", 1, 0, [0; 5]).unwrap_err(),
        "CHAR_NAME_NO_NAME"
    );
    assert_eq!(
        target(1, 0x1).request("A", 1, 0, [0; 5]).unwrap_err(),
        "CHAR_NAME_TOO_SHORT"
    );
}
