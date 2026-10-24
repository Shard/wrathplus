//! The stock glue on the installed data.

use super::*;
use wrathplus_ui::script::glue::GlueRequest;
use wrathplus_ui::script::ScriptValue;

/// The stock glue on the installed data: GlueXML loads clean, the engine's first screen shows
/// AccountLogin with its boxes, and the stock Login button's handler queues the login with the
/// typed account and password. Skips without an install.
#[test]
fn the_stock_account_login_opens_and_submits_on_the_installed_data() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.set_screen_size(1600.0, 1000.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    // GlueParent's own start: FRAMES_LOADED, then the engine's first screen.
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![ScriptValue::Str("login".into())]);
    script.tick(0.1);
    let shown = script
        .eval::<String>(
            "local function v(f) return f and (f:IsVisible() and 'v' or (f:IsShown() and 's' or 'h')) or 'nil' end \
             return table.concat({v(GlueParent), v(AccountLogin), v(AccountLoginUI), \
             v(AccountLoginAccountEdit), v(AccountLoginPasswordEdit), \
             tostring(GetCurrentGlueScreenName())}, ' ')",
        )
        .unwrap();
    assert_eq!(
        shown, "v v v v v login",
        "GlueParent AccountLogin AccountLoginUI account password screen"
    );
    // AzerothBench's Server field (`assets/ui/glue/ServerField.xml`): it shows the address,
    // and leaving it after an edit asks the host to keep the new one.
    script.set_glue_realmlist("127.0.0.1", false);
    script.fire_event("BENILLA_REALMLIST_CHANGED", vec![]);
    let field = |script: &UiScript| {
        script
            .eval::<String>(
                "return tostring(AccountLoginServerEdit:IsVisible())..' '..\
                 tostring(not AccountLoginServerEdit.pinned)..' '..\
                 AccountLoginServerEdit:GetText()..' '..AccountLoginServerEditLabel:GetText()",
            )
            .unwrap()
    };
    assert_eq!(field(&script), "1 true 127.0.0.1 Server");
    script
        .run(
            "AccountLoginServerEdit:SetFocus() AccountLoginServerEdit:SetText('10.0.0.5:3724') \
             AccountLoginServerEdit:ClearFocus()",
        )
        .unwrap();
    assert_eq!(
        script.take_glue_requests(),
        vec![GlueRequest::SetRealmlist("10.0.0.5:3724".into())]
    );
    // The host keeps it and says so; a launch pin shows, disabled.
    script.set_glue_realmlist("10.0.0.5:3724", false);
    script.fire_event("BENILLA_REALMLIST_CHANGED", vec![]);
    assert_eq!(field(&script), "1 true 10.0.0.5:3724 Server");
    script.set_glue_realmlist("vps.example:3724", true);
    script.fire_event("BENILLA_REALMLIST_CHANGED", vec![]);
    assert_eq!(
        field(&script),
        "1 false vps.example:3724 Server (set by the launch)"
    );
    script
        .run(
            "AccountLoginServerEdit:SetFocus() AccountLoginServerEdit:SetText('x') \
             AccountLoginServerEdit:ClearFocus()",
        )
        .unwrap();
    assert!(
        script.take_glue_requests().is_empty(),
        "a pinned field sends nothing"
    );
    assert!(
        script.errors().is_empty(),
        "script errors: {:?}",
        script.errors()
    );
    script.set_glue_realmlist("127.0.0.1", false);
    script.fire_event("BENILLA_REALMLIST_CHANGED", vec![]);
    // `--fill-only`: the launch's credentials wait in the boxes; nothing submits until Login.
    script.fill_login("Bob", "hunter2").unwrap();
    assert_eq!(
        script
            .eval::<(String, String)>(
                "return AccountLoginAccountEdit:GetText(), AccountLoginPasswordEdit:GetText()"
            )
            .unwrap(),
        ("Bob".to_string(), "hunter2".to_string())
    );
    assert!(
        script.take_glue_requests().is_empty(),
        "a fill does not log in"
    );
    script.run("AccountLogin_Login()").unwrap();
    assert_eq!(
        script.take_glue_requests(),
        vec![
            GlueRequest::Login {
                account: "Bob".into(),
                password: "hunter2".into()
            },
            // Save Account Name unticked: the saved name clears.
            GlueRequest::SaveAccountName(String::new()),
        ]
    );
    assert_eq!(
        script
            .eval::<String>("return AccountLoginPasswordEdit:GetText()")
            .unwrap(),
        "",
        "the stock handler clears the password box"
    );
    // The realm list the engine opens after the logon: the stock RealmList over a seeded
    // list, and a double-click on a row is `ChangeRealm(tab, row)`.
    let realm = |name: &str| wrathplus_ui::script::glue::GlueRealm {
        name: name.into(),
        num_characters: 1,
        ..Default::default()
    };
    script.set_glue_realms(
        vec![wrathplus_ui::script::glue::GlueRealmTab {
            name: String::new(),
            realms: vec![realm("AzerothCore"), realm("Second")],
        }],
        1,
        5.0,
    );
    script.fire_event("OPEN_REALM_LIST", vec![]);
    script.tick(0.1);
    assert_eq!(
        script
            .eval::<String>(
                "return tostring(RealmList:IsVisible())..' '..RealmListRealmButton1:GetText()..' '..\
                 tostring(RealmListRealmButton3:IsShown())"
            )
            .unwrap(),
        // 12340's visibility queries answer 1 or nil.
        "1 AzerothCore nil"
    );
    script
        .run("RealmSelectButton_OnDoubleClick(RealmListRealmButton2, 2)")
        .unwrap();
    assert_eq!(
        script.take_glue_requests(),
        vec![GlueRequest::ChangeRealm {
            category: 1,
            index: 2
        }]
    );
    // Character select over seeded rows: the stock list shows them, a click selects the row
    // and asks the stage for it, a double-click enters the world.
    script.set_glue_characters(
        vec![
            wrathplus_ui::script::glue::GlueCharacter {
                name: "Benchone".into(),
                race: "Human".into(),
                class: "Warrior".into(),
                level: 7,
                zone: "Elwynn Forest".into(),
                stage: "Human".into(),
                ..Default::default()
            },
            wrathplus_ui::script::glue::GlueCharacter {
                name: "Grunt".into(),
                race: "Orc".into(),
                class: "Death Knight".into(),
                level: 55,
                zone: "Acherus".into(),
                stage: "DeathKnight".into(),
                ..Default::default()
            },
        ],
        "Orc",
    );
    // Leaving login fades AccountLoginUI out first (`GlueScreenExit` → `GlueFrameFadeOut(...,
    // GoToPendingGlueScreen)`), so the switch lands once the fade has run.
    script.fire_event(
        "SET_GLUE_SCREEN",
        vec![ScriptValue::Str("charselect".into())],
    );
    for _ in 0..30 {
        script.tick(0.1);
    }
    script.fire_event("CHARACTER_LIST_UPDATE", vec![]);
    script.tick(0.1);
    assert_eq!(
        script
            .eval::<String>(
                "return tostring(CharacterSelect:IsVisible())..' '..GetCurrentGlueScreenName()..' '..\
                 CharSelectCharacterButton1ButtonTextName:GetText()..' '..\
                 tostring(CharSelectCharacterButton2:IsShown())..' '..tostring(CharSelectCharacterButton3:IsShown())"
            )
            .unwrap(),
        "1 charselect Benchone 1 nil"
    );
    script.take_glue_requests();
    script
        .run("CharacterSelectButton_OnClick(CharSelectCharacterButton2)")
        .unwrap();
    let requests = script.take_glue_requests();
    assert!(
        requests.contains(&GlueRequest::SelectCharacter(2)),
        "{requests:?}"
    );
    assert!(
        requests.contains(&GlueRequest::SetBackground(
            r"Interface\Glues\Models\UI_DeathKnight\UI_DeathKnight.m2".into()
        )),
        "{requests:?}"
    );
    script
        .run("CharacterSelectButton_OnDoubleClick(CharSelectCharacterButton2)")
        .unwrap();
    assert!(script
        .take_glue_requests()
        .contains(&GlueRequest::EnterWorld));
    let errors = script.take_errors();
    assert!(errors.is_empty(), "{errors:#?}");
}

