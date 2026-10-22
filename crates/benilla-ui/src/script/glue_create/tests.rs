use std::collections::HashMap;

use super::*;
use crate::script::UiScript;

fn race(id: u8, name: &str, female: &str, file: &str, side: &str) -> GlueCreateRace {
    GlueCreateRace {
        id,
        names: [name.into(), female.into()],
        file: file.into(),
        enabled: true,
        faction: (side.into(), side.into()),
        hair: if id == 2 {
            "HORN".into()
        } else {
            "HAIR".into()
        },
        facial_hair: ["BEARD".into(), "NORMAL".into()],
    }
}

fn class(id: u8, name: &str, file: &str, enabled: bool) -> GlueCreateClass {
    GlueCreateClass {
        id,
        names: [name.into(), name.into()],
        file: file.into(),
        enabled,
        roles: [id == 1, id == 2, true],
    }
}

/// Human and Orc; Warrior, Paladin, a gated Death Knight and Mage. Humans may be any of them,
/// Orcs a Warrior or a Death Knight.
fn vm() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.install_glue_create_api().unwrap();
    s.set_glue_create_catalog(GlueCreateCatalog {
        races: vec![
            race(1, "Human", "Human", "Human", "Alliance"),
            race(2, "Orc", "Orcess", "Orc", "Horde"),
        ],
        classes: vec![
            class(1, "Warrior", "WARRIOR", true),
            class(2, "Paladin", "PALADIN", true),
            class(6, "Death Knight", "DEATHKNIGHT", false),
            class(8, "Mage", "MAGE", true),
        ],
        valid: vec![(1, 1), (1, 2), (1, 6), (1, 8), (2, 1), (2, 6)],
        dials: HashMap::from([
            ((1, 0), [3, 4, 5, 6, 7]),
            ((1, 1), [2, 2, 2, 2, 1]),
            ((2, 0), [9, 9, 9, 9, 9]),
            ((2, 1), [9, 9, 9, 9, 9]),
        ]),
        backgrounds: HashMap::from([((2, 1), "Orc".to_string())]),
        names: HashMap::from([(
            (1, 0),
            ["Agamand", "Andromath", "Antonidas", "Nielas"].map(String::from).to_vec(),
        )]),
    });
    s.run("ResetCharCustomize()").unwrap();
    s
}

#[test]
fn the_lists_answer_triples_and_the_selection_reads_back_at_once() {
    let s = vm();
    let races = s
        .eval::<(String, String, f64, String, String, f64)>("return GetAvailableRaces()")
        .unwrap();
    assert_eq!(
        races,
        (
            "Human".into(),
            "Human".into(),
            1.0,
            "Orc".into(),
            "Orc".into(),
            1.0
        )
    );
    assert_eq!(
        s.eval::<i64>("return select('#', GetAvailableClasses())")
            .unwrap(),
        12
    );
    // The handler idiom: set, then read back in the same call.
    let got = s
        .eval::<(i64, String, String, String)>(
            "SetSelectedRace(2) local race, file = GetNameForRace() \
             local _, fac = GetFactionForRace(GetSelectedRace()) \
             return GetSelectedRace(), race, fac, (GetSelectedClass())",
        )
        .unwrap();
    assert_eq!(got, (2, "Orc".into(), "Horde".into(), "Warrior".into()));
    assert_eq!(
        s.eval::<String>("return GetHairCustomization()").unwrap(),
        "HORN"
    );
    assert_eq!(
        s.eval::<String>("return GetCreateBackgroundModel()").unwrap(),
        "Orc"
    );
    s.run("SetSelectedRace(1)").unwrap();
    assert_eq!(
        s.eval::<String>("return GetCreateBackgroundModel()").unwrap(),
        "Human",
        "no seeded stage: the race file"
    );
}

#[test]
fn sex_maps_two_and_three_and_names_follow_it() {
    let s = vm();
    assert_eq!(s.eval::<i64>("return GetSelectedSex()").unwrap(), 2);
    s.run("SetSelectedRace(2) SetSelectedSex(3)").unwrap();
    assert_eq!(s.eval::<i64>("return GetSelectedSex()").unwrap(), 3);
    assert_eq!(
        s.eval::<String>("return (GetNameForRace())").unwrap(),
        "Orcess"
    );
    assert_eq!(
        s.eval::<String>("return GetFacialHairCustomization()")
            .unwrap(),
        "NORMAL"
    );
    // Anything but 2 or 3 does nothing.
    s.run("SetSelectedSex(5)").unwrap();
    assert_eq!(s.eval::<i64>("return GetSelectedSex()").unwrap(), 3);
}

