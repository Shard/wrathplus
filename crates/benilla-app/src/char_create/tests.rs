use super::*;

/// Every 3.3.5 create result (AzerothCore `ResponseCodes` `0x2e..=0x45`) names a
/// `CHAR_CREATE_*` key in the 12340 status table. The shipped GlueStrings carry `0x2e..=0x3d`;
/// the paid-service rows `0x3e..=0x45` (race and faction change, mail, gold, forced login) have
/// no glue string in 12340, so the status line shows the built-in caption for them. Asserted on
/// the loader's own table (`Localize()` over `GlueStrings.lua`).
#[test]
fn every_char_create_result_resolves_in_the_real_glue_strings() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = benilla_formats::open_chain(&data).expect("open chain");
    let map = crate::glue_strings::table_from_chain(&mut chain).into_map();
    for code in 0x2Eu8..=0x45 {
        let key = crate::char_select::wrath_char_result_key(code).expect("a table row");
        assert!(key.starts_with("CHAR_CREATE_"), "{code:#04x}: {key}");
        assert_eq!(
            map.contains_key(key),
            code <= 0x3D,
            "{code:#04x}: {key} shipped, against the 12340 split"
        );
    }
    assert_eq!(
        crate::char_select::wrath_char_result_key(0x32),
        Some("CHAR_CREATE_NAME_IN_USE")
    );
    assert_eq!(
        crate::char_select::wrath_char_result_key(0x5F),
        Some("CHAR_NAME_RESERVED")
    );
}

/// The server's native answer reaches the screen: a success returns to select with the new
/// name armed, a refusal ends Creating and shows its 12340 key's text.
#[test]
fn a_native_create_result_ends_creating_and_answers_the_screen() {
    use crate::net::WrathCharActionResultMessage as Native;
    let run = |code: u8| {
        let (tx, _rx) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(ClientState::CharCreate)
            .init_resource::<Roster>()
            .init_resource::<paid::PaidChange>()
            .init_resource::<crate::glue::dialog::GlueDialog>()
            .insert_resource(GlueStrings::from_map(
                [(
                    "CHAR_CREATE_NAME_IN_USE".to_string(),
                    "That name is unavailable".to_string(),
                )]
                .into(),
            ))
            .insert_resource(CharPick(tx))
            .add_message::<Native>()
            .add_message::<crate::glue::dialog::GlueDialogAnswer>()
            .add_systems(Update, create_result);
        let mut sel = CreateSelection {
            creating: true,
            ..Default::default()
        };
        sel.name.set_text("Newbie");
        app.insert_resource(sel);
        app.world_mut().spawn((Text::new(""), parts::StatusLine));
        app.world_mut().write_message(Native {
            action: CharAction::Create,
            code,
        });
        app.update();
        app.update(); // the state transition applies
        let creating = app.world().resource::<CreateSelection>().creating;
        let state = *app.world().resource::<State<ClientState>>().get();
        let created = app.world().resource::<Roster>().just_created.clone();
        let line = app
            .world_mut()
            .query_filtered::<&Text, With<parts::StatusLine>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        (creating, state, created, line)
    };
    let (creating, state, created, _) = run(benilla_protocol::wrath_world::CHAR_CREATE_SUCCESS);
    assert!(!creating);
    assert_eq!(state, ClientState::CharSelect);
    assert_eq!(created.as_deref(), Some("Newbie"));
    let (creating, state, created, line) = run(0x32);
    assert!(!creating);
    assert_eq!(state, ClientState::CharCreate);
    assert_eq!(created, None);
    assert_eq!(line, "That name is unavailable");
}

#[test]
fn race_columns_match_the_reference_screen() {
    assert_eq!(
        ALLIANCE,
        [1, 3, 4, 7, 11],
        "Human, Dwarf, Night Elf, Gnome, Draenei"
    );
    assert_eq!(
        HORDE,
        [2, 5, 6, 8, 10],
        "Orc, Scourge, Tauren, Troll, Blood Elf"
    );
    let mut all: Vec<u8> = ALLIANCE.iter().chain(&HORDE).copied().collect();
    all.sort_unstable();
    assert_eq!(all, [1, 2, 3, 4, 5, 6, 7, 8, 10, 11]);
    assert!(ALLIANCE.windows(2).all(|w| w[0] < w[1]));
    assert!(HORDE.windows(2).all(|w| w[0] < w[1]));
}