/// Character select takes down the login's connect status: Shannon's 10-24 session kept
/// "Handshaking" up over the character list, the realm list (which closes it) never opening for a
/// remembered realm.
#[test]
fn character_select_closes_the_connect_status_dialog() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.set_screen_size(1600.0, 1000.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![ScriptValue::Str("login".into())]);
    script.fire_event(
        "OPEN_STATUS_DIALOG",
        vec![
            ScriptValue::Str("CANCEL".into()),
            ScriptValue::Str("Connecting".into()),
        ],
    );
    script.fire_event(
        "UPDATE_STATUS_DIALOG",
        vec![ScriptValue::Str("Handshaking".into())],
    );
    let dialog = |script: &UiScript| {
        script
            .eval::<(bool, String)>("return GlueDialog:IsShown(), GlueDialogText:GetText()")
            .unwrap()
    };
    assert_eq!(dialog(&script), (true, "Handshaking".to_string()));

    show_stock_select(&mut script);
    assert!(!dialog(&script).0, "the status dialog is down");
    // Login to character select fades the login out first (`GlueScreenExit`, `LOGIN_FADE_OUT`).
    for _ in 0..20 {
        script.tick(0.1);
    }
    assert_eq!(script.glue_current_screen().as_deref(), Some("charselect"));
    assert!(
        script.errors().is_empty(),
        "script errors: {:?}",
        script.errors()
    );
}

