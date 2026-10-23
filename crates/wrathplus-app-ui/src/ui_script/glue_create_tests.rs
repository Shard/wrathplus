use wrathplus_ui::script::glue_create::GlueCreateSelection;

use super::*;
use crate::ui_script::glue::load_glue_ui;

/// A glue VM with both APIs, GlueXML loaded off the install, the catalog seeded from it, and the
/// engine's start on the create screen. `None` without an install.
fn stock_create_vm() -> Option<UiScript> {
    let data = wrathplus_formats::wow_data()?;
    let mut chain = wrathplus_formats::open_chain(&data).ok()?;
    let catalog = CharCreateCatalog::load(&mut chain).expect("CharCreate catalog");
    let classes = wrathplus_formats::load_chr_classes(&mut chain).expect("ChrClasses");
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.install_glue_create_api().unwrap();
    script.set_screen_size(1600.0, 1000.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    let names = wrathplus_formats::name_gen::load_name_gen(&mut chain).expect("NameGen");
    script.set_glue_create_catalog(super::catalog(&catalog, Some(&classes), false, names));
    script.seed_glue_create_rng(7);
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![s("charcreate")]);
    script.tick(0.1);
    Some(script)
}

/// The stock CharacterCreate on the installed data: it opens on Human, its race, gender, class,
/// dial and randomize buttons drive the selection the booth wears, the race's stage reaches
/// `SetBackgroundModel` through `GetCreateBackgroundModel` (pinned against the stock call), and
/// Okay queues the typed name; Back returns to the list. Skips without an install.
#[test]
fn the_stock_character_create_drives_the_selection_on_the_installed_data() {
    let Some(mut script) = stock_create_vm() else {
        eprintln!("skipped: no installed client");
        return;
    };
    let shown = script
        .eval::<String>(
            "return tostring(CharacterCreate:IsVisible())..' '..GetCurrentGlueScreenName()..' '..\
             CharacterCreateRaceLabel:GetText()",
        )
        .unwrap();
    assert_eq!(shown, "1 charcreate Human");
    assert_eq!(script.glue_create_selection().race, 1);

    // The stock `SetBackgroundModel` hands `SetCharCustomizeBackground` the stage the getter names.
    script
        .run(
            "STAGES = {} local real = SetCharCustomizeBackground \
             SetCharCustomizeBackground = function(path) table.insert(STAGES, path) \
             if real then real(path) end end",
        )
        .unwrap();
    // Orc is the sixth button (Alliance's five, then Horde's first); a Troll stands on Orc's stage.
    script.run("CharacterCreateRaceButton6:Click()").unwrap();
    let orc = script
        .eval::<(String, String)>(
            "return CharacterCreateRaceLabel:GetText(), STAGES[#STAGES] or 'none'",
        )
        .unwrap();
    assert_eq!(
        orc,
        (
            "Orc".into(),
            r"Interface\Glues\Models\UI_Orc\UI_Orc.m2".into()
        )
    );
    assert_eq!(script.glue_create_selection().race, 2);
    script.run("CharacterCreateRaceButton9:Click()").unwrap();
    assert_eq!(
        script.eval::<String>("return STAGES[#STAGES]").unwrap(),
        r"Interface\Glues\Models\UI_Orc\UI_Orc.m2",
        "Troll stands on Orc's stage"
    );
    assert_eq!(script.glue_create_selection().race, 8);

    // Gender, a dial and Randomize move the selection the booth wears.
    script
        .run("CharacterCreateGenderButtonFemale:Click()")
        .unwrap();
    assert_eq!(script.glue_create_selection().sex, 1);
    let before = script.glue_create_selection().dials;
    script
        .run("CharacterCustomizationButtonFrame2RightButton:Click()")
        .unwrap();
    assert_ne!(
        script.glue_create_selection().dials[1],
        before[1],
        "the face dial moves"
    );
    script.run("CharCreateRandomizeButton:Click()").unwrap();
    // The stock Random Name button fills the box from the Troll female `NameGen.dbc` walk.
    assert!(script.eval::<bool>("return CharacterCreateRandomName:IsVisible() == 1").unwrap());
    script.run("CharacterCreateRandomName:Click()").unwrap();
    let random = script.eval::<String>("return CharacterCreateNameEdit:GetText()").unwrap();
    assert!(
        (4..=10).contains(&random.len())
            && random.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && random.chars().all(|c| c.is_ascii_alphabetic()),
        "{random:?}"
    );

    // Okay sends the typed name; Back returns to the list.
    script
        .run("CharacterCreateNameEdit:SetText('Glueone') CharCreateOkayButton:Click()")
        .unwrap();
    assert!(
        script
            .take_glue_create_requests()
            .contains(&GlueCreateRequest::Create("Glueone".into())),
        "Okay queues the create"
    );
    script.run("CharCreateBackButton:Click()").unwrap();
    assert_eq!(script.glue_current_screen().as_deref(), Some("charselect"));
}