/// Ten class buttons in `ChrClasses` order, Death Knight sixth; a class the race cannot be
/// stays on its button, disabled, and a click on it changes nothing.
#[test]
fn every_class_has_a_slot_and_invalid_ones_refuse() {
    assert_eq!(CLASS_SLOTS, [1, 2, 3, 4, 5, 6, 7, 8, 9, 11]);
    assert_eq!(class_file(6), "DEATHKNIGHT");
    assert!(crate::glue::art::class_tc(6).is_some());
    for race in ALLIANCE.iter().chain(&HORDE) {
        assert!(crate::glue::art::race_tc(*race, 0).is_some(), "race {race}");
        assert!(crate::glue::art::race_tc(*race, 1).is_some(), "race {race}");
    }
    // Without a catalog every class reads as allowed.
    assert!(CLASS_SLOTS.iter().all(|&c| class_allowed(None, 10, c)));
}

/// `DEATHKNIGHT_DISABLED`: the Death Knight needs a level-55 character on the roster.
#[test]
fn the_death_knight_needs_a_level_55_character() {
    let mut sel = CreateSelection {
        race: 10,
        class: 6,
        ..default()
    };
    assert!(!class_enabled(None, &sel, 6));
    assert!(class_enabled(None, &sel, 1));
    sel.death_knight = true;
    assert!(class_enabled(None, &sel, 6));
}

#[test]
fn look_carries_the_class() {
    let mut sel = CreateSelection {
        race: 1,
        sex: 0,
        class: 1,
        ..default()
    };
    let warrior = sel.look();
    sel.class = 8; // mage, a different starting outfit
    let mage = sel.look();
    assert_eq!(warrior.class, 1);
    assert_eq!(mage.class, 8);
    assert_ne!(
        warrior, mage,
        "the booth look must differ by class, or the preview cannot re-dress"
    );
}

/// `SetCharacterRace`, `SetCharacterClass` and `SetCharacterGender` `LockHighlight()` the
/// chosen button (`CharacterCreate.lua`); with the template's `<CheckedTexture>` commented out,
/// that lock is the whole selected visual. Built with the real `icon_button`, which must insert
/// the `LockHighlight` the refresh query needs.
#[test]
fn the_chosen_icons_are_lock_highlighted() {
    use crate::glue::art::GlueArt;
    use crate::glue::widgets::{icon_button, LockHighlight};

    fn spawn_icons(mut commands: Commands, art: Res<GlueArt>) {
        let font = Handle::<Font>::default();
        commands.spawn(Node::default()).with_children(|p| {
            for race in ALLIANCE {
                icon_button(
                    p,
                    &font,
                    CreateAction::Race(race),
                    None::<parts::DynIcon>,
                    None,
                    None::<parts::DynText>,
                    "",
                    &art,
                    1.0,
                );
            }
            for sex in 0..2u8 {
                icon_button(
                    p,
                    &font,
                    CreateAction::Gender(sex),
                    None::<parts::DynIcon>,
                    None,
                    None::<parts::DynText>,
                    "",
                    &art,
                    1.0,
                );
            }
            for slot in 0..10u8 {
                icon_button(
                    p,
                    &font,
                    CreateAction::ClassSlot(slot),
                    None::<parts::DynIcon>,
                    None,
                    None::<parts::DynText>,
                    "",
                    &art,
                    1.0,
                );
            }
        });
    }

    let mut app = App::new();
    app.init_resource::<GlueArt>()
        // Human, female, mage: the mage's button is slot 7 of `CLASS_SLOTS`.
        .insert_resource(CreateSelection {
            race: 1,
            sex: 1,
            class: 8,
            ..default()
        })
        .add_systems(Startup, spawn_icons)
        .add_systems(Update, refresh::refresh_hover);
    app.update();

    let locked: Vec<CreateAction> = app
        .world_mut()
        .query::<(&CreateAction, &LockHighlight)>()
        .iter(app.world())
        .filter(|(_, l)| l.0)
        .map(|(a, _)| *a)
        .collect();
    assert_eq!(
        locked.len(),
        3,
        "exactly one race, one gender and one class icon is locked — got {locked:?}"
    );
    assert!(locked.contains(&CreateAction::Race(1)), "Human");
    assert!(locked.contains(&CreateAction::Gender(1)), "female");
    assert!(
        locked.contains(&CreateAction::ClassSlot(7)),
        "mage is slot 7 of [1,2,3,4,5,6,7,8,9,11] — the mage icon itself must carry the lock, \
         not sit unmarked"
    );

    // Moving the selection moves the lock, never lights a second icon.
    app.world_mut().resource_mut::<CreateSelection>().class = 1; // warrior, slot 0
    app.update();
    let locked: Vec<CreateAction> = app
        .world_mut()
        .query::<(&CreateAction, &LockHighlight)>()
        .iter(app.world())
        .filter(|(_, l)| l.0)
        .map(|(a, _)| *a)
        .collect();
    assert!(locked.contains(&CreateAction::ClassSlot(0)));
    assert!(!locked.contains(&CreateAction::ClassSlot(7)));
}