/// Stock character select with two characters on a connected realm: Create New Character shows
/// (`UpdateCharacterList` shows it only while `IsConnectedToServer`), with Enter World, the stage's
/// rotate buttons, Delete and Back, and the realm label is the realm's name (`GetServerName`), not
/// the logon address. Shannon's 10-24 session had neither: the glue never said it was connected.
#[test]
fn character_select_shows_create_and_names_the_realm() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.set_screen_size(1600.0, 1000.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![ScriptValue::Str("login".into())]);
    let character = |name: &str| wrathplus_ui::script::glue::GlueCharacter {
        name: name.into(),
        race: "Human".into(),
        class: "Warrior".into(),
        level: 7,
        zone: "Elwynn Forest".into(),
        stage: "Human".into(),
        ..Default::default()
    };
    script.set_glue_characters(vec![character("Benchone"), character("Locktest")], "Human");
    // What `enter_stock_select` now tells the glue: connected, to the realm by name and type.
    script.set_glue_connection(true, "AzerothCore");
    script.set_glue_server_type(true, false);
    show_stock_select(&mut script);
    for _ in 0..20 {
        script.tick(0.1);
    }
    // Connected, `CharacterSelect_OnShow` asks for the list (`GetCharacterListUpdate`); the
    // host's answer when it lands.
    script.fire_event("UPDATE_SELECTED_CHARACTER", vec![ScriptValue::Number(1.0)]);
    script.fire_event("CHARACTER_LIST_UPDATE", vec![]);
    let shown: String = script
        .eval(
            "local out = {} for _, n in ipairs({'CharSelectCreateCharacterButton', \
             'CharSelectEnterWorldButton', 'CharacterSelectRotateLeft', 'CharacterSelectRotateRight', \
             'CharacterSelectDeleteButton', 'CharacterSelectBackButton'}) do \
             out[#out + 1] = n .. '=' .. tostring(_G[n]:IsVisible()) end \
             return table.concat(out, ' ')",
        )
        .unwrap();
    assert_eq!(
        shown,
        "CharSelectCreateCharacterButton=1 CharSelectEnterWorldButton=1 \
         CharacterSelectRotateLeft=1 CharacterSelectRotateRight=1 \
         CharacterSelectDeleteButton=1 CharacterSelectBackButton=1"
    );
    assert_eq!(
        script
            .eval::<String>("return CharSelectRealmName:GetText()")
            .unwrap(),
        format!(
            "AzerothCore {}",
            script.eval::<String>("return PVP_PARENTHESES").unwrap()
        )
    );
    // Not connected, as the glue answered before: no Create New Character.
    script.set_glue_connection(false, "127.0.0.1");
    script.fire_event("CHARACTER_LIST_UPDATE", vec![]);
    assert!(!script
        .eval::<bool>("return CharSelectCreateCharacterButton:IsVisible() == 1")
        .unwrap());
    assert!(
        script.errors().is_empty(),
        "script errors: {:?}",
        script.errors()
    );
}