/// A small glue VM: both APIs, a two-race catalog, and a frame recording the status events.
fn host_vm() -> UiScript {
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.install_glue_create_api().unwrap();
    let race = |id: u8, name: &str| GlueCreateRace {
        id,
        names: [name.into(), name.into()],
        file: name.into(),
        enabled: true,
        ..Default::default()
    };
    script.set_glue_create_catalog(GlueCreateCatalog {
        races: vec![race(1, "Human"), race(2, "Orc")],
        classes: vec![GlueCreateClass {
            id: 1,
            names: ["Warrior".into(), "Warrior".into()],
            file: "WARRIOR".into(),
            enabled: true,
            roles: roles(1),
        }],
        valid: vec![(1, 1), (2, 1)],
        dials: HashMap::from([((2, 1), [4, 4, 4, 4, 4])]),
        backgrounds: HashMap::new(),
        names: HashMap::new(),
    });
    script
        .run(
            "EVENTS = {} local f = CreateFrame('Frame') \
             for _, e in ipairs({'OPEN_STATUS_DIALOG', 'UPDATE_STATUS_DIALOG', \
             'CLOSE_STATUS_DIALOG', 'SET_GLUE_SCREEN'}) do f:RegisterEvent(e) end \
             f:SetScript('OnEvent', function(_, e, a, b) \
             table.insert(EVENTS, e..':'..tostring(a)..':'..tostring(b)) end)",
        )
        .unwrap();
    script
}

fn host_app() -> (App, crossbeam_channel::Receiver<crate::net::CharRequest>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
        .init_state::<ClientState>()
        .init_resource::<GluePreview>()
        .init_resource::<crate::app_core::state::Roster>()
        .init_resource::<StockCreate>()
        .insert_resource(crate::net::CharPick(tx))
        .add_message::<crate::net::WrathCharActionResultMessage>()
        .add_systems(Update, drive_stock_create);
    app.insert_non_send_resource(host_vm());
    (app, rx)
}

fn events(app: &mut App) -> String {
    app.world_mut()
        .non_send_resource::<UiScript>()
        .eval::<String>("local s = table.concat(EVENTS, ' ') EVENTS = {} return s")
        .unwrap()
}

/// `CreateCharacter` sends the selection with the typed name and opens the status dialog; a
/// failure updates it with the result's text, a success records the character, closes it and
/// returns to the list. The booth wears the VM's selection.
#[test]
fn the_host_sends_the_create_and_answers_its_result() {
    let (mut app, rx) = host_app();
    {
        let mut script = app.world_mut().non_send_resource_mut::<UiScript>();
        script.set_glue_create_selection(GlueCreateSelection {
            race: 2,
            class: 1,
            sex: 1,
            dials: [1, 2, 3, 0, 1],
            facing: 0.5,
        });
        script.run("CreateCharacter('Thrallson')").unwrap();
    }
    app.update();
    let Ok(crate::net::CharRequest::Create(req)) = rx.try_recv() else {
        panic!("one create request");
    };
    assert_eq!(
        (req.name.as_str(), req.race, req.class, req.gender, req.face),
        ("Thrallson", 2, 1, 1, 2)
    );
    assert!(events(&mut app).starts_with("OPEN_STATUS_DIALOG:CANCEL:"));
    let preview = app.world().resource::<GluePreview>();
    assert_eq!(preview.yaw, 0.5);
    assert!(matches!(preview.look, Some(GlueLook::Create(l)) if l.race == 2 && l.face == 2));

    // A refusal: the dialog shows the result's key (no GlueStrings here) with Okay.
    app.world_mut()
        .write_message(crate::net::WrathCharActionResultMessage {
            action: CharAction::Create,
            code: wrathplus_protocol::wrath_world::CHAR_CREATE_SUCCESS + 1,
        });
    app.update();
    assert!(
        events(&mut app).starts_with("UPDATE_STATUS_DIALOG:"),
        "a failure updates"
    );

    // A second try succeeds: recorded, closed, back to the list.
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run("CreateCharacter('Thrallson')")
        .unwrap();
    app.update();
    let _ = rx.try_recv();
    let _ = events(&mut app);
    app.world_mut()
        .write_message(crate::net::WrathCharActionResultMessage {
            action: CharAction::Create,
            code: wrathplus_protocol::wrath_world::CHAR_CREATE_SUCCESS,
        });
    app.update();
    assert_eq!(
        events(&mut app),
        "CLOSE_STATUS_DIALOG:nil:nil SET_GLUE_SCREEN:charselect:nil"
    );
}