#[test]
fn a_class_switch_needs_the_pair_and_the_classes_own_flag() {
    let s = vm();
    assert!(s
        .eval::<bool>("return IsRaceClassValid(1, 2) == 1")
        .unwrap());
    assert!(s
        .eval::<bool>("return IsRaceClassValid(2, 2) == nil")
        .unwrap());
    // Paladin, then a switch to Orc falls back to the first live row.
    s.run("SetSelectedClass(2)").unwrap();
    let (name, _, row) = s
        .eval::<(String, String, i64)>("return GetSelectedClass()")
        .unwrap();
    assert_eq!((name.as_str(), row), ("Paladin", 2));
    s.run("SetSelectedRace(2)").unwrap();
    assert_eq!(
        s.eval::<String>("return (GetSelectedClass())").unwrap(),
        "Warrior"
    );
    // A gated Death Knight is refused though the pair is valid.
    s.run("SetSelectedClass(3)").unwrap();
    assert_eq!(
        s.eval::<String>("return (GetSelectedClass())").unwrap(),
        "Warrior"
    );
    // The three role bits close the six values.
    let roles = s
        .eval::<(bool, bool, bool)>("return select(4, GetSelectedClass())")
        .unwrap();
    assert_eq!(roles, (true, false, true));
}

#[test]
fn dials_wrap_randomize_stays_in_range_and_sex_reclamps() {
    let mut s = vm();
    s.run("CycleCharCustomization(1, -1)").unwrap();
    assert_eq!(
        s.glue_create_selection().dials[0],
        2,
        "skin wraps 0 -> 2 of 3"
    );
    s.run("CycleCharCustomization(5, 1) CycleCharCustomization(5, 5)")
        .unwrap();
    assert_eq!(
        s.glue_create_selection().dials[4],
        2,
        "the delta's sign steps once"
    );
    s.seed_glue_create_rng(42);
    for _ in 0..20 {
        s.run("RandomizeCharCustomization()").unwrap();
        let d = s.glue_create_selection().dials;
        assert!(d.iter().zip([3, 4, 5, 6, 7]).all(|(&v, n)| v < n), "{d:?}");
    }
    s.run("SetSelectedSex(3)").unwrap();
    let d = s.glue_create_selection().dials;
    assert!(d.iter().zip([2, 2, 2, 2, 1]).all(|(&v, n)| v < n), "{d:?}");
}

#[test]
fn facing_is_degrees_over_radians_and_the_calls_queue() {
    let mut s = vm();
    s.run("SetCharacterCreateFacing(-15)").unwrap();
    let deg = s.eval::<f64>("return GetCharacterCreateFacing()").unwrap();
    assert!((deg + 15.0).abs() < 1e-4, "{deg}");
    assert!((s.glue_create_selection().facing + 15f32.to_radians()).abs() < 1e-6);
    s.run(
        "SetCharCustomizeFrame('CharacterCreate') UpdateCustomizationScene() \
         CreateCharacter('Thrall')",
    )
    .unwrap();
    assert_eq!(
        s.take_glue_create_requests(),
        vec![
            GlueCreateRequest::CustomizeFrame("CharacterCreate".into()),
            GlueCreateRequest::UpdateScene,
            GlueCreateRequest::Create("Thrall".into()),
        ]
    );
    for bad in [
        "SetSelectedRace()",
        "SetCharacterCreateFacing('x')",
        "CycleCharCustomization(1)",
        "SetCharCustomizeFrame()",
        "GetFactionForRace(nil)",
    ] {
        let err = s.run(bad).unwrap_err().to_string();
        assert!(err.contains("Usage:"), "{bad}: {err}");
    }
    let name = s.eval::<String>("return GetRandomName()").unwrap();
    assert!((4..=10).contains(&name.len()), "{name}");
}