/// Shannon's 10-24 session: a click on the second row, then Enter World, logged in the first
/// character. Through the host and the stock CharacterSelect Lua, frame by frame: the click's
/// `SelectCharacter(2)` moves the pick (1-based, as stock passes it), no list replay re-selects the
/// old row, and Enter World sends the second character's guid.
#[test]
fn character_select_enters_the_clicked_character() {
    use crate::app_core::state::{ClientState, Roster};
    use crate::char_select::test_character;
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.set_screen_size(1600.0, 1000.0);
    assert!(load_glue_ui(&script).is_empty());
    script.fire_event("FRAMES_LOADED", vec![]);
    script.set_glue_connection(true, "AzerothCore");

    let (login_tx, _login_rx) = crossbeam_channel::unbounded();
    let (realm_tx, _realm_rx) = crossbeam_channel::unbounded();
    let (pick_tx, pick_rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
        .insert_state(ClientState::CharSelect)
        .init_resource::<crate::app_core::ui_flags::LoginIntent>()
        .insert_resource(crate::net::LoginSubmit(login_tx))
        .insert_resource(crate::net::LoginAbandon(Default::default()))
        .insert_resource(crate::realmlist::Realmlist::unpinned("127.0.0.1"))
        .init_resource::<crate::app_core::ui_flags::Realms>()
        .insert_resource(crate::net::RealmChoice(realm_tx))
        .init_resource::<Roster>()
        .insert_resource(crate::net::CharPick(pick_tx))
        .init_resource::<crate::portrait::GluePreview>()
        .add_message::<crate::net::LoginStageMessage>()
        .add_message::<crate::net::LoginFailedMessage>()
        .add_message::<AppExit>()
        .add_message::<crate::net::CharListMessage>()
        .add_systems(Update, stock_glue);
    app.insert_non_send_resource(script);
    {
        let mut roster = app.world_mut().resource_mut::<Roster>();
        roster.chars = vec![
            test_character(101, "Benchone"),
            test_character(102, "Locktest"),
        ];
        roster.select(Some(0));
    }
    let frames = |app: &mut App, n: usize| {
        for _ in 0..n {
            app.update();
            app.world_mut()
                .non_send_resource_mut::<UiScript>()
                .tick(0.1);
        }
    };
    let list_arrives = |app: &mut App| {
        app.world_mut().write_message(crate::net::CharListMessage {
            characters: vec![],
            realm: None,
        });
    };
    frames(&mut app, 2);
    show_stock_select(&mut app.world_mut().non_send_resource_mut::<UiScript>());
    frames(&mut app, 20);
    // The list `CharacterSelect_OnShow` asked for lands.
    list_arrives(&mut app);
    app.world_mut().resource_mut::<Roster>().set_changed();
    frames(&mut app, 3);

    // The row click, as the stock button runs it.
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run("CharacterSelectButton_OnClick(CharSelectCharacterButton2)")
        .unwrap();
    frames(&mut app, 6);
    assert_eq!(app.world().resource::<Roster>().selected(), Some(1));
    assert_eq!(
        app.world()
            .non_send_resource::<UiScript>()
            .eval::<i64>("return CharacterSelect.selectedIndex")
            .unwrap(),
        2,
        "stock's selectedIndex agrees, and stays"
    );
    // A list update after the click (a fresh list, a row's data moved): the selection holds.
    list_arrives(&mut app);
    app.world_mut().resource_mut::<Roster>().chars[0].level += 1;
    frames(&mut app, 6);
    assert_eq!(app.world().resource::<Roster>().selected(), Some(1));
    assert_eq!(
        app.world()
            .non_send_resource::<UiScript>()
            .eval::<i64>("return CharacterSelect.selectedIndex")
            .unwrap(),
        2,
        "and across a list update"
    );
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run("CharacterSelect_EnterWorld()")
        .unwrap();
    frames(&mut app, 2);
    let sent: Vec<_> = pick_rx.try_iter().collect();
    assert!(
        matches!(sent[..], [crate::net::CharRequest::Enter(102)]),
        "{} requests, not one Enter(102)",
        sent.len()
    );
    let script = app.world().non_send_resource::<UiScript>();
    assert!(
        script.errors().is_empty(),
        "script errors: {:?}",
        script.errors()
    );
}
