//! Character select → Create New Character on the installed GlueXML, through the app's own
//! seeding: `CharacterSelect_SelectCharacter` calls `SetGlueScreen("charcreate")`, which shows
//! `CharacterCreate` inside that Lua call, so its `OnShow` must find the catalog already seeded
//! (a live session raised `CharacterCreate.lua:257: bad argument #1 to 'strupper'`).

use super::*;
use crate::ui_script::glue::load_glue_ui;

/// The glue VM on character select, with no catalog pushed by hand.
fn select_vm() -> UiScript {
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.install_glue_create_api().unwrap();
    script.set_screen_size(1600.0, 1000.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![s("charselect")]);
    script.tick(0.1);
    script
}

/// The app's seeding and create systems over the installed catalogs.
fn select_app() -> Option<(App, crossbeam_channel::Receiver<crate::net::CharRequest>)> {
    let data = wrathplus_formats::wow_data()?;
    let mut chain = wrathplus_formats::open_chain(&data).ok()?;
    let catalog = CharCreateCatalog::load(&mut chain).expect("CharCreate catalog");
    let classes = wrathplus_formats::load_chr_classes(&mut chain).expect("ChrClasses");
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
        .init_state::<ClientState>()
        .init_resource::<GluePreview>()
        .init_resource::<crate::app_core::state::Roster>()
        .init_resource::<StockCreate>()
        .insert_resource(crate::entities::CharCreate(catalog))
        .insert_resource(crate::chr_classes::ChrClassTable(classes))
        .insert_resource(crate::net::CharPick(tx))
        .add_message::<crate::net::WrathCharActionResultMessage>()
        .add_systems(Update, (seed_stock_create, drive_stock_create).chain());
    app.insert_non_send_resource(select_vm());
    app.update();
    Some((app, rx))
}

/// Every race and class button is shown with its atlas cell (`RACE_ICON_TCOORDS`,
/// `CLASS_ICON_TCOORDS`), the labels name the selection, and no Lua error is raised; Accept then
/// sends the selection (its byte order against AzerothCore's `HandleCharCreateOpcode` read is
/// `wrath_world`'s `character_create_has_exact_wrath_wire_order_and_client_header`).
#[test]
fn create_new_character_from_select_shows_every_race_and_class_button() {
    let Some((mut app, rx)) = select_app() else {
        eprintln!("skipped: no installed client");
        return;
    };
    let script = app.world().non_send_resource::<UiScript>();
    script
        .run("CharacterSelect_SelectCharacter(CharacterSelect.createIndex)")
        .unwrap();
    assert!(script.errors().is_empty(), "{:#?}", script.errors());
    let report: String = script
        .eval(
            r#"
            local out = {}
            local function cell(button, want)
                local a, b, c, d = _G[button:GetName().."NormalTexture"]:GetTexCoord()
                return math.abs(a - want[1]) < 1e-4 and math.abs(d - want[4]) < 1e-4
                    and button:IsShown() == 1
            end
            local gender = GetSelectedSex() == SEX_MALE and "MALE" or "FEMALE"
            local races = {GetAvailableRaces()}
            table.insert(out, "races="..CharacterCreate.numRaces)
            for i = 1, CharacterCreate.numRaces do
                local file = strupper(races[i * 3 - 1].."_"..gender)
                if not cell(_G["CharacterCreateRaceButton"..i], RACE_ICON_TCOORDS[file]) then
                    table.insert(out, "bad race "..i)
                end
            end
            local classes = {GetAvailableClasses()}
            table.insert(out, "classes="..CharacterCreate.numClasses)
            for i = 1, CharacterCreate.numClasses do
                local file = strupper(classes[i * 3 - 1])
                if not cell(_G["CharacterCreateClassButton"..i], CLASS_ICON_TCOORDS[file]) then
                    table.insert(out, "bad class "..i)
                end
            end
            local race = GetNameForRace()
            local class = GetSelectedClass()
            local race_button = _G["CharacterCreateRaceButton"..GetSelectedRace()]
            local _, _, class_index = GetSelectedClass()
            local class_button = _G["CharacterCreateClassButton"..class_index]
            table.insert(out, tostring(CharacterCreateRaceLabel:GetText() == race))
            table.insert(out, tostring(CharacterCreateClassLabel:GetText() == class))
            table.insert(out, tostring(race_button:GetChecked() == 1))
            table.insert(out, tostring(class_button:GetChecked() == 1))
            -- The two label icons show their cell, not the whole atlas sheet.
            local _, race_file = GetNameForRace()
            local _, class_file = GetSelectedClass()
            local function icon(texture, want)
                local a, _, _, d = texture:GetTexCoord()
                return math.abs(a - want[1]) < 1e-4 and math.abs(d - want[4]) < 1e-4
            end
            table.insert(out, tostring(icon(CharacterCreateRaceIcon,
                RACE_ICON_TCOORDS[strupper(race_file).."_"..gender])))
            table.insert(out, tostring(icon(CharacterCreateClassIcon,
                CLASS_ICON_TCOORDS[strupper(class_file)])))
            return table.concat(out, " ")
        "#,
        )
        .unwrap();
    assert_eq!(report, "races=10 classes=10 true true true true true true");

    let sel = script.glue_create_selection();
    script
        .run("CharacterCreateNameEdit:SetText('Glueone') CharCreateOkayButton:Click()")
        .unwrap();
    app.update();
    let Ok(crate::net::CharRequest::Create(req)) = rx.try_recv() else {
        panic!("Accept sends one create");
    };
    assert_eq!(
        (req.name.as_str(), req.race, req.class, req.gender),
        ("Glueone", sel.race, sel.class, sel.sex)
    );
    assert_eq!(
        [
            req.skin,
            req.face,
            req.hair_style,
            req.hair_color,
            req.facial_hair
        ],
        sel.dials
    );
}

/// Every logout builds a fresh glue VM: create works there too, seeded by the same system. A
/// level-55 character arriving on the roster re-seeds the Death Knight gate into the live VM.
#[test]
fn a_post_logout_vm_is_seeded_and_the_roster_reseeds_the_death_knight() {
    let Some((mut app, _rx)) = select_app() else {
        eprintln!("skipped: no installed client");
        return;
    };
    let dk_enabled = |app: &mut App| {
        app.world_mut()
            .non_send_resource::<UiScript>()
            .eval::<bool>(
                "local c = {GetAvailableClasses()} \
                 for i = 1, #c, 3 do if c[i + 1] == 'DEATHKNIGHT' then return c[i + 2] == 1 end end \
                 return false",
            )
            .unwrap()
    };
    assert!(!dk_enabled(&mut app), "no level-55 character yet");
    let mut veteran = crate::char_select::test_character(9, "Veteran");
    veteran.level = 55;
    app.world_mut()
        .resource_mut::<crate::app_core::state::Roster>()
        .chars
        .push(veteran);
    app.update();
    assert!(dk_enabled(&mut app), "the roster change re-seeds");

    // The logout's fresh VM, on character select again.
    app.insert_non_send_resource(select_vm());
    app.update();
    let script = app.world().non_send_resource::<UiScript>();
    script
        .run("CharacterSelect_SelectCharacter(CharacterSelect.createIndex)")
        .unwrap();
    assert!(script.errors().is_empty(), "{:#?}", script.errors());
    let shown: String = script
        .eval("return CharacterCreate.numRaces..' '..CharacterCreate.numClasses")
        .unwrap();
    assert_eq!(shown, "10 10");
    assert!(dk_enabled(&mut app), "the new VM carries the gate");
}
